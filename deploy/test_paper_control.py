# SPDX-License-Identifier: Apache-2.0
"""Private paper IPC roles, revocation generations, publication and fixed execution."""

from contextlib import contextmanager
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import tempfile
import threading
import types
import unittest
from unittest.mock import call, patch

spec = importlib.util.spec_from_file_location("paper_control", Path(__file__).with_name("paper-control.py"))
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)
ACCOUNT = "11111111-1111-4111-8111-111111111111"
OPERATOR = {"email":"owner@example.com", "subject":"owner-1"}


def prefs():
    return dict(capital_usd="50", max_trade_usd="10", daily_loss_usd="5", daily_loss_enabled=False,
        agent_decides=dict(capital_usd=False, max_trade_usd=False, daily_loss_usd=False))


def intent():
    return dict(account_id=ACCOUNT, client_order_id="radar_once", symbol="SPY", side="buy",
        quantity_nanoshares=10_000_000, limit_microusd_per_share=100_000_000, fee_bound_microusd=10000)


def observation():
    return dict(version=1, venue="alpaca", environment="paper", source="https://paper-api.alpaca.markets",
        execution_authority=False, read_started_unix_secs=999, read_finished_unix_secs=1000,
        account=dict(id=ACCOUNT, status="ACTIVE", currency="USD", cash="100", equity="100",
            account_blocked=False, trading_blocked=False, trade_suspended_by_user=False),
        positions=[], market_clock={"is_open":True})


class PaperControlTests(unittest.TestCase):
    @contextmanager
    def context(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = b"retained-broker-history\n"
            (root / "orders.jsonl").write_bytes(original)
            (root / ".lock").write_bytes(b"genesis-fence")
            calls = []
            def run(argv, **kwargs):
                calls.append((argv, kwargs))
                if argv[1:] == ["alpaca-read"]:
                    return types.SimpleNamespace(stdout=json.dumps(observation()))
                self.assertEqual(argv[1], "alpaca-paper-submit")
                proposal = json.loads(Path(argv[argv.index("--intent") + 1]).read_text())
                return types.SimpleNamespace(stdout=json.dumps(dict(venue="alpaca", environment="paper",
                    client_order_id=proposal["client_order_id"], broker_order_id="broker-1",
                    submission="response_payload_matched", fills_reconciled=False,
                    retry_authorized=False, claim_release_authorized=False,
                    paper_execution_model=dict(environment="paper", live_capacity_verified=False,
                        live_costs_verified=False, profit_measured=False))))
            instance = control.Control(dict(account_id=ACCOUNT, operator_email=OPERATOR["email"]),
                {1100:"web", 1200:"supervisor"}, store=root, run=run, clock=lambda:1000)
            yield types.SimpleNamespace(root=root, original=original, calls=calls, instance=instance, run=run)

    def enable(self, case, values=None, generation=0):
        return case.instance.dispatch(1100, dict(action="configure", generation=generation,
            operator=OPERATOR, preferences=prefs() if values is None else values, enabled=True))

    def test_executor_refusal_receipt_is_validated_before_forwarding(self):
        receipt = dict(venue="alpaca", environment="paper", client_order_id="radar_once", broker_order_id=None,
            submission="refused_before_reservation", refusal_reason="risk", reservation_created=False,
            network_attempted=False, fills_reconciled=False, retry_authorized=False, claim_release_authorized=False,
            paper_execution_model=dict(environment="paper", live_capacity_verified=False,
                live_costs_verified=False, profit_measured=False))
        for field, wrong in [(None, None), ("client_order_id", "radar_other"), ("environment", "live"),
                ("venue", "other"), ("broker_order_id", "broker-1"), ("refusal_reason", "timeout"),
                ("reservation_created", True), ("network_attempted", True), ("fills_reconciled", True),
                ("retry_authorized", True), ("claim_release_authorized", True), ("paper_execution_model", {}),
                ("unexpected", False)]:
            with self.subTest(field=field), self.context() as case:
                self.enable(case)
                value = receipt if field is None else dict(receipt, **{field:wrong})
                def run(argv, **kwargs):
                    if argv[1] == "alpaca-paper-submit":
                        return types.SimpleNamespace(stdout=json.dumps(value))
                    return case.run(argv, **kwargs)
                case.instance.run = run
                request = dict(action="submit", generation=1, intent=intent())
                if field is None:
                    self.assertEqual(case.instance.dispatch(1200, request), receipt)
                else:
                    with self.assertRaises(ValueError): case.instance.dispatch(1200, request)
                self.assertEqual((case.root / "orders.jsonl").read_bytes(), case.original)
        self.assertFalse(control.submission_refused({"error":"risk refused"}, "radar_once"))

    def test_fixed_decimal_manual_and_delegated_policy_bounds(self):
        self.assertEqual(control.amount("50.000001"), 50_000_001)
        self.assertEqual(control.amount("0", positive=False), 0)
        for value in ("0", "-1", "1e3", "NaN", "1.0000001", " 1", True, "99999999999999"):
            with self.assertRaises(ValueError): control.amount(value)
        value = prefs()
        selected = control.policy(ACCOUNT, value, 30_000_000, True)
        self.assertEqual((selected["max_capital_microusd"], selected["max_order_microusd"]), (30_000_000, 10_000_000))
        self.assertIsNone(selected["max_daily_loss_microusd"])
        value["agent_decides"] = {field:True for field in value["agent_decides"]}
        value["daily_loss_enabled"] = True
        selected = control.policy(ACCOUNT, value, 20_000_000, True)
        self.assertEqual((selected["max_capital_microusd"], selected["max_order_microusd"],
            selected["max_daily_loss_microusd"]), (20_000_000,) * 3)
        self.assertFalse(control.policy(ACCOUNT, value, 0, True)["enabled"])
        for field, wrong in (("agent_decides", {}), ("daily_loss_enabled", 1), ("capital_usd", 50),
                ("max_trade_usd", "51"), ("unknown", "x")):
            changed = prefs()
            changed[field] = wrong
            with self.assertRaises(ValueError): control.preferences(changed)

    def test_roles_operator_and_stale_generation_refuse_without_dispatch_or_mutation(self):
        with self.context() as case:
            self.assertEqual(case.instance.dispatch(1100, {"action":"status"})["generation"], 0)
            case.calls.clear()
            change = dict(action="configure", generation=0, operator=OPERATOR, preferences=prefs(), enabled=True)
            for uid, request in ((1300, change), (1200, change), (1100, {"action":"submit", "generation":1, "intent":intent()}),
                    (1100, {**change, "operator":{**OPERATOR, "email":"other@example.com"}}),
                    (1100, {**change, "generation":True}), (1100, {**change, "enabled":1}),
                    (1100, {**change, "path":"/tmp/arbitrary"})):
                with self.assertRaises(ValueError): case.instance.dispatch(uid, request)
            self.assertEqual(case.calls, [])
            self.assertFalse(case.instance.state.exists())
            self.enable(case)
            before = case.instance.state.read_bytes()
            with self.assertRaises(ValueError):
                case.instance.dispatch(1100, dict(action="submit", generation=1, intent=intent()))
            for request in (change, {**change, "generation":1, "operator":{**OPERATOR, "subject":"other"}}):
                with self.assertRaises(ValueError): case.instance.dispatch(1100, request)
            self.assertEqual(case.instance.state.read_bytes(), before)
            self.assertEqual((case.root / "orders.jsonl").read_bytes(), case.original)

    def test_pause_and_resume_never_reenable_a_previous_generation_policy(self):
        with self.context() as case:
            self.assertTrue(self.enable(case)["handover_enabled"])
            first = case.instance.policy_path(1)
            self.assertTrue(json.loads(first.read_text())["enabled"])
            result = case.instance.dispatch(1100, dict(action="pause", generation=1, operator=OPERATOR))
            self.assertFalse(result["handover_enabled"])
            self.assertFalse(result["pending_orders_canceled"])
            with self.assertRaises(ValueError):
                case.instance.dispatch(1200, dict(action="submit", generation=2, intent=intent()))
            self.enable(case, generation=2)
            self.assertFalse(json.loads(first.read_text())["enabled"])
            self.assertTrue(json.loads(case.instance.policy_path(3).read_text())["enabled"])
            before = len(case.calls)
            with self.assertRaises(ValueError):
                case.instance.dispatch(1200, dict(action="submit", generation=1, intent=intent()))
            self.assertEqual(len(case.calls), before)
            status = case.instance.status()
            self.assertEqual(status["generation"], 3)
            self.assertFalse(status["execution_authority"])
            self.assertFalse(status["agent_lifecycle_verified"])

    def test_fixed_submit_uses_existing_history_clean_environment_and_never_retries(self):
        with self.context() as case, patch.dict(os.environ, {"PATH":"bad", "RADAR_ALPACA_PAPER_KEY_ID":"test-key",
                "RADAR_ALPACA_PAPER_SECRET_KEY":"test-secret", "PRIVATE_SENTINEL":"must-not-pass"}, clear=True):
            self.enable(case)
            report = case.instance.dispatch(1200, dict(action="submit", generation=1, intent=intent()))
            self.assertEqual(report["submission"], "response_payload_matched")
            argv, options = case.calls[-1]
            self.assertEqual(argv[0], str(control.READER))
            self.assertEqual(argv[1:4], ["alpaca-paper-submit", "--journal", str(case.root / "orders.jsonl")])
            self.assertEqual(argv[argv.index("--policy") + 1], str(case.instance.policy_path(1)))
            self.assertEqual(argv[-4:], ["--max-pages", "2", "--paper-model", str(control.MODEL)])
            self.assertEqual(options, dict(check=True, capture_output=True, text=True, timeout=75, cwd="/",
                env={"PATH":"/usr/bin:/bin", "RADAR_ALPACA_PAPER_KEY_ID":"test-key", "RADAR_ALPACA_PAPER_SECRET_KEY":"test-secret"}))
            self.assertFalse(Path(argv[argv.index("--intent") + 1]).exists())
            self.assertEqual(sum(call[0][1] == "alpaca-paper-submit" for call in case.calls), 1)
            self.assertEqual((case.root / "orders.jsonl").read_bytes(), case.original)
            for field, wrong in (("symbol", "BTC"), ("account_id", "other"), ("side", "withdraw"),
                    ("quantity_nanoshares", True), ("limit_microusd_per_share", 0), ("client_order_id", "../bad"), ("url", "live")):
                proposal = intent()
                proposal[field] = wrong
                before = len(case.calls)
                with self.assertRaises(ValueError):
                    case.instance.dispatch(1200, dict(action="submit", generation=1, intent=proposal))
                self.assertEqual(len(case.calls), before)

    def test_revoke_during_collection_or_inflight_submission_preserves_old_policy_refusal(self):
        for phase in ("collection", "submission"):
            with self.context() as case:
                self.enable(case)
                def run(argv, **kwargs):
                    if (phase == "collection" and argv[1] == "alpaca-read") or (phase == "submission" and argv[1] == "alpaca-paper-submit"):
                        case.instance.dispatch(1100, dict(action="pause", generation=1, operator=OPERATOR))
                        if phase == "submission":
                            with (case.root / "orders.jsonl").open("ab") as stream:
                                stream.write(b"submission_unknown\n")
                            self.enable(case, generation=2)
                            self.assertFalse(json.loads(Path(argv[argv.index("--policy") + 1]).read_text())["enabled"])
                            raise subprocess.CalledProcessError(1, argv, stderr="private-sentinel")
                    return case.run(argv, **kwargs)
                case.instance.run = run
                with self.assertRaises((ValueError, subprocess.CalledProcessError)):
                    case.instance.dispatch(1200, dict(action="submit", generation=1, intent=intent()))
                retained = (case.root / "orders.jsonl").read_bytes()
                self.assertTrue(retained.startswith(case.original))
                self.assertEqual(retained, case.original if phase == "collection" else case.original + b"submission_unknown\n")
                self.assertFalse(json.loads(case.instance.policy_path(1).read_text())["enabled"])

    def test_submission_failure_or_invalid_response_is_not_retried_or_discarded(self):
        for failure in ("process", "response"):
            with self.context() as case:
                self.enable(case)
                submitted = []
                def run(argv, **kwargs):
                    if argv[1] == "alpaca-paper-submit":
                        submitted.append(argv)
                        with (case.root / "orders.jsonl").open("ab") as stream:
                            stream.write(b"submission_unknown\n")
                        if failure == "process":
                            raise subprocess.CalledProcessError(1, argv, stderr="private-sentinel")
                        return types.SimpleNamespace(stdout=json.dumps({"venue":"alpaca", "environment":"paper"}))
                    return case.run(argv, **kwargs)
                case.instance.run = run
                with self.assertRaises((ValueError, subprocess.CalledProcessError)):
                    case.instance.dispatch(1200, dict(action="submit", generation=1, intent=intent()))
                self.assertEqual(len(submitted), 1)
                self.assertEqual((case.root / "orders.jsonl").read_bytes(), case.original + b"submission_unknown\n")
                self.assertEqual(list(case.root.glob(".paper-intent-*")), [])

    def test_partial_publication_and_missing_or_changed_state_do_not_reset_generations(self):
        with self.context() as case:
            self.enable(case)
            before = case.instance.state.read_bytes()
            real_publish = control.publish
            def publish(path, value, **kwargs):
                if path == case.instance.state:
                    raise OSError("uncertain write")
                return real_publish(path, value, **kwargs)
            with patch.object(control, "publish", side_effect=publish), self.assertRaises(OSError):
                case.instance.dispatch(1100, dict(action="pause", generation=1, operator=OPERATOR))
            self.assertEqual(case.instance.state.read_bytes(), before)
            self.assertFalse(json.loads(case.instance.policy_path(1).read_text())["enabled"])
            with self.assertRaises(ValueError): case.instance.status()
            with self.assertRaises(FileExistsError):
                case.instance.dispatch(1100, dict(action="pause", generation=1, operator=OPERATOR))
            case.instance.state.unlink()
            with self.assertRaises(ValueError): case.instance.retained()
            self.assertEqual((case.root / "orders.jsonl").read_bytes(), case.original)

    def test_wrong_stale_or_restricted_account_and_broken_storage_refuse_handover(self):
        for field, wrong in (("environment", "live"), ("source", "https://api.alpaca.markets"),
                ("read_started_unix_secs", 800), ("read_finished_unix_secs", 1001), ("execution_authority", True),
                ("id", "other"), ("currency", "EUR"), ("status", "SUSPENDED"), ("account_blocked", None),
                ("cash", "NaN"), ("equity", "1e6")):
            with self.context() as case:
                value = observation()
                (value if field in value else value["account"])[field] = wrong
                case.instance.run = lambda *args, **kwargs:types.SimpleNamespace(stdout=json.dumps(value))
                with self.assertRaises(ValueError): self.enable(case)
                self.assertFalse(case.instance.state.exists())
        with self.context() as case:
            (case.root / "orders.jsonl").unlink()
            with self.assertRaises(FileNotFoundError): self.enable(case)
            self.assertFalse((case.root / "orders.jsonl").exists())
            self.assertFalse(case.instance.state.exists())
            (case.root / "orders.jsonl").write_bytes(case.original)
            (case.root / "unexpected-directory").mkdir()
            with self.assertRaises(ValueError): self.enable(case)

    def test_socket_peer_credentials_strict_json_bounds_and_private_errors(self):
        def request(instance, raw):
            local, remote = socket.socketpair()
            worker = threading.Thread(target=control.serve_one, args=(remote, instance))
            with local:
                local.settimeout(5)
                local.sendall(raw)
                local.shutdown(socket.SHUT_WR)
                worker.start()
                with local.makefile("rb") as stream:
                    reply = stream.readline()
            worker.join(5)
            self.assertFalse(worker.is_alive())
            return json.loads(reply)
        with self.context() as case:
            case.instance.roles = {os.getuid():"web"}
            reply = request(case.instance, b'{"action":"status"}\n')
            self.assertEqual(reply["result"]["generation"], 0)
            before = len(case.calls)
            for raw in (b'{"action":"status","action":"submit"}\n', b'{"action":"shell","private":"sentinel"}\n',
                    b'{"action":"status"}', b"x" * 65537 + b"\n", b'{"action":NaN}\n'):
                reply = request(case.instance, raw)
                self.assertIn("error", reply)
                self.assertNotIn("sentinel", json.dumps(reply))
                self.assertEqual(len(case.calls), before)
            case.instance.roles = {os.getuid() + 1:"web"}
            self.assertIn("error", request(case.instance, b'{"action":"status"}\n'))
            self.assertEqual(len(case.calls), before)
            case.instance.roles = {os.getuid():"web"}
            def failed(*args, **kwargs):
                raise ValueError("private-sentinel")
            case.instance.run = failed
            self.assertNotIn("private-sentinel", json.dumps(request(case.instance, b'{"action":"status"}\n')))

    def test_configuration_pins_reader_model_account_principals_and_existing_history(self):
        with self.context() as case:
            config, model, reader = (case.root / name for name in ("config", "model", "reader"))
            scenario = dict(version="alpaca-paper-stress-v1", account_id=ACCOUNT, max_quantity_nanoshares=10,
                fee_per_side_microusd=10000, slippage_bps_per_side=10)
            raw = control.encoded(scenario)
            model.write_bytes(raw)
            reader.write_bytes(b"verified-reader")
            settings = dict(version=control.VERSION, account_id=ACCOUNT, operator_email=OPERATOR["email"],
                model_sha256=hashlib.sha256(raw).hexdigest())
            config.write_bytes(control.encoded(settings))
            identities = {"guardian":1100, "radar-paper-supervisor":1200, "radar-agent":1300}
            with patch.object(control, "CONFIG", config), patch.object(control, "MODEL", model), \
                    patch.object(control, "READER", reader), patch.object(control, "STORE", case.root), \
                    patch.object(control, "READER_HASH", hashlib.sha256(reader.read_bytes()).hexdigest()), \
                    patch.object(control, "protected") as protected, \
                    patch.object(control.pwd, "getpwnam", lambda name:types.SimpleNamespace(pw_uid=identities[name])):
                actual = control.configured()
                self.assertEqual(actual.roles, {1100:"web", 1200:"supervisor"})
                self.assertEqual(protected.call_args_list, [call(config), call(model), call(reader)])
                for path in (model, reader):
                    original = path.read_bytes()
                    path.write_bytes(b"different")
                    with self.assertRaises(ValueError): control.configured()
                    path.write_bytes(original)
                changed = {**scenario, "fee_per_side_microusd":10001}
                model.write_bytes(control.encoded(changed))
                with self.assertRaises(ValueError): control.configured()
                for field, wrong in (("version", "live"), ("account_id", "other"),
                        ("max_quantity_nanoshares", 0), ("fee_per_side_microusd", 0),
                        ("slippage_bps_per_side", True), ("slippage_bps_per_side", 0), ("slippage_bps_per_side", 10001)):
                    changed = {**scenario, field:wrong}
                    model.write_bytes(control.encoded(changed))
                    config.write_bytes(control.encoded({**settings, "model_sha256":hashlib.sha256(model.read_bytes()).hexdigest()}))
                    with self.assertRaises(ValueError): control.configured()
                model.write_bytes(raw)
                config.write_bytes(control.encoded(settings))
                identities["radar-agent"] = 1200
                with self.assertRaises(ValueError): control.configured()
                identities["radar-agent"] = 1300
                (case.root / ".lock").unlink()
                with self.assertRaises(FileNotFoundError): control.configured()

    def test_protected_regular_no_follow_inputs_and_exclusive_policy_publish(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "input"
            path.write_bytes(b"data")
            self.assertEqual(control.read(path), b"data")
            with self.assertRaises(ValueError): control.read(path, 1)
            link = root / "link"
            link.symlink_to(path)
            with self.assertRaises(OSError): control.read(link)
            target = root / "policy"
            control.publish(target, {"enabled":False}, exclusive=True)
            with self.assertRaises(FileExistsError): control.publish(target, {"enabled":True}, exclusive=True)
            self.assertFalse(json.loads(target.read_text())["enabled"])
            self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o600)
            safe_dir = types.SimpleNamespace(st_mode=stat.S_IFDIR | 0o755, st_uid=0)
            safe_file = types.SimpleNamespace(st_mode=stat.S_IFREG | 0o600, st_uid=0)
            with patch.object(Path, "lstat", lambda value:safe_file if value == path else safe_dir):
                control.protected(path)
            for mode, owner in ((stat.S_IFREG | 0o777, 0), (stat.S_IFREG | 0o600, 1), (stat.S_IFLNK | 0o600, 0)):
                bad = types.SimpleNamespace(st_mode=mode, st_uid=owner)
                with patch.object(Path, "lstat", lambda value:bad if value == path else safe_dir), self.assertRaises(ValueError):
                    control.protected(path)
