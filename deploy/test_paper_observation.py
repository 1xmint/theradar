# SPDX-License-Identifier: Apache-2.0
"""Controller observation and cancellation use fixed CLI calls under existing locks."""

import hashlib
import json
from pathlib import Path
import types
import unittest
from unittest.mock import patch
import test_paper_control as fixture

control = fixture.control


class ObservationTests(unittest.TestCase):
    context = fixture.PaperControlTests.context
    enable = fixture.PaperControlTests.enable

    def runner(self, case):
        scenario = dict(version="alpaca-paper-stress-v1", account_id=fixture.ACCOUNT,
            max_quantity_nanoshares=10000000, fee_per_side_microusd=10000, slippage_bps_per_side=10)
        model = case.root / "model.json"
        model.write_bytes(control.encoded(scenario))
        case.instance.config["model_sha256"] = hashlib.sha256(model.read_bytes()).hexdigest()
        original = case.instance.run
        case.report = dict(venue="alpaca", environment="paper", expected_account_id=fixture.ACCOUNT,
            checkpoint_refreshed=True, new_order_authority=False, retry_authorized=False,
            claim_release_authorized=False, profit_measured=False, working_before_cancellation=[],
            unresolved=[], cancellation_pending_before_tick=[], terminal=[])
        case.cancel_report = dict(venue="alpaca", environment="paper", client_order_id="radar_old", broker_order_id="broker-1",
            cancellation="request_accepted", cancellation_confirmed=False, fills_reconciled=False,
            retry_authorized=False, claim_release_authorized=False)
        def run(argv, **kwargs):
            command = argv[1]
            if command == "alpaca-read": return original(argv, **kwargs)
            case.calls.append((argv, kwargs))
            if command == "alpaca-paper-maintain":
                self.assertNotIn("--cancel-working", argv)
                value = case.report
                if hasattr(case, "during_read"): case.during_read()
            elif command == "alpaca-market-read":
                self.assertEqual(argv[2:4], ["--symbols", "QQQ,SPY"])
                output = Path(argv[5])
                self.assertFalse(output.exists())
                output.write_bytes(b"private market capture")
                value = dict(capture_written=True, feed="iex", execution_authority=False)
            elif command == "alpaca-quote":
                self.assertTrue(Path(argv[3]).exists())
                self.assertEqual(argv[-4:], ["--max-age-ms", "120000", "--max-spread-bps", "20"])
                value = dict(venue="alpaca", feed="iex", symbol=argv[5], execution_authority=False,
                    as_of_unix_ms=int(argv[7]), bid_microusd_per_share="100000000", ask_microusd_per_share="100010000")
                if hasattr(case, "bad_quote"): value[case.bad_quote] = "invalid"
            elif command == "alpaca-paper-cancel":
                self.assertEqual(argv[2:], ["--journal", str(case.root / "orders.jsonl"), "--account-id", fixture.ACCOUNT,
                    "--client-order-id", "radar_old"])
                value = case.cancel_report
            else: self.fail("Unexpected command " + command)
            self.assertEqual(kwargs["env"], {"PATH":"/usr/bin:/bin"})
            return types.SimpleNamespace(stdout=json.dumps(value))
        case.instance.run = run
        return patch.object(control, "MODEL", model)

    def test_observation_reconciles_then_collects_validated_quotes_without_broker_authority(self):
        with self.context() as case, self.runner(case):
            self.enable(case)
            case.calls.clear()
            result = case.instance.dispatch(1200, {"action":"observe"})
            self.assertEqual([argv[1] for argv, _ in case.calls],
                ["alpaca-paper-maintain", "alpaca-read", "alpaca-market-read", "alpaca-quote", "alpaca-quote"])
            self.assertEqual(result["generation"], 1)
            self.assertEqual(set(result["quotes"]), {"QQQ", "SPY"})
            self.assertEqual(result["observed_at_unix_ms"], 1000000)
            self.assertFalse(result["execution_authority"])
            self.assertFalse(result["learned_edge_verified"])
            self.assertFalse(result["profit_measured"])
            self.assertEqual((case.root / "orders.jsonl").read_bytes(), case.original)
            self.assertEqual(list(case.root.glob(".paper-market-*")), [])
            count = len(case.calls)
            for uid in (1100, 1300):
                with self.assertRaises(ValueError): case.instance.dispatch(uid, {"action":"observe"})
            self.assertEqual(len(case.calls), count)

    def test_paused_observation_skips_quotes_and_bad_reconciliation_or_model_refuses(self):
        with self.context() as case, self.runner(case):
            result = case.instance.dispatch(1200, {"action":"observe"})
            self.assertFalse(result["handover_enabled"])
            self.assertEqual(result["quotes"], {})
            self.assertEqual([argv[1] for argv, _ in case.calls], ["alpaca-paper-maintain", "alpaca-read"])
            for field, wrong in (("expected_account_id", "other"), ("checkpoint_refreshed", False),
                    ("new_order_authority", True), ("retry_authorized", True), ("claim_release_authorized", True),
                    ("profit_measured", True), ("terminal", None)):
                before = case.report[field]
                case.report[field] = wrong
                with self.assertRaises(ValueError): case.instance.dispatch(1200, {"action":"observe"})
                case.report[field] = before
            control.MODEL.write_bytes(b"changed")
            with self.assertRaises(ValueError): case.instance.dispatch(1200, {"action":"observe"})

    def test_quote_failure_cleans_only_own_capture_and_policy_change_during_read_refuses(self):
        with self.context() as case, self.runner(case):
            self.enable(case)
            sentinel = case.root / "retained-capture.json"
            sentinel.write_bytes(b"preserve")
            case.bad_quote = "as_of_unix_ms"
            with self.assertRaises(ValueError): case.instance.dispatch(1200, {"action":"observe"})
            self.assertEqual(list(case.root.glob(".paper-market-*")), [])
            self.assertEqual(sentinel.read_bytes(), b"preserve")
            del case.bad_quote
            case.during_read = lambda:case.instance.dispatch(1100, dict(action="pause", generation=1, operator=fixture.OPERATOR))
            with self.assertRaises(ValueError): case.instance.dispatch(1200, {"action":"observe"})

    def test_cancel_has_separate_role_generation_and_selected_acknowledgement(self):
        with self.context() as case, self.runner(case):
            self.enable(case)
            request = dict(action="cancel", generation=1, client_order_id="radar_old")
            self.assertEqual(case.instance.dispatch(1200, request)["cancellation"], "request_accepted")
            for uid, changed in ((1100, request), (1300, request), (1200, dict(request, generation=0)),
                    (1200, dict(request, generation=2)), (1200, dict(request, generation=True))):
                before = len(case.calls)
                with self.assertRaises(ValueError): case.instance.dispatch(uid, changed)
                self.assertEqual(len(case.calls), before)
            for field, wrong in (("client_order_id", "other"), ("cancellation", "confirmed"),
                    ("cancellation_confirmed", True), ("fills_reconciled", True), ("retry_authorized", True),
                    ("claim_release_authorized", True), ("broker_order_id", None)):
                prior = case.cancel_report[field]
                case.cancel_report[field] = wrong
                with self.assertRaises(ValueError): case.instance.dispatch(1200, request)
                case.cancel_report[field] = prior
            case.instance.dispatch(1100, dict(action="pause", generation=1, operator=fixture.OPERATOR))
            before = len(case.calls)
            with self.assertRaises(ValueError): case.instance.dispatch(1200, dict(request, generation=2))
            self.assertEqual(len(case.calls), before)
