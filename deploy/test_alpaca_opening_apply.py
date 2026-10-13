# SPDX-License-Identifier: Apache-2.0
"""One-time opening repair preserves real private-file bytes and stops on failures."""

from contextlib import contextmanager
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import stat
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


opening = load("opening_apply", "apply-alpaca-opening.py")
administration = load("opening_admin", "radar-paper-admin.py")


def report():
    return dict(version=1, venue="alpaca", environment="paper", expected_account_id=administration.ACCOUNT,
        checkpoint_written=True, execution_authority=False, retry_authorized=False,
        claim_release_authorized=False, profit_measured=False, claims={"claims":[]},
        checkpoint={"account_id":administration.ACCOUNT, "orders":{}, "observation":{
            "opening_cash_acknowledgement":{"activity_id":opening.ACTIVITY, "amount_nanos":opening.AMOUNT,
                "operator_acknowledged_already_in_opening_cash":True, "broker_posting_time_verified":False}}})


class OpeningApplicationTests(unittest.TestCase):
    @contextmanager
    def context(self, failure=None):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, target, reader = (root / name for name in ("source", "acknowledge", "reader"))
            source.write_bytes(b"new-reviewed-reader")
            reader.write_bytes(b"unchanged-recurring-reader")
            journal, lock = root / "orders.jsonl", root / ".lock"
            original = b"retained-original-genesis\n"
            journal.write_bytes(original)
            lock.write_bytes(b"existing-observation-lock")
            calls, protected = [], []
            admin = types.SimpleNamespace(READER=reader, TIMER=administration.TIMER,
                SERVICE=administration.SERVICE, ACCOUNT=administration.ACCOUNT,
                protected=lambda path:protected.append(path), plan=lambda:{"max_pages":2})
            def diagnostic(action, pages):
                with patch.object(administration, "READER", reader):
                    return administration.diagnostic(action, pages)
            admin.diagnostic = diagnostic
            def call(argv):
                calls.append(argv)
                if "start" in argv:
                    # The unchanged maintenance worker must be able to acquire its lock.
                    with lock.open("rb") as stream:
                        fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    if failure == "probe":
                        raise subprocess.CalledProcessError(1, argv)
            admin.call = call
            def run(argv, **kwargs):
                calls.append(argv)
                self.assertEqual(kwargs, dict(check=True, capture_output=True, text=True,
                    timeout=100, env={"PATH":"/usr/bin:/bin"}))
                self.assertEqual(target.read_bytes(), source.read_bytes())
                self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o755)
                self.assertEqual(reader.read_bytes(), b"unchanged-recurring-reader")
                with lock.open("rb") as stream, self.assertRaises(BlockingIOError):
                    fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                if failure == "command":
                    raise subprocess.CalledProcessError(1, argv)
                journal.write_bytes((b"changed!" if failure == "prefix" else original)
                    + (b"" if failure == "no_append" else b"new-acknowledgement\n"))
                response = report()
                if failure == "report":
                    response["execution_authority"] = True
                return types.SimpleNamespace(stdout=json.dumps(response))
            with patch.object(opening, "SOURCE", source), patch.object(opening, "TARGET", target), \
                    patch.object(opening, "STATE", root), patch.object(opening.os, "geteuid", return_value=0), \
                    patch.object(opening, "READER_HASH", hashlib.sha256(reader.read_bytes()).hexdigest()), \
                    patch("builtins.print") as printed:
                yield types.SimpleNamespace(root=root, source=source, target=target, reader=reader,
                    journal=journal, original=original, admin=admin, run=run, calls=calls,
                    protected=protected, printed=printed, digest=hashlib.sha256(source.read_bytes()).hexdigest())

    def test_report_requires_exact_paper_event_no_authority_and_empty_claims(self):
        opening.verify(report(), administration.ACCOUNT)
        changes = [((), "version", True), ((), "venue", "other"), ((), "environment", "live"),
            ((), "expected_account_id", "other"), ((), "checkpoint_written", False),
            ((), "execution_authority", True), ((), "retry_authorized", True),
            ((), "claim_release_authorized", True), ((), "profit_measured", True),
            ((), "checkpoint", None), ((), "claims", None), (("claims",), "claims", [{}]),
            (("checkpoint",), "account_id", "other"), (("checkpoint",), "orders", {"new":{}}),
            (("checkpoint",), "observation", []),
            (("checkpoint", "observation"), "opening_cash_acknowledgement", None)]
        for field, value in (("activity_id", "other"), ("amount_nanos", "1"),
                ("operator_acknowledged_already_in_opening_cash", False), ("broker_posting_time_verified", True)):
            changes.append((("checkpoint", "observation", "opening_cash_acknowledgement"), field, value))
        for parents, field, value in changes:
            bad = report()
            target = bad
            for parent in parents:
                target = target[parent]
            target[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                opening.verify(bad, administration.ACCOUNT)

    def test_root_hashes_and_existing_targets_refuse_before_side_effects(self):
        for kind in ("user", "malformed", "hash", "old_reader", "existing", "dangling"):
            with self.context() as case:
                digest = case.digest
                if kind == "malformed": digest = "bad"
                if kind == "hash": digest = "0" * 64
                if kind == "old_reader": case.reader.write_bytes(b"different")
                if kind == "existing": case.target.write_bytes(b"retain-this")
                if kind == "dangling": case.target.symlink_to(case.root / "missing")
                with patch.object(opening.os, "geteuid", return_value=1 if kind == "user" else 0), \
                        self.assertRaises(ValueError):
                    opening.apply(digest, lambda:case.admin, case.run)
                self.assertEqual(case.calls, [])
                self.assertEqual(case.journal.read_bytes(), case.original)
                if kind == "existing": self.assertEqual(case.target.read_bytes(), b"retain-this")

    def test_success_uses_fixed_isolated_command_preserves_prefix_and_probes_before_timer(self):
        with self.context() as case:
            opening.apply(case.digest, lambda:case.admin, case.run)
            self.assertEqual(len(case.calls), 4)
            self.assertEqual(case.calls[0], ["/usr/bin/systemctl", "stop", administration.TIMER])
            argv = case.calls[1]
            index = argv.index(str(case.target))
            self.assertEqual(argv[index:], [str(case.target), "alpaca-opening-cash-acknowledge", "--journal",
                "/var/lib/radar-paper-execution/orders.jsonl", "--account-id", administration.ACCOUNT,
                "--activity-id", opening.ACTIVITY, "--max-pages", "2", "--already-in-opening-balance"])
            for value in ("--unit=radar-paper-opening-ack", "DynamicUser=yes", "User=radar-paper-execution",
                    "StateDirectory=radar-paper-execution", "ProtectSystem=strict", "CapabilityBoundingSet=",
                    "EnvironmentFile=/etc/radar/alpaca-paper.env", "RuntimeMaxSec=75"):
                self.assertIn(value, argv[:index])
            for name in (administration.TIMER, administration.SERVICE):
                self.assertIn(Path("/etc/systemd/system") / name, case.protected)
            self.assertEqual(case.calls[2:], [["/usr/bin/systemctl", "start", administration.SERVICE],
                ["/usr/bin/systemctl", "enable", "--now", administration.TIMER]])
            self.assertEqual(case.journal.read_bytes(), case.original + b"new-acknowledgement\n")
            self.assertFalse(json.loads(case.printed.call_args.args[0])["new_order_authority"])
            with self.assertRaises(ValueError):
                opening.apply(case.digest, lambda:case.admin, case.run)
            self.assertEqual(len(case.calls), 4)

    def test_failed_command_report_prefix_append_or_probe_leaves_staged_reader_and_timer_stopped(self):
        for failure in ("command", "report", "prefix", "no_append", "probe"):
            with self.context(failure) as case, self.assertRaises((ValueError, subprocess.CalledProcessError)):
                try:
                    opening.apply(case.digest, lambda:case.admin, case.run)
                finally:
                    self.assertTrue(case.target.is_file())
                    self.assertFalse(any("enable" in call for call in case.calls))
                    self.assertEqual(len(case.calls), 3 if failure == "probe" else 2)
                    if failure == "command": self.assertEqual(case.journal.read_bytes(), case.original)

    def test_regular_bounded_no_follow_inputs_exclusive_publish_and_missing_lock_preserve_history(self):
        with self.context() as case:
            self.assertEqual(opening.read(case.source, 100), case.source.read_bytes())
            for path, limit in ((case.source, 1), (case.root, 100)):
                with self.assertRaises((ValueError, OSError)):
                    opening.read(path, limit)
            link = case.root / "symlink"
            link.symlink_to(case.source)
            with self.assertRaises(OSError): opening.read(link, 100)
            for length in (0, len(case.original) + 1):
                with self.assertRaises(ValueError): opening.prefix(case.journal, length)
            opening.publish(case.target, b"first")
            with self.assertRaises(FileExistsError): opening.publish(case.target, b"replace")
            self.assertEqual(case.target.read_bytes(), b"first")
            case.target.unlink()
            (case.root / ".lock").unlink()
            with self.assertRaises(FileNotFoundError): opening.apply(case.digest, lambda:case.admin, case.run)
            self.assertFalse((case.root / ".lock").exists())
            self.assertFalse(case.target.exists())
            self.assertEqual(case.journal.read_bytes(), case.original)
            self.assertEqual(len(case.calls), 1)

    def test_only_hash_pinned_protected_installed_admin_bytes_are_executed(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "admin"
            raw = b"def protected(path):\n    global checked\n    checked = str(path)\n"
            path.write_bytes(raw)
            real_lstat = Path.lstat
            safe = types.SimpleNamespace(st_mode=stat.S_IFREG | 0o755, st_uid=0)
            with patch.object(opening, "ADMIN", path), \
                    patch.object(opening, "ADMIN_HASH", hashlib.sha256(raw).hexdigest()), \
                    patch.object(Path, "lstat", lambda value:safe if value == path else real_lstat(value)):
                self.assertEqual(opening.admin_module().checked, str(path))
                path.write_bytes(raw + b"raise RuntimeError('must not execute unpinned bytes')\n")
                with self.assertRaises(ValueError): opening.admin_module()
                path.write_bytes(raw)
                for mode, uid in ((stat.S_IFREG | 0o777, 0), (stat.S_IFREG | 0o755, 1),
                        (stat.S_IFLNK | 0o755, 0)):
                    safe.st_mode, safe.st_uid = mode, uid
                    with self.assertRaises(ValueError): opening.admin_module()
