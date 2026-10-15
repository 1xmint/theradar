# SPDX-License-Identifier: Apache-2.0
"""Protected coordinated upgrade using temporary files and fake service calls."""

from contextlib import contextmanager, ExitStack
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("refusal_upgrade", Path(__file__).with_name("upgrade-paper-refusals.py"))
upgrade = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upgrade)


def archive(entries):
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w:") as output:
        for name, raw, kind, *link in entries:
            member = tarfile.TarInfo(name)
            member.size, member.type = len(raw), kind
            member.linkname = link[0] if link else ""
            output.addfile(member, io.BytesIO(raw))
    return stream.getvalue()


class UpgradeTests(unittest.TestCase):
    @contextmanager
    def context(self):
        with tempfile.TemporaryDirectory() as temporary, ExitStack() as patches:
            root = Path(temporary)
            library, state = (root / path.lstrip("/") for path in (upgrade.LIBRARY, upgrade.STATE))
            library.mkdir(parents=True)
            state.mkdir(parents=True)
            original = {name:("old-" + name).encode() for name in upgrade.OLD}
            for name, raw in original.items(): (library / name).write_bytes(raw)
            (state / "orders.jsonl").write_bytes(b"original-history\n")
            (state / ".lock").write_bytes(b"retained-lock")
            (state / "genesis-complete.json").write_text(json.dumps(dict(genesis="initialized",
                account_id=upgrade.ACCOUNT, new_order_authority=False)))
            case = types.SimpleNamespace(root=root, library=library, state=state, original=original, reader=b"new-reader", calls=[])
            contents = [('paper-control.py', b'READER_HASH = "' + upgrade.digest(case.reader).encode() + b'"\n', tarfile.REGTYPE),
                ('paper-supervisor.py', b'new-supervisor', tarfile.REGTYPE)]
            case.entries, case.raw = contents, archive(contents)
            def run(argv):
                case.calls.append(argv)
                if hasattr(case, "during"): case.during(argv)
                return types.SimpleNamespace(stdout=getattr(case, "active", "inactive"))
            patches.enter_context(patch.object(upgrade.os, "geteuid", return_value=0))
            patches.enter_context(patch.object(upgrade, "protected"))
            original_lstat = Path.lstat
            patches.enter_context(patch.object(Path, "lstat", lambda path:types.SimpleNamespace(
                st_uid=0, st_mode=original_lstat(path).st_mode)))
            patches.enter_context(patch.object(upgrade, "OLD", {name:upgrade.digest(raw) for name, raw in original.items()}))
            patches.enter_context(patch.object(upgrade, "READER_HASH", upgrade.digest(case.reader)))
            case.instance = upgrade.Upgrade(root, run)
            yield case

    def apply(self, case):
        return case.instance.apply(case.raw, upgrade.digest(case.raw), case.reader)

    def test_upgrade_backs_up_known_payload_preserves_history_and_starts_paused_before_timer(self):
        with self.context() as case:
            self.assertEqual(self.apply(case), dict(upgraded=True, original_prefix_preserved=True,
                handover_enabled=False, new_order_authority=False))
            backup = case.library / "before-refusal-upgrade-70aabc9"
            for name, raw in case.original.items(): self.assertEqual((backup / name).read_bytes(), raw)
            self.assertEqual((case.library / "radar").read_bytes(), case.reader)
            self.assertEqual((case.state / "orders.jsonl").read_bytes(), b"original-history\n")
            self.assertEqual((case.state / ".lock").read_bytes(), b"retained-lock")
            self.assertFalse((case.state / "control.json").exists())
            tick = ["/usr/bin/systemctl", "start", "radar-paper-supervisor.service"]
            timer = ["/usr/bin/systemctl", "start", "radar-paper-supervisor.timer"]
            self.assertLess(case.calls.index(tick), case.calls.index(timer))
            with self.assertRaises(ValueError): self.apply(case)

    def test_root_hash_and_installed_pin_refusals_have_no_service_effect(self):
        for wrong in ("root", "archive", "reader", "installed"):
            with self.subTest(wrong=wrong), self.context() as case:
                if wrong == "installed": (case.library / "paper-control.py").write_bytes(b"changed")
                with patch.object(upgrade.os, "geteuid", return_value=1000 if wrong == "root" else 0):
                    with self.assertRaises(ValueError):
                        case.instance.apply(case.raw, "0" * 64 if wrong == "archive" else upgrade.digest(case.raw),
                            b"wrong" if wrong == "reader" else case.reader)
                self.assertEqual(case.calls, [])

    def test_archive_rejects_duplicates_links_extra_missing_and_wrong_reader_pin(self):
        with self.context() as case:
            for entries in (case.entries * 2, case.entries[:1], case.entries + [("other", b"", tarfile.REGTYPE)],
                    [("paper-supervisor.py", case.entries[0][1], tarfile.REGTYPE),
                        ("paper-control.py", b"", tarfile.SYMTYPE, "paper-supervisor.py")],
                    [("paper-control.py", b"wrong-pin", tarfile.REGTYPE), case.entries[1]]):
                raw = archive(entries)
                with self.assertRaises(ValueError): upgrade.payload(raw, upgrade.digest(raw))

    def test_configured_handover_and_foreign_genesis_cannot_be_reinitialized(self):
        for wrong in ("control.json", "effective-policy-1.json", "genesis-complete.json"):
            with self.context() as case:
                (case.state / wrong).write_bytes(b"{}")
                original = (case.state / wrong).read_bytes()
                with self.assertRaises(ValueError): self.apply(case)
                self.assertEqual(case.calls, [])
                self.assertEqual((case.state / wrong).read_bytes(), original)

    def test_writable_installed_payload_cannot_pass_pin_check(self):
        with self.context() as case:
            (case.library / "paper-control.py").chmod(0o666)
            with self.assertRaises(ValueError): self.apply(case)
            self.assertEqual(case.calls, [])

    def test_directory_metadata_and_exclusive_backups_are_enforced(self):
        with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_uid=0, st_mode=0o40755)):
            upgrade.protected(Path("/protected"))
        for uid, mode in ((1, 0o40755), (0, 0o40777), (0, 0o120777)):
            with patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_uid=uid, st_mode=mode)):
                with self.assertRaises(ValueError): upgrade.protected(Path("/protected"))
        with self.context() as case:
            (case.library / "before-refusal-upgrade-70aabc9").mkdir()
            with self.assertRaises(ValueError): self.apply(case)
            self.assertEqual(case.calls, [])
            existing = case.library / "retain"
            existing.write_bytes(b"retained")
            with self.assertRaises(FileExistsError): upgrade.publish(existing, b"new", 0o644)
            self.assertEqual(existing.read_bytes(), b"retained")

    def test_active_tick_and_racing_owner_grant_prevent_publication(self):
        for wrong in ("active", "owner"):
            with self.context() as case:
                if wrong == "active": case.active = "active"
                else:
                    def during(argv):
                        if "radar-paper-supervisor.timer" in argv: (case.state / "control.json").write_bytes(b"grant")
                    case.during = during
                with self.assertRaises(ValueError): self.apply(case)
                for name, raw in case.original.items(): self.assertEqual((case.library / name).read_bytes(), raw)
                self.assertFalse((case.library / "before-refusal-upgrade-70aabc9").exists())

    def test_corrupt_history_or_markers_retains_backups_and_never_starts_timer(self):
        for wrong in ("orders.jsonl", ".lock", "genesis-complete.json"):
            with self.context() as case:
                def during(argv):
                    if argv == ["/usr/bin/systemctl", "start", "radar-paper-supervisor.service"]:
                        (case.state / wrong).write_bytes(b"changed")
                case.during = during
                with self.assertRaises(ValueError): self.apply(case)
                self.assertTrue((case.library / "before-refusal-upgrade-70aabc9").is_dir())
                self.assertNotIn(["/usr/bin/systemctl", "start", "radar-paper-supervisor.timer"], case.calls)
                with self.assertRaises(ValueError): self.apply(case)
