# SPDX-License-Identifier: Apache-2.0
"""Isolated worker tests use private temporary files and fake broker subprocesses."""

import fcntl
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("maintenance", Path(__file__).with_name("maintain-alpaca-paper.py"))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)

ACCOUNT = "374d3d8e-63ef-41d1-a1a3-b7921776fbc4"


class MaintenanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.store = self.root / "store"
        self.store.mkdir()
        self.journal = self.store / "orders.jsonl"
        self.journal.write_text("{}")
        self.config = self.root / "config.json"
        self.plan = dict(version="alpaca-maintenance-v1", account_id=ACCOUNT, max_pages=2, mode="observe")
        self.config.write_text(json.dumps(self.plan))
        self.config.chmod(0o600)
        self.calls = []
        self.report = dict(version=1, venue="alpaca", environment="paper", expected_account_id=ACCOUNT,
            checkpoint_refreshed=True, new_order_authority=False, retry_authorized=False,
            claim_release_authorized=False, working_before_cancellation=["radar_1"], unresolved=[],
            cancellation_pending_before_tick=[], terminal=[], cancellation_attempt=None)

    def provider(self, args, **kwargs):
        self.calls.append((args, kwargs))
        return types.SimpleNamespace(stdout=json.dumps(self.report))

    def tick(self, **kwargs):
        return worker.tick(self.store, self.config, kwargs.pop("run", self.provider),
            {"RADAR_ALPACA_PAPER_KEY_ID":"fixture-key", "RADAR_ALPACA_PAPER_SECRET_KEY":"fixture-secret",
             "OTHER_SECRET":"omit", "RADAR_MODEL_CODEX":"omit"},
            load=lambda path: worker.configuration(path, os.geteuid()), **kwargs)

    def test_modes_dispatch_only_fixed_maintenance_with_minimal_credential_environment(self):
        for mode in ("observe", "cancel_working"):
            self.plan["mode"] = mode
            self.config.write_text(json.dumps(self.plan))
            self.report["cancellation_attempt"] = None if mode == "observe" else {"cancellation":"request_accepted"}
            result = self.tick()
            expected = ["/usr/local/lib/radar-paper-execution/radar", "alpaca-paper-maintain", "--journal",
                        str(self.journal), "--account-id", ACCOUNT, "--max-pages", "2"]
            if mode == "cancel_working":
                expected.append("--cancel-working")
            self.assertEqual(self.calls[-1], (expected, dict(check=True, capture_output=True, text=True,
                timeout=75, env={"PATH":"/usr/bin:/bin", "RADAR_ALPACA_PAPER_KEY_ID":"fixture-key",
                                 "RADAR_ALPACA_PAPER_SECRET_KEY":"fixture-secret"})))
            self.assertEqual(result["cancellation_requested"], mode == "cancel_working")
            self.assertFalse(result["cancellation_confirmed"])
            self.assertFalse(result["new_order_authority"])
            self.assertEqual(result["orders"]["working_before_cancellation"], 1)
            self.assertEqual(self.journal.read_text(), "{}")

    def test_root_owned_readonly_configuration_and_exact_schema_are_required(self):
        alias = self.root / "config-link"
        alias.symlink_to(self.config)
        with self.assertRaises(OSError):
            worker.configuration(alias, os.geteuid())
        with self.assertRaises(ValueError):
            worker.configuration(self.config, os.geteuid() + 1)
        self.config.chmod(0o622)
        with self.assertRaises(ValueError):
            worker.configuration(self.config, os.geteuid())
        self.config.chmod(0o600)
        for field, value in [("version", "other"), ("account_id", "paper"), ("account_id", True),
                             ("max_pages", 0), ("max_pages", 11), ("max_pages", True),
                             ("max_pages", 1.0), ("mode", "autonomous"), ("url", "https://api.alpaca.markets")]:
            changed = dict(self.plan)
            changed[field] = value
            self.config.write_text(json.dumps(changed))
            with self.assertRaises(ValueError, msg=str((field, value))):
                self.tick()
        self.assertEqual(self.calls, [])
        for pages in (1, 10):
            self.config.write_text(json.dumps({**self.plan, "max_pages":pages}))
            self.assertEqual(self.tick()["maintenance"], "observed")
        self.config.write_text(" " * 65537)
        with self.assertRaises(ValueError):
            worker.configuration(self.config, os.geteuid())

    def test_missing_journal_symlink_and_fifo_inputs_refuse_before_subprocess(self):
        self.journal.unlink()
        with self.assertRaises(ValueError):
            self.tick()
        self.assertFalse(self.journal.exists())
        self.journal.symlink_to(self.config)
        with self.assertRaises(ValueError):
            self.tick()
        self.journal.unlink()
        os.mkfifo(self.journal)
        with self.assertRaises(ValueError):
            self.tick()
        self.journal.unlink()
        os.mkfifo(self.root / "fifo")
        with self.assertRaises(ValueError):
            worker.configuration(self.root / "fifo", os.geteuid())
        with self.assertRaises(ValueError):
            worker.configuration(self.config, os.geteuid() + 1)
        self.assertEqual(self.calls, [])

    def test_storage_exact_boundaries_pause_without_recreating_or_trimming_journal(self):
        self.assertEqual(self.tick(max_bytes=worker.RESERVATION + 1)["reason"], "storage_limit")
        self.assertEqual(self.tick(max_files=1)["reason"], "storage_limit")
        self.assertEqual(self.calls, [])
        self.assertEqual(self.tick(max_bytes=worker.RESERVATION + 2, max_files=2)["maintenance"], "observed")
        self.assertEqual(self.journal.read_text(), "{}")

    def test_bad_account_authority_and_classification_reports_never_report_success(self):
        for field, value in [("version", True), ("venue", "other"), ("environment", "live"),
                             ("expected_account_id", "other"), ("checkpoint_refreshed", False),
                             ("new_order_authority", True), ("retry_authorized", True),
                             ("claim_release_authorized", True), ("unresolved", None),
                             ("terminal", [1]), ("cancellation_attempt", {"cancellation":"request_accepted"})]:
            before = self.report[field]
            self.report[field] = value
            with self.assertRaises(ValueError, msg=str((field, value))):
                self.tick()
            self.report[field] = before
        del self.report["cancellation_attempt"]
        with self.assertRaises(ValueError):
            self.tick()
        for output in ("[]", "invalid", " " * (4 * 1024 * 1024 + 1)):
            with self.assertRaises(ValueError):
                self.tick(run=lambda *args, **kwargs: types.SimpleNamespace(stdout=output))

    def test_lock_refuses_overlap_and_subprocess_failure_leaves_history_untouched(self):
        with (self.store / ".lock").open("wb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(BlockingIOError):
                self.tick()
        self.assertEqual(self.calls, [])
        def failed(*args, **kwargs):
            raise subprocess.TimeoutExpired("fixture", 75, stderr="secret-must-not-be-printed")
        with self.assertRaises(subprocess.TimeoutExpired):
            self.tick(run=failed)
        self.assertEqual(self.journal.read_text(), "{}")

    def test_main_refuses_argument_overrides_before_any_worker_dispatch(self):
        with patch.object(worker.sys, "argv", ["worker", "--url", "https://api.alpaca.markets"]), \
                patch.object(worker, "tick", side_effect=AssertionError("must not dispatch")), \
                self.assertRaises(ValueError):
            worker.main()
