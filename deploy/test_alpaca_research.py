# SPDX-License-Identifier: Apache-2.0
"""Campaign tests retain failures and prove order-free subprocess dispatch."""

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

spec = importlib.util.spec_from_file_location("research", Path(__file__).with_name("run-alpaca-research.py"))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class ResearchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.store = self.root / "store"
        self.store.mkdir()
        self.config = self.root / "campaign.json"
        self.plan = dict(version="alpaca-research-v1", symbol="SPY", start_unix_ms=1000,
                         interval_ms=120000, start_window_ms=1000, slots=3,
                         experiment=dict(strategy_id="fixture", quantity_nanoshares=1, horizon_ms=60000),
                         assumptions={"fixture": "CLI validates assumptions before model calls"})
        self.config.write_text(json.dumps(self.plan))
        self.calls = []

    def provider(self, argv, **kwargs):
        self.calls.append((argv, kwargs))
        begin = list(self.store.glob("*.begin.json"))
        self.assertEqual(len(begin), 1)
        self.assertFalse(list(self.store.glob("*.end.json")))
        self.assertEqual(json.loads(begin[0].read_text())["started_unix_ms"], 1000)
        Path(argv[-1]).write_text("{}")
        return types.SimpleNamespace(stdout=json.dumps({"execution_authority": False,
                                                        "record": {"decision_written": True}}))

    def tick(self, at=1000, run=None, **kwargs):
        return worker.tick(self.store, self.config, run or self.provider, lambda: at,
                           {"RADAR_ALPACA_PAPER_KEY_ID": "paper-fixture",
                            "RADAR_ALPACA_PAPER_SECRET_KEY": "secret-fixture", "OTHER_SECRET": "no"},
                           **kwargs)

    def test_claim_precedes_effects_and_model_has_no_broker_keys(self):
        synced = []
        original = os.fsync
        def fsync(descriptor):
            synced.append(os.readlink(f"/proc/self/fd/{descriptor}"))
            original(descriptor)
        def provider(argv, **kwargs):
            begin = str(self.store / "slot-0000.begin.json")
            self.assertIn(begin, synced)
            self.assertIn(str(self.store), synced[synced.index(begin) + 1:])
            return self.provider(argv, **kwargs)
        with patch.object(worker.os, "fsync", side_effect=fsync):
            self.assertEqual(self.tick(run=provider)["outcome"], "decision")
        self.assertEqual([call[0][1] for call in self.calls], ["alpaca-market-read", "alpaca-paper-propose"])
        market, model = self.calls
        self.assertEqual(market[0][0], "/usr/local/lib/radar-research/radar")
        self.assertEqual(model[0][0], "/usr/local/lib/radar-research/radar")
        self.assertEqual(market[1], dict(check=True, capture_output=True, text=True, timeout=20,
            env={"PATH": "/usr/bin:/bin", "RADAR_ALPACA_PAPER_KEY_ID": "paper-fixture",
                 "RADAR_ALPACA_PAPER_SECRET_KEY": "secret-fixture"}))
        self.assertEqual(model[1], dict(check=True, capture_output=True, text=True, timeout=100,
            env={"PATH": "/usr/bin:/bin", "RADAR_MODEL_CODEX": "/usr/local/bin/radar-codex",
                 "RADAR_MODEL_DAILY_USD": "unlimited"}))
        self.assertEqual(json.loads((self.store / "slot-0000.experiment.json").read_text()), self.plan["experiment"])
        self.assertEqual(self.tick()["slots"], ["decision", "future", "future"])
        self.assertEqual(len(self.calls), 2)

    def test_future_and_missed_slots_do_not_call_provider(self):
        self.assertEqual(self.tick(999)["slots"], ["future"] * 3)
        self.assertEqual(self.tick(2000)["outcome"], "missed")
        self.assertEqual(self.tick(2000)["slots"], ["missed", "future", "future"])
        self.assertFalse(self.calls)

    def test_last_allowed_millisecond_starts(self):
        def provider(argv, **kwargs):
            self.calls.append(argv)
            Path(argv[-1]).write_text("{}")
            return types.SimpleNamespace(stdout='{"execution_authority":false,"record":{"decision_written":true}}')
        self.assertEqual(self.tick(1999, provider)["outcome"], "decision")
        self.assertEqual(len(self.calls), 2)

    def test_failed_or_interrupted_attempt_is_not_retried_or_zero_profit(self):
        def fail(argv, **kwargs):
            self.calls.append(argv)
            raise subprocess.TimeoutExpired(argv, 1, stderr="private-vendor-body")
        self.assertEqual(self.tick(run=fail)["outcome"], "failed")
        self.assertEqual(self.tick()["slots"], ["failed", "future", "future"])
        self.assertEqual(len(self.calls), 1)
        (self.store / "slot-0000.end.json").unlink()
        self.assertEqual(self.tick()["slots"], ["interrupted_unknown", "future", "future"])
        self.assertEqual(len(self.calls), 1)
        self.assertNotIn("private-vendor-body", "".join(path.read_text() for path in self.store.iterdir()))

    def test_changed_campaign_lock_and_symlinks_refuse(self):
        self.tick(999)
        self.plan["symbol"] = "QQQ"
        self.config.write_text(json.dumps(self.plan))
        with self.assertRaises(ValueError):
            self.tick()
        (self.store / "campaign.json").unlink()
        with (self.store / ".lock").open("rb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(BlockingIOError):
                self.tick()
        (self.store / "unsafe").symlink_to(self.config)
        with self.assertRaises(ValueError):
            self.tick()
        self.assertFalse(self.calls)

    def test_storage_reservation_refuses_before_claim_or_model(self):
        self.assertEqual(self.tick(max_bytes=worker.RESERVATION - 1)["status"], "storage_limit")
        self.assertFalse((self.store / "campaign.json").exists())
        raw = self.config.read_bytes()
        self.assertEqual(self.tick(max_bytes=worker.RESERVATION + len(raw) - 1)["slots"][0], "storage_limit")
        self.assertFalse(list(self.store.glob("*.begin.json")))
        self.assertEqual(self.tick(max_bytes=worker.RESERVATION + len(raw))["outcome"], "decision")

    def test_bad_report_and_oversized_capture_stay_failed(self):
        def provider(argv, **kwargs):
            self.calls.append(argv)
            Path(argv[-1]).write_bytes(b"x" * (1024 * 1024 + 1))
            return types.SimpleNamespace(stdout="{}")
        self.assertEqual(self.tick(run=provider)["outcome"], "failed")
        self.assertEqual(len(self.calls), 1)

    def test_response_cannot_grant_authority_or_hide_missing_decision(self):
        def provider(argv, **kwargs):
            result = self.provider(argv, **kwargs)
            if argv[1] == "alpaca-paper-propose":
                result.stdout = '{"execution_authority":true,"record":{"decision_written":true}}'
            return result
        self.assertEqual(self.tick(run=provider)["outcome"], "failed")
        self.assertEqual(self.tick()["slots"], ["failed", "future", "future"])

    def test_invalid_campaign_and_partial_claim_refuse(self):
        for changes in ({"slots": True}, {"slots": 0}, {"symbol": "BTC"}, {"interval_ms": 119999},
                        {"start_window_ms": 90001}, {"version": "other"}, {"extra": 1}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                worker.campaign(json.dumps(self.plan | changes))
        self.tick(999)
        (self.store / "slot-0000.begin.json").write_text("{")
        with self.assertRaises(ValueError):
            self.tick()
        self.assertFalse(self.calls)
