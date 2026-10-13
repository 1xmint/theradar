# SPDX-License-Identifier: Apache-2.0
"""First-checkpoint tests use fresh temporary stores and no broker credentials."""

import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("initialization", Path(__file__).with_name("initialize-alpaca-paper.py"))
init = importlib.util.module_from_spec(spec)
spec.loader.exec_module(init)
ACCOUNT = "374d3d8e-63ef-41d1-a1a3-b7921776fbc4"


class InitializationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.config = self.root / "config.json"
        self.plan = dict(version="alpaca-maintenance-v1", account_id=ACCOUNT, max_pages=2, mode="observe")
        self.config.write_text(json.dumps(self.plan))
        self.config.chmod(0o600)
        self.count = 0
        self.calls = []
        self.report = dict(version=1, venue="alpaca", environment="paper", expected_account_id=ACCOUNT,
            checkpoint_written=True, execution_authority=False, retry_authorized=False,
            claim_release_authorized=False, checkpoint=dict(account_id=ACCOUNT, started=100, completed=101, orders={}),
            claims={"claims":[]})

    def fresh(self):
        self.count += 1
        directory = self.root / f"state-{self.count}"
        directory.mkdir()
        return directory

    def provider(self, args, **kwargs):
        self.calls.append((args, kwargs))
        journal = Path(args[3])
        fence = json.loads((journal.parent / ".lock").read_text())
        self.assertEqual(fence, {"phase":"genesis_started", "account_id":ACCOUNT})
        self.assertFalse(journal.exists())
        journal.write_text("fixture-durable-checkpoint")
        return types.SimpleNamespace(stdout=json.dumps(self.report))

    def initialize(self, directory=None, run=None):
        clock = iter([100, 102])
        return init.initialize(directory or self.fresh(), self.config, run or self.provider,
            {"RADAR_ALPACA_PAPER_KEY_ID":"fixture-key", "RADAR_ALPACA_PAPER_SECRET_KEY":"fixture-secret",
             "RADAR_MODEL_CODEX":"omit", "OTHER_SECRET":"omit"},
            load=lambda path: init.worker.configuration(path, os.geteuid()), clock=lambda: next(clock))

    def test_durable_fence_precedes_single_checkpoint_and_completion_is_exclusive(self):
        directory = self.fresh()
        synced = []
        original = os.fsync
        def fsync(descriptor):
            synced.append(os.readlink(f"/proc/self/fd/{descriptor}"))
            original(descriptor)
        def provider(args, **kwargs):
            self.assertIn(str(directory / ".lock"), synced)
            self.assertIn(str(directory), synced[synced.index(str(directory / ".lock")) + 1:])
            return self.provider(args, **kwargs)
        with patch.object(init.os, "fsync", side_effect=fsync):
            outcome = self.initialize(directory, provider)
        self.assertEqual(outcome, {"genesis":"initialized", "account_id":ACCOUNT, "new_order_authority":False})
        self.assertEqual(self.calls, [([init.worker.READER, "alpaca-cash-checkpoint", "--journal",
            str(directory / "orders.jsonl"), "--account-id", ACCOUNT, "--max-pages", "2"],
            dict(check=True, capture_output=True, text=True, timeout=75,
                 env={"PATH":"/usr/bin:/bin", "RADAR_ALPACA_PAPER_KEY_ID":"fixture-key",
                      "RADAR_ALPACA_PAPER_SECRET_KEY":"fixture-secret"}))])
        complete = directory / "genesis-complete.json"
        self.assertEqual(json.loads(complete.read_text()), outcome)
        self.assertEqual(stat.S_IMODE(complete.stat().st_mode), 0o600)
        with self.assertRaises(ValueError):
            self.initialize(directory)
        self.assertEqual(len(self.calls), 1)

    def test_prior_storage_and_stop_mode_refuse_before_checkpoint(self):
        for name in ("orders.jsonl", ".lock", "genesis-complete.json", "other"):
            directory = self.fresh()
            prior = directory / name
            prior.write_text("retained-history")
            with self.assertRaises(ValueError):
                self.initialize(directory)
            self.assertEqual(prior.read_text(), "retained-history")
        self.config.write_text(json.dumps({**self.plan, "mode":"cancel_working"}))
        directory = self.fresh()
        with self.assertRaises(ValueError):
            self.initialize(directory)
        self.assertEqual(list(directory.iterdir()), [])
        self.assertEqual(self.calls, [])

    def test_timeout_retains_fence_and_partial_history_and_prevents_retry(self):
        directory = self.fresh()
        def failed(args, **kwargs):
            self.calls.append(args)
            Path(args[3]).write_text("partial-history")
            raise subprocess.TimeoutExpired("fixture", 75, stderr="secret-fixture")
        with self.assertRaises(subprocess.TimeoutExpired):
            self.initialize(directory, failed)
        self.assertTrue((directory / ".lock").is_file())
        self.assertFalse((directory / "genesis-complete.json").exists())
        with self.assertRaises(ValueError):
            self.initialize(directory, failed)
        self.assertEqual((directory / "orders.jsonl").read_text(), "partial-history")
        self.assertEqual(len(self.calls), 1)

    def test_invalid_checkpoint_report_and_stale_times_never_publish_completion(self):
        cases = [("venue", "other"), ("environment", "live"), ("expected_account_id", "other"), ("version", True),
                 ("checkpoint_written", False), ("execution_authority", True), ("retry_authorized", True),
                 ("claim_release_authorized", True), ("checkpoint", None), ("claims", {"claims":[1]})]
        for field, value in cases:
            before = self.report[field]
            self.report[field] = value
            directory = self.fresh()
            with self.assertRaises(ValueError):
                self.initialize(directory)
            self.assertFalse((directory / "genesis-complete.json").exists())
            self.report[field] = before
        for changes in ({"account_id":"other"}, {"orders":{"unknown":{}}}, {"started":True},
                        {"started":99}, {"completed":103}, {"completed":99}):
            checkpoint = self.report["checkpoint"]
            self.report["checkpoint"] = {**checkpoint, **changes}
            directory = self.fresh()
            with self.assertRaises(ValueError):
                self.initialize(directory)
            self.assertFalse((directory / "genesis-complete.json").exists())
            self.report["checkpoint"] = checkpoint

    def test_missing_empty_and_nonregular_journal_do_not_count_as_initialization(self):
        for kind in ("missing", "empty", "symlink", "fifo"):
            directory = self.fresh()
            def provider(args, **kwargs):
                path = Path(args[3])
                if kind == "empty":
                    path.write_bytes(b"")
                elif kind == "symlink":
                    path.symlink_to(self.config)
                elif kind == "fifo":
                    os.mkfifo(path)
                return types.SimpleNamespace(stdout=json.dumps(self.report))
            with self.assertRaises((ValueError, OSError)):
                self.initialize(directory, provider)
            self.assertFalse((directory / "genesis-complete.json").exists())

    def test_main_rejects_overrides_and_publication_never_overwrites(self):
        with patch.object(init.sys, "argv", ["init", "--reset"]), \
                patch.object(init, "initialize", side_effect=AssertionError("must not dispatch")), \
                self.assertRaises(ValueError):
            init.main()
        target = self.root / "retained.json"
        target.write_bytes(b"retained")
        with self.assertRaises(FileExistsError):
            init.publish(target, {"replacement":True})
        self.assertEqual(target.read_bytes(), b"retained")
