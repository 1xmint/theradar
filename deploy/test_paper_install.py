# SPDX-License-Identifier: Apache-2.0
"""Private installation boundaries with temporary files and fake systemd/users."""

from contextlib import contextmanager, ExitStack
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import sys
import tarfile
import tempfile
import types
import unittest
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


setup = load("setup-paper-autonomy")
provision = load("provision-paper-supervisor")
admin = load("radar-paper-autonomy-admin")


def archive(entries=None):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:") as stream:
        for name, value, kind, *link in entries or [(name, name.encode(), tarfile.REGTYPE) for name in setup.FILES]:
            entry = tarfile.TarInfo(name)
            entry.size, entry.type = len(value), kind
            entry.linkname = link[0] if link else ""
            stream.addfile(entry, io.BytesIO(value))
    return output.getvalue()


class InstallationTests(unittest.TestCase):
    @contextmanager
    def context(self):
        with tempfile.TemporaryDirectory() as temporary, ExitStack() as patches:
            root = Path(temporary)
            case = types.SimpleNamespace(root=root, calls=[], raw=archive(), reader=b"verified-reader", tasks="1", active="inactive")
            for directory in ("/etc/radar", "/etc/systemd/system", "/etc/sudoers.d", "/usr/local/bin", "/usr/local/sbin",
                    "/usr/local/lib/radar", setup.STATE):
                (root / directory.lstrip("/")).mkdir(parents=True, exist_ok=True)
            (root / "etc/radar/alpaca-paper.env").write_bytes(b"private-sentinel-never-read")
            (root / setup.BRIDGE.lstrip("/")).write_bytes(b"old-bridge")
            (root / "usr/local/bin/radar-codex").write_bytes(b"fixed-client")
            state = root / setup.STATE.lstrip("/")
            case.journal = b"retained-journal-prefix\n"
            (state / "orders.jsonl").write_bytes(case.journal)
            (state / ".lock").write_bytes(b"original-lock")
            (state / "genesis-complete.json").write_text(json.dumps(dict(genesis="initialized", account_id=setup.ACCOUNT, new_order_authority=False)))
            case.state = state
            def run(argv):
                case.calls.append(argv)
                if hasattr(case, "during_command"): case.during_command(argv)
                value = case.tasks if "--property=TasksCurrent" in argv else case.active if "--property=ActiveState" in argv else ""
                return types.SimpleNamespace(stdout=value)
            def user(name):
                if name == "radar-paper-supervisor": raise KeyError(name)
                return types.SimpleNamespace(pw_uid=1000 if name == "guardian" else 997)
            def check_pin(path, expected):
                if setup.digest(setup.read(path)) != expected:
                    raise ValueError("Existing subscription installation differs")
            patches.enter_context(patch.object(setup.os, "geteuid", return_value=0))
            patches.enter_context(patch.object(setup, "protected"))
            patches.enter_context(patch.object(setup, "private_environment"))
            patches.enter_context(patch.object(setup, "pinned", side_effect=check_pin))
            patches.enter_context(patch.object(setup.pwd, "getpwnam", side_effect=user))
            patches.enter_context(patch.object(setup.grp, "getgrnam", side_effect=KeyError))
            for name, value in (("READER_HASH", setup.digest(case.reader)), ("BRIDGE_HASH", setup.digest(b"old-bridge")), ("CLIENT_HASH", setup.digest(b"fixed-client"))):
                patches.enter_context(patch.object(setup, name, value))
            case.instance = setup.Setup(root, run)
            yield case

    def install(self, case):
        return case.instance.install(case.raw, setup.digest(case.raw), case.reader)

    def test_full_install_preserves_history_secrets_client_and_starts_paused_before_timer(self):
        with self.context() as case:
            result = self.install(case)
            self.assertEqual(result, dict(private_paper_installed=True, original_prefix_preserved=True,
                handover_enabled=False, new_order_authority=False, website_restart_required=True))
            self.assertEqual((case.state / "orders.jsonl").read_bytes(), case.journal)
            self.assertEqual((case.root / "etc/radar/alpaca-paper.env").read_bytes(), b"private-sentinel-never-read")
            self.assertEqual((case.root / "usr/local/bin/radar-codex").read_bytes(), b"fixed-client")
            self.assertEqual((case.root / (setup.BRIDGE + ".before-paper-autonomy").lstrip("/")).read_bytes(), b"old-bridge")
            start = ["/usr/bin/systemctl", "start", "radar-paper-supervisor.service"]
            timer = ["/usr/bin/systemctl", "enable", "--now", "radar-paper-supervisor.timer"]
            self.assertLess(case.calls.index(start), case.calls.index(timer))
            for name in ("radar-paper-research", "radar-context-research"):
                self.assertIn(["/usr/bin/systemctl", "disable", "--now", name + ".timer"], case.calls)
            commands = json.dumps(case.calls)
            self.assertNotIn("radar-paper-initialize", commands)
            self.assertNotIn("radar-deploy", commands)
            self.assertNotIn("alpaca-paper-submit", commands)
            provision_call = next(item for item in case.calls if item[0] == "/usr/bin/systemd-run")
            self.assertIn("User=radar-paper-supervisor", provision_call)
            self.assertNotIn("EnvironmentFile=/etc/radar/alpaca-paper.env", provision_call)
            settings = json.loads((case.root / "etc/radar/alpaca-paper-control.json").read_bytes())
            self.assertEqual(settings["model_sha256"], setup.digest((case.root / "etc/radar/alpaca-paper-model.json").read_bytes()))
            rule = (case.root / setup.RULE.lstrip("/")).read_text()
            self.assertEqual(rule.count("NOPASSWD:"), 1)
            self.assertNotIn("*", rule)
            for action in ("inspect", "start", "stop"): self.assertIn(setup.ADMIN + " " + action, rule)

    def test_bundle_reader_and_root_checks_precede_any_runtime_change(self):
        with self.context() as case:
            with self.assertRaises(ValueError): case.instance.install(case.raw, "0" * 64, case.reader)
            with self.assertRaises(ValueError): case.instance.install(case.raw, setup.digest(case.raw), b"different-reader")
            with patch.object(setup.os, "geteuid", return_value=1000):
                with self.assertRaises(ValueError): self.install(case)
            self.assertEqual(case.calls, [])
        regular = [(name, name.encode(), tarfile.REGTYPE) for name in setup.FILES]
        linked = [(name, b"", tarfile.SYMTYPE, "paper-supervisor.py") if name == "paper-control.py" else
            (name, name.encode(), tarfile.REGTYPE) for name in setup.FILES]
        for entries in ([("../escape", b"x", tarfile.REGTYPE)], linked,
                [*regular, ("paper-control.py", b"replacement", tarfile.REGTYPE)], [("paper-control.py", b"x", tarfile.REGTYPE)]):
            raw = archive(entries)
            with self.assertRaises(ValueError): setup.bundle(raw, setup.digest(raw))

    def test_existing_files_symlinks_and_owner_policy_refuse_without_overwrite(self):
        for target in (setup.LIBRARY, setup.RULE, "/etc/radar/alpaca-paper-model.json", "/var/lib/radar-paper-supervisor",
                setup.FILES["radar-paper-web-ipc.conf"][0]):
            with self.context() as case:
                path = case.instance.path(target)
                path.parent.mkdir(parents=True, exist_ok=True)
                path.symlink_to(case.root / "absent")
                with self.assertRaises(ValueError): self.install(case)
                self.assertTrue(path.is_symlink())
                self.assertEqual(case.calls, [])
        with self.context() as case:
            (case.state / "control.json").write_bytes(b"preserve")
            with self.assertRaises(ValueError): self.install(case)
            self.assertEqual(case.calls, [])

    def test_missing_or_foreign_genesis_never_bootstraps_a_new_journal(self):
        for kind in ("journal", "lock", "genesis", "foreign"):
            with self.context() as case:
                path = case.state / {"journal":"orders.jsonl", "lock":".lock", "genesis":"genesis-complete.json", "foreign":"genesis-complete.json"}[kind]
                if kind == "foreign": path.write_bytes(b"{}")
                else: path.unlink()
                with self.assertRaises((ValueError, FileNotFoundError)): self.install(case)
                self.assertEqual(case.calls, [])

    def test_active_research_or_model_refuses_before_stopping_subscription_or_publishing(self):
        for kind in ("model", "research"):
            with self.context() as case:
                if kind == "model": case.tasks = "2"
                else: case.active = "active"
                with self.assertRaises(ValueError): self.install(case)
                self.assertNotIn(["/usr/bin/systemctl", "stop", "radar-codex.socket"], case.calls)
                self.assertFalse(case.instance.path(setup.LIBRARY).exists())

    def test_changed_prefix_or_markers_refuse_before_enabling_timer_and_preserve_evidence(self):
        for kind in ("prefix", "marker"):
            with self.context() as case:
                def corrupt(argv):
                    if argv == ["/usr/bin/systemctl", "start", "radar-paper-supervisor.service"]:
                        (case.state / ("orders.jsonl" if kind == "prefix" else ".lock")).write_bytes(b"changed")
                case.during_command = corrupt
                with self.assertRaises(ValueError): self.install(case)
                self.assertNotIn(["/usr/bin/systemctl", "enable", "--now", "radar-paper-supervisor.timer"], case.calls)
                self.assertTrue(case.instance.path("/etc/radar/paper-autonomy-origin.json").exists())
                with self.assertRaises(ValueError): self.install(case)

    def test_exclusive_publication_and_private_input_modes(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "file"
            setup.publish(target, b"first", 0o600)
            with self.assertRaises(FileExistsError): setup.publish(target, b"second", 0o600)
            self.assertEqual(target.read_bytes(), b"first")
            with patch.object(setup, "protected"), patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=stat.S_IFREG | 0o600, st_uid=0)):
                setup.private_environment(target)
                setup.pinned(target, setup.digest(b"first"))
                with self.assertRaises(ValueError): setup.pinned(target, "0" * 64)
            for mode, uid in ((stat.S_IFREG | 0o644, 0), (stat.S_IFREG | 0o600, 1000), (stat.S_IFLNK | 0o600, 0)):
                with patch.object(setup, "protected"), patch.object(Path, "lstat", return_value=types.SimpleNamespace(st_mode=mode, st_uid=uid)):
                    with self.assertRaises(ValueError): setup.private_environment(target)

    def test_supervisor_provision_fences_success_and_interrupted_publication_without_reset(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "unknown-retained-state").write_bytes(b"preserve")
            with self.assertRaises(ValueError): provision.provision(root)
            self.assertEqual([path.name for path in root.iterdir()], ["unknown-retained-state"])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            provision.provision(root)
            before = {path.name:path.read_bytes() for path in root.iterdir()}
            self.assertEqual(json.loads(before["origin.json"])["account_id"], setup.ACCOUNT)
            with self.assertRaises(ValueError): provision.provision(root)
            self.assertEqual(before, {path.name:path.read_bytes() for path in root.iterdir()})
        with tempfile.TemporaryDirectory() as temporary, patch.object(provision.shared, "publish", side_effect=OSError("interrupted")):
            root = Path(temporary)
            with self.assertRaises(OSError): provision.provision(root)
            self.assertTrue((root / ".lock").exists())
            with self.assertRaises(ValueError): provision.provision(root)

    def test_admin_exact_actions_clean_environment_and_no_policy_authority(self):
        with patch.object(admin.os, "geteuid", return_value=1000), patch.object(sys, "argv", ["admin", "inspect"]), patch.object(admin, "call") as run:
            with self.assertRaises(ValueError): admin.main()
            run.assert_not_called()
        with patch.object(admin.os, "geteuid", return_value=0), patch.object(admin, "protected"), patch.object(admin, "call") as run:
            for action in ("inspect", "start", "stop"):
                with patch.object(sys, "argv", ["admin", action]): admin.main()
            for args in (["submit"], ["start", "--anything"], []):
                before = run.call_count
                with patch.object(sys, "argv", ["admin", *args]):
                    with self.assertRaises(ValueError): admin.main()
                self.assertEqual(run.call_count, before)
            commands = json.dumps(run.call_args_list, default=str)
            self.assertNotIn("alpaca-paper-submit", commands)
            self.assertNotIn("configure", commands)
        with patch.object(admin.subprocess, "run") as run:
            admin.call(["fixed"])
            self.assertEqual(run.call_args.kwargs["env"], {"PATH":"/usr/bin:/bin"})
