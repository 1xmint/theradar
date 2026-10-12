# SPDX-License-Identifier: Apache-2.0
"""Every planned slot remains visible; future outcomes never enter scoring."""

import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import types
import unittest

spec = importlib.util.spec_from_file_location("cohort", Path(__file__).with_name("report-alpaca-cohort.py"))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class CohortTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.store = Path(self.temp.name)
        self.plan = dict(version="alpaca-research-v1", slots=6, start_unix_ms=1000, interval_ms=120000,
                         symbol="SPY", experiment=dict(strategy_id="fixture", quantity_nanoshares=1, horizon_ms=4000),
                         assumptions={"explicit": "hypothetical"})
        raw = json.dumps(self.plan).encode()
        (self.store / "campaign.json").write_bytes(raw)
        self.digest = hashlib.sha256(raw).hexdigest()
        self.calls = []

    def claim(self, index, status=None):
        at = 1000 + index * 120000
        prefix = self.store / f"slot-{index:04d}"
        Path(str(prefix) + ".begin.json").write_text(json.dumps({"campaign_sha256": self.digest,
            "scheduled_unix_ms": at, "started_unix_ms": at}))
        if status is not None:
            Path(str(prefix) + ".end.json").write_text(json.dumps({"status": status, "finished_unix_ms": at + 1}))
        if status == "decision":
            Path(str(prefix) + ".decision.json").write_text(json.dumps({"proposal": self.plan["experiment"],
                "symbol": "SPY", "assumptions": self.plan["assumptions"], "outcome_not_before_unix_ms": at + 4000}))

    def scorer(self, argv, **kwargs):
        self.calls.append((argv, kwargs))
        return types.SimpleNamespace(stdout=json.dumps({"status": "scored", "exit_as_of_unix_ms": int(argv[-1]),
            "execution_authority": False, "strategy_net_pnl_microusd": "-7"}))

    def report(self, tape=(), at=400000, run=None):
        return worker.cohort(self.store, tape, at, run or self.scorer)

    def test_complete_denominator_and_exact_cutoff_preserve_losses(self):
        for index, status in enumerate(("decision", "failed", "missed", None)):
            self.claim(index, status)
        tape = [(4999, self.store / "older", ["SPY"]), (5000, self.store / "exact", ["SPY"]),
                (5001, self.store / "future", ["SPY"])]
        report = self.report(tape)
        self.assertEqual(report["planned_slots"], 6)
        self.assertEqual(len(report["slots"]), 6)
        self.assertEqual(report["status_counts"], {"scored": 1, "failed": 1, "missed": 1,
                                                  "interrupted_unknown": 1, "future": 2})
        self.assertIsNone(report["aggregate_profit_microusd"])
        self.assertEqual(report["slots"][0]["comparison"]["strategy_net_pnl_microusd"], "-7")
        argv, kwargs = self.calls[0]
        self.assertEqual(argv, [worker.READER, "alpaca-paper-decision-score", "--decision",
            str(self.store / "slot-0000.decision.json"), "--exit", str(self.store / "exact"),
            "--exit-as-of-unix-ms", "5000"])
        self.assertEqual(kwargs, dict(check=True, capture_output=True, text=True, timeout=20,
                                     env={"PATH": "/usr/bin:/bin"}))

    def test_future_claim_terminal_and_horizon_are_not_scored(self):
        self.claim(0, "decision")
        self.assertEqual(self.report(at=999)["slots"][0]["status"], "not_available_at_watermark")
        self.assertEqual(self.report(at=1000)["slots"][0]["status"], "terminal_not_available_at_watermark")
        self.assertEqual(self.report(at=1001)["slots"][0]["status"], "awaiting_horizon")
        self.assertEqual(self.report(at=5000)["slots"][0]["status"], "unscoreable")
        self.assertFalse(self.calls)

    def test_unknown_exit_and_scorer_failure_never_become_zero_profit(self):
        self.claim(0, "decision")
        report = self.report()
        self.assertEqual(report["slots"][0]["reason"], "no_available_exit_capture")
        self.assertIsNone(report["slots"][0]["comparison"])
        self.assertEqual(report["slots"][1]["status"], "not_recorded")
        def fail(argv, **kwargs):
            raise subprocess.TimeoutExpired(argv, 20, stderr="private-error")
        report = self.report([(5000, self.store / "exact", ["SPY"])], run=fail)
        self.assertEqual(report["slots"][0]["reason"], "scorer_refused")
        self.assertNotIn("private-error", json.dumps(report))

    def test_duplicate_latest_capture_refuses_arbitrary_tie_break(self):
        self.claim(0, "decision")
        report = self.report([(5000, self.store / "a", ["SPY"]), (5000, self.store / "b", ["SPY"])])
        self.assertEqual(report["slots"][0]["reason"], "ambiguous_latest_capture")
        self.assertFalse(self.calls)

    def test_campaign_identity_and_fixed_experiment_must_match(self):
        self.claim(0, "decision")
        path = self.store / "slot-0000.decision.json"
        decision = json.loads(path.read_text())
        decision["proposal"]["quantity_nanoshares"] = 2
        path.write_text(json.dumps(decision))
        with self.assertRaises(ValueError):
            self.report()
        decision["proposal"]["quantity_nanoshares"] = 1
        path.write_text(json.dumps(decision))
        begin = self.store / "slot-0000.begin.json"
        begin.write_text('{"campaign_sha256":"wrong","scheduled_unix_ms":1000,"started_unix_ms":1000}')
        with self.assertRaisesRegex(ValueError, "identity mismatch"):
            self.report()

    def test_capture_inventory_and_links_are_bounded(self):
        tape = self.store / "tape"
        tape.mkdir()
        path = tape / ("capture-" + "a" * 32 + ".json")
        path.write_text(json.dumps({"format": "alpaca-iex-snapshot-v1", "available_at_unix_ms": 5000,
                                    "symbols": ["SPY"]}))
        self.assertEqual(worker.captures(tape), [(5000, path, ["SPY"])])
        path.unlink()
        path.symlink_to(self.store / "campaign.json")
        with self.assertRaises(OSError):
            worker.captures(tape)
