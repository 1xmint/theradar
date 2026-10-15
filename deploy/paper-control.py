#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Private paper broker control. Model UID has no policy or execution capability."""

from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import socket
import stat
import struct
import subprocess
import tempfile
import threading
import time
import uuid

CONFIG = Path("/etc/radar/alpaca-paper-control.json")
MODEL = Path("/etc/radar/alpaca-paper-model.json")
READER = Path("/usr/local/lib/radar-paper-control/radar")
READER_HASH = "47b0448dd16b89818bcf65551745814161cc7ac082e1ff9618c143b409c30a3f"
STORE = Path("/var/lib/radar-paper-execution")
VERSION = "alpaca-paper-control-v1"
SYMBOLS = ["QQQ", "SPY"]
MAX = (1 << 64) - 1
RESEARCH_VERSION = "alpaca-paper-research-v1"


def research_message(value):
    fields = {"id", "role", "text", "received_at_unix_ms", "reply_to"}
    if (not isinstance(value, dict) or set(value) != fields
            or not isinstance(value["id"], str) or not re.fullmatch(r"radar_research_[A-Za-z0-9_-]{1,80}", value["id"])
            or value["role"] not in ("user", "assistant")
            or not isinstance(value["text"], str) or not value["text"].strip() or len(value["text"].encode()) > 4000
            or not integer(value["received_at_unix_ms"], 1)
            or (value["role"] == "user" and value["reply_to"] is not None)
            or (value["role"] == "assistant" and (not isinstance(value["reply_to"], str)
                or not re.fullmatch(r"radar_research_[A-Za-z0-9_-]{1,80}", value["reply_to"])))):
        raise ValueError("Invalid inert research message")
    return value


def decode(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("Duplicate control field")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique,
        parse_constant=lambda value: (_ for _ in ()).throw(ValueError("Nonfinite control value")))


def read(path, limit=65536):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Expected regular control file")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Control file exceeds bound")
    return raw


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def submission_refused(value, client):
    # Only the pinned executor can establish this pre-reservation boundary.
    # Missing IDs, stderr and transport errors never establish non-dispatch.
    if not isinstance(value, dict) or value.get("submission") != "refused_before_reservation":
        return False
    fields = {"venue", "environment", "client_order_id", "submission", "refusal_reason", "broker_order_id",
        "reservation_created", "network_attempted", "fills_reconciled", "retry_authorized",
        "claim_release_authorized", "paper_execution_model"}
    scenario = value.get("paper_execution_model")
    if (set(value) != fields or value.get("venue") != "alpaca" or value.get("environment") != "paper"
            or value.get("client_order_id") != client or value.get("broker_order_id") is not None
            or value.get("refusal_reason") not in ("risk", "authorization")
            or any(value.get(field) is not False for field in ("reservation_created", "network_attempted",
                "fills_reconciled", "retry_authorized", "claim_release_authorized"))
            or not isinstance(scenario, dict) or scenario.get("environment") != "paper"
            or any(scenario.get(field) is not False for field in
                ("live_capacity_verified", "live_costs_verified", "profit_measured"))):
        raise ValueError("Invalid pre-reservation refusal receipt")
    return True


def publish(path, value, exclusive=False):
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ValueError("Unexpected control target")
    descriptor, temporary = tempfile.mkstemp(prefix=".paper-control-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(encoded(value))
            stream.flush()
            os.fchmod(stream.fileno(), 0o600)
            os.fsync(stream.fileno())
        if exclusive:
            os.link(temporary, path)
        else:
            os.replace(temporary, path)
        descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def protected(path):
    for parent in path.parents:
        info = parent.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise ValueError("Unprotected broker installation")
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
        raise ValueError("Unprotected broker installation")


def text(value, maximum=256):
    return isinstance(value, str) and 0 < len(value) <= maximum and value.isascii() and all(32 < ord(c) < 127 for c in value)


def integer(value, minimum=0):
    return type(value) is int and minimum <= value <= MAX


def amount(value, positive=True):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9]{1,14}(?:\.[0-9]{1,6})?", value):
        raise ValueError("Expected fixed USD decimal")
    major, _, minor = value.partition(".")
    result = int(major) * 1_000_000 + int(minor.ljust(6, "0"))
    if result > MAX or (positive and result == 0):
        raise ValueError("Invalid USD bound")
    return result


def preferences(value):
    amounts = {"capital_usd", "max_trade_usd", "daily_loss_usd"}
    if (not isinstance(value, dict) or set(value) != amounts | {"daily_loss_enabled", "agent_decides"}
            or type(value["daily_loss_enabled"]) is not bool
            or not isinstance(value["agent_decides"], dict) or set(value["agent_decides"]) != amounts
            or any(type(flag) is not bool for flag in value["agent_decides"].values())
            or any(not isinstance(value[field], str) or len(value[field]) > 64 for field in amounts)):
        raise ValueError("Invalid paper preferences")
    selected = {}
    for field in amounts:
        active = field != "daily_loss_usd" or value["daily_loss_enabled"]
        selected[field] = amount(value[field]) if active and not value["agent_decides"][field] else None
    capital = selected["capital_usd"]
    if capital is not None and any(limit is not None and limit > capital
            for limit in (selected["max_trade_usd"], selected["daily_loss_usd"])):
        raise ValueError("Manual limits exceed manual capital")
    return selected


def policy(account, prefs=None, equity=None, enabled=False):
    selected = preferences(prefs) if prefs is not None else None
    capital = min(selected["capital_usd"] or equity, equity) if selected and equity else 0
    order = min(selected["max_trade_usd"] or capital, capital) if selected else 0
    loss = (selected["daily_loss_usd"] or capital) if selected and prefs["daily_loss_enabled"] else None
    return dict(enabled=enabled and capital > 0 and order > 0, account_id=account, symbols=SYMBOLS,
        max_capital_microusd=capital, max_order_microusd=order, max_position_microusd=capital,
        max_daily_loss_microusd=loss, max_input_age_ms=120000, authorization_lifetime_ms=1000,
        max_spread_bps=20, max_limit_distance_bps=20, max_round_trip_cost_bps=100)


class Control:
    def __init__(self, config, roles, store=STORE, run=subprocess.run, clock=time.time):
        self.config, self.roles, self.store, self.run, self.clock = config, roles, store, run, clock
        self.state = store / "control.json"

    def policy_path(self, generation):
        if not integer(generation, 1):
            raise ValueError("Invalid policy generation")
        return self.store / f"effective-policy-{generation}.json"

    @contextmanager
    def locked(self, execution=False):
        name = ".lock" if execution else ".control.lock"
        flags = os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK | (0 if execution else os.O_CREAT)
        descriptor = os.open(self.store / name, flags, 0o600)
        with os.fdopen(descriptor, "rb") as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                raise ValueError("Invalid broker lock")
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
            yield

    def retained(self, integrity=True):
        try:
            value = decode(read(self.state))
        except FileNotFoundError:
            if any(self.store.glob("effective-policy-*.json")):
                raise ValueError("Missing control history beside retained policy")
            return dict(version=VERSION, generation=0, operator=None, preferences=None,
                enabled=False, policy_sha256=None)
        if (not isinstance(value, dict) or set(value) != {"version", "generation", "operator", "preferences", "enabled", "policy_sha256"}
                or value["version"] != VERSION or not integer(value["generation"], 1)
                or type(value["enabled"]) is not bool
                or not isinstance(value["policy_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", value["policy_sha256"])):
            raise ValueError("Invalid retained control state")
        self.operator(value["operator"])
        if value["preferences"] is not None:
            preferences(value["preferences"])
        elif value["enabled"]:
            raise ValueError("Enabled control lacks preferences")
        if integrity and hashlib.sha256(read(self.policy_path(value["generation"]))).hexdigest() != value["policy_sha256"]:
            raise ValueError("Policy publication unresolved; owner must inspect")
        return value

    def operator(self, value, retained=None):
        if (not isinstance(value, dict) or set(value) != {"email", "subject"}
                or value["email"] != self.config["operator_email"] or not text(value["subject"])
                or (retained and retained["operator"] is not None and retained["operator"] != value)):
            raise ValueError("Paper operator differs")

    def execute(self, args):
        environment = {"PATH": "/usr/bin:/bin"}
        for name in ("RADAR_ALPACA_PAPER_KEY_ID", "RADAR_ALPACA_PAPER_SECRET_KEY"):
            if name in os.environ:
                environment[name] = os.environ[name]
        result = self.run([str(READER), *args], check=True, capture_output=True, text=True,
            timeout=75, cwd="/", env=environment)
        if len(result.stdout.encode()) > 4 * 1024 * 1024:
            raise ValueError("Broker response exceeds bound")
        return decode(result.stdout)

    def invoke(self, args):
        result = self.execute(args)
        if not isinstance(result, dict) or result.get("venue") != "alpaca" or result.get("environment") != "paper":
            raise ValueError("Invalid paper response")
        return result

    def observe(self, request):
        if set(request) != {"action"}:
            raise ValueError("Invalid observation request")
        with self.locked(execution=True):
            with self.locked():
                prior = self.retained()
                selected = decode(read(self.policy_path(prior["generation"]))) if prior["generation"] else None
            report = self.invoke(["alpaca-paper-maintain", "--journal", str(self.store / "orders.jsonl"),
                "--account-id", self.config["account_id"], "--max-pages", "2"])
            classes = ("working_before_cancellation", "unresolved", "cancellation_pending_before_tick", "terminal")
            if (report.get("expected_account_id") != self.config["account_id"]
                    or report.get("checkpoint_refreshed") is not True
                    or any(report.get(field) is not False for field in
                        ("new_order_authority", "retry_authorized", "claim_release_authorized", "profit_measured"))
                    or any(not isinstance(report.get(field), list) or any(not text(item) for item in report[field]) for field in classes)):
                raise ValueError("Observation reconciliation unavailable")
            account, _ = self.account()
            quotes = {}
            at = int(self.clock() * 1000)
            if prior["enabled"] and account["market_clock"].get("is_open") is True:
                self.storage()
                descriptor, temporary = tempfile.mkstemp(prefix=".paper-market-", dir=self.store)
                os.close(descriptor)
                os.unlink(temporary)  # Only this call's placeholder; CLI requires a new capture.
                try:
                    written = self.execute(["alpaca-market-read", "--symbols", "QQQ,SPY", "--output", temporary])
                    if (not isinstance(written, dict) or written.get("capture_written") is not True
                            or written.get("execution_authority") is not False or written.get("feed") != "iex"):
                        raise ValueError("Market capture unavailable")
                    at = int(self.clock() * 1000)
                    for symbol in SYMBOLS:
                        quote = self.execute(["alpaca-quote", "--input", temporary, "--symbol", symbol,
                            "--as-of-unix-ms", str(at), "--max-age-ms", "120000", "--max-spread-bps", "20"])
                        if (not isinstance(quote, dict) or quote.get("venue") != "alpaca" or quote.get("feed") != "iex"
                                or quote.get("symbol") != symbol or quote.get("execution_authority") is not False
                                or quote.get("as_of_unix_ms") != at):
                            raise ValueError("Validated market quote unavailable")
                        quotes[symbol] = quote
                finally:
                    if os.path.lexists(temporary):
                        os.unlink(temporary)
            with self.locked():
                current = self.retained()
                if current != prior:
                    raise ValueError("Owner policy changed during observation")
                research = self.research_snapshot(at)
            raw_model = read(MODEL)
            if hashlib.sha256(raw_model).hexdigest() != self.config["model_sha256"]:
                raise ValueError("Paper scenario changed")
            return dict(version="alpaca-paper-observation-v1", venue="alpaca", environment="paper",
                account_id=self.config["account_id"], generation=prior["generation"],
                handover_enabled=prior["enabled"], effective_policy=selected,
                account=account["account"], positions=account["positions"], market_clock=account["market_clock"],
                observed_at_unix_ms=at, quotes=quotes, paper_execution_model=decode(raw_model),
                orders={field:report[field] for field in classes},
                research=research,
                execution_authority=False, learned_edge_verified=False, profit_measured=False)

    def research_state(self):
        origin_path, state_path = self.store / "research-origin.json", self.store / "research-messages.json"
        try:
            origin = decode(read(origin_path))
        except FileNotFoundError:
            if os.path.lexists(state_path):
                raise ValueError("Research origin missing; preserve history")
            return None
        if (not isinstance(origin, dict) or set(origin) != {"version", "account_id", "operator"}
                or origin["version"] != RESEARCH_VERSION or origin["account_id"] != self.config["account_id"]):
            raise ValueError("Invalid research origin")
        self.operator(origin["operator"])
        value = decode(read(state_path, 1024 * 1024))
        if (not isinstance(value, dict) or set(value) != {"version", "messages"}
                or value["version"] != RESEARCH_VERSION or not isinstance(value["messages"], list)
                or len(value["messages"]) > 128):
            raise ValueError("Invalid research history")
        identities, users, answered, prior = set(), set(), set(), 0
        for item in value["messages"]:
            research_message(item)
            if item["id"] in identities or item["received_at_unix_ms"] < prior:
                raise ValueError("Ambiguous research history")
            identities.add(item["id"])
            prior = item["received_at_unix_ms"]
            if item["role"] == "user": users.add(item["id"])
            elif item["reply_to"] not in users or item["reply_to"] in answered:
                raise ValueError("Research reply lacks a unique prior question")
            else: answered.add(item["reply_to"])
        return origin, value

    def research_snapshot(self, at, limit=6):
        result = dict(version=RESEARCH_VERSION, account_id=self.config["account_id"], as_of_unix_ms=at,
            available=False, messages=[], truncated=False, execution_authority=False, source_claims_verified=False)
        try:
            retained = self.research_state()
            if retained is None: return result
            origin, state = retained
            owner = self.retained()["operator"]
            if owner is not None and owner != origin["operator"]: return result
            messages = [item for item in state["messages"] if item["received_at_unix_ms"] <= at]
            return dict(result, available=True, messages=messages[-limit:], truncated=len(messages) > limit)
        except (ValueError, OSError):
            # Optional research corruption cannot disable the owner's pause or
            # masquerade as no tips. It supplies explicit unknown context only.
            return result

    def research(self, request):
        action = request.get("action")
        fields = {"action", "operator"} if action == "research_read" else {"action", "operator", "message"}
        if set(request) != fields:
            raise ValueError("Invalid research envelope")
        with self.locked():
            self.operator(request["operator"], self.retained())
            retained = self.research_state()
            if retained is not None and retained[0]["operator"] != request["operator"]:
                raise ValueError("Research belongs to another operator")
            at = int(self.clock() * 1000)
            if action == "research_read": return self.research_snapshot(at, limit=128)
            incoming = request["message"]
            if not isinstance(incoming, dict) or set(incoming) != {"id", "role", "text", "reply_to"}:
                raise ValueError("Research receive time is assigned by the controller")
            message = research_message(dict(incoming, received_at_unix_ms=at))
            role = "user" if action == "research_add" else "assistant"
            if message["role"] != role:
                raise ValueError("Research role or receive time differs")
            origin, state = retained if retained is not None else (dict(version=RESEARCH_VERSION,
                account_id=self.config["account_id"], operator=request["operator"]), dict(version=RESEARCH_VERSION, messages=[]))
            existing = next((item for item in state["messages"] if item["id"] == message["id"]), None)
            if existing is not None:
                if any(existing[field] != message[field] for field in ("role", "text", "reply_to")):
                    raise ValueError("Research identity already retained with different content")
                return self.research_snapshot(at, limit=128)
            if (len(state["messages"]) >= 128
                    or (state["messages"] and state["messages"][-1]["received_at_unix_ms"] > at)):
                raise ValueError("Research retention or clock bound reached")
            if role == "assistant" and (not any(item["id"] == message["reply_to"] and item["role"] == "user" for item in state["messages"])
                    or any(item["reply_to"] == message["reply_to"] for item in state["messages"])):
                raise ValueError("Research response requires an unanswered question")
            self.storage()
            if retained is None:
                publish(self.store / "research-origin.json", origin, exclusive=True)
                publish(self.store / "research-messages.json", state, exclusive=True)
            publish(self.store / "research-messages.json", dict(state, messages=[*state["messages"], message]))
            return self.research_snapshot(at, limit=128)

    def cancel(self, request):
        if (set(request) != {"action", "generation", "client_order_id"}
                or not integer(request.get("generation"), 1) or not text(request.get("client_order_id"))):
            raise ValueError("Invalid cancellation proposal")
        with self.locked(execution=True):
            with self.locked():
                prior = self.retained()
                if not prior["enabled"] or prior["generation"] != request["generation"]:
                    raise ValueError("Cancellation handover closed or changed")
            # Existing CLI binds account, retained intent and broker payload;
            # it persists cancellation uncertainty before its one fixed DELETE.
            result = self.invoke(["alpaca-paper-cancel", "--journal", str(self.store / "orders.jsonl"),
                "--account-id", self.config["account_id"], "--client-order-id", request["client_order_id"]])
            if (result.get("client_order_id") != request["client_order_id"] or not text(result.get("broker_order_id"))
                    or result.get("cancellation") != "request_accepted"
                    or any(result.get(field) is not False for field in
                        ("cancellation_confirmed", "fills_reconciled", "retry_authorized", "claim_release_authorized"))):
                raise ValueError("Paper cancellation requires reconciliation")
            return result

    def account(self):
        value = self.invoke(["alpaca-read"])
        account = value.get("account")
        now = int(self.clock())
        start, finish = value.get("read_started_unix_secs"), value.get("read_finished_unix_secs")
        if (value.get("source") != "https://paper-api.alpaca.markets" or value.get("execution_authority") is not False
                or type(value.get("version")) is not int or value["version"] != 1
                or not isinstance(account, dict) or account.get("id") != self.config["account_id"]
                or not integer(start) or not integer(finish) or not 0 <= start <= finish <= now or now - start > 120
                or account.get("status") != "ACTIVE" or account.get("currency") != "USD"
                or any(account.get(field) is not False for field in
                    ("account_blocked", "trading_blocked", "trade_suspended_by_user"))):
            raise ValueError("Pinned paper account is unavailable or restricted")
        equity = amount(account.get("equity"), positive=False)
        amount(account.get("cash"), positive=False)
        return value, equity

    def persist(self, value, selected, previous=None):
        # Disable the old policy first. During a partial publication, digest
        # mismatch refuses new dispatches; no retry adopts an unknown write.
        old = value["generation"] if previous is None else previous
        if old:
            publish(self.policy_path(old), policy(self.config["account_id"]))
        value = {**value, "policy_sha256":hashlib.sha256(encoded(selected)).hexdigest()}
        publish(self.policy_path(value["generation"]), selected, exclusive=previous is not None)
        publish(self.state, value)
        return value

    def status(self):
        with self.locked():
            value = self.retained()
            selected = decode(read(self.policy_path(value["generation"]))) if value["generation"] else None
        account, _ = self.account()
        return dict(version=VERSION, environment="paper", account_id=self.config["account_id"],
            generation=value["generation"], preferences=value["preferences"],
            handover_enabled=value["enabled"], effective_policy=selected, account=account["account"],
            positions=account.get("positions"), market_clock=account.get("market_clock"),
            execution_authority=False, agent_lifecycle_verified=False, profit_measured=False)

    def change(self, request, pause=False):
        expected = {"action", "generation", "operator"} | (set() if pause else {"preferences", "enabled"})
        if set(request) != expected or not integer(request.get("generation")):
            raise ValueError("Invalid control change")
        if not pause:
            preferences(request["preferences"])
            if type(request["enabled"]) is not bool:
                raise ValueError("Expected explicit handover boolean")
        self.operator(request["operator"])
        _, equity = self.account() if not pause and request["enabled"] else (None, None)
        with self.locked():
            # An authenticated newer revision can repair an interrupted control
            # publication; it never resets the broker order journal or generation.
            prior = self.retained(integrity=False)
            self.operator(request["operator"], prior)
            if prior["generation"] != request["generation"] or prior["generation"] == MAX:
                raise ValueError("Stale control generation")
            prefs = prior["preferences"] if pause else request["preferences"]
            enabled = False if pause else request["enabled"]
            selected = policy(self.config["account_id"], prefs, equity, enabled)
            self.storage()
            value = self.persist(dict(version=VERSION, generation=prior["generation"] + 1,
                operator=request["operator"], preferences=prefs, enabled=selected["enabled"]), selected,
                previous=prior["generation"])
        return dict(version=VERSION, environment="paper", generation=value["generation"],
            handover_enabled=value["enabled"], execution_authority=False,
            pending_orders_canceled=False, positions_liquidated=False)

    def submit(self, request):
        if set(request) != {"action", "generation", "intent"} or not integer(request.get("generation"), 1):
            raise ValueError("Invalid paper proposal envelope")
        intent = request["intent"]
        fields = {"account_id", "client_order_id", "symbol", "side", "quantity_nanoshares", "limit_microusd_per_share", "fee_bound_microusd"}
        if (not isinstance(intent, dict) or set(intent) != fields or intent["account_id"] != self.config["account_id"]
                or intent["symbol"] not in SYMBOLS or intent["side"] not in ("buy", "sell")
                or not isinstance(intent["client_order_id"], str) or not re.fullmatch(r"radar_[A-Za-z0-9_-]{1,80}", intent["client_order_id"])
                or any(not integer(intent[field], 1) for field in
                    ("quantity_nanoshares", "limit_microusd_per_share", "fee_bound_microusd"))):
            raise ValueError("Invalid bounded paper intent")
        with self.locked(execution=True):
            with self.locked():
                prior = self.retained()
                if not prior["enabled"] or prior["generation"] != request["generation"]:
                    raise ValueError("Paper handover closed or changed")
            _, equity = self.account()
            with self.locked():
                current = self.retained()
                if not current["enabled"] or current["generation"] != prior["generation"]:
                    raise ValueError("Paper handover changed during collection")
                selected = policy(self.config["account_id"], current["preferences"], equity, True)
                self.persist(current, selected)
            self.storage()
            descriptor, temporary = tempfile.mkstemp(prefix=".paper-intent-", dir=self.store)
            try:
                with os.fdopen(descriptor, "wb") as stream:
                    stream.write(encoded(intent))
                    stream.flush()
                    os.fsync(stream.fileno())
                result = self.invoke(["alpaca-paper-submit", "--journal", str(self.store / "orders.jsonl"),
                    "--intent", temporary, "--policy", str(self.policy_path(current["generation"])),
                    "--max-pages", "2", "--paper-model", str(MODEL)])
                if submission_refused(result, intent["client_order_id"]):
                    return result
                scenario = result.get("paper_execution_model")
                if (result.get("client_order_id") != intent["client_order_id"]
                        or result.get("submission") != "response_payload_matched"
                        or not text(result.get("broker_order_id")) or result.get("fills_reconciled") is not False
                        or result.get("retry_authorized") is not False or result.get("claim_release_authorized") is not False
                        or not isinstance(scenario, dict) or scenario.get("environment") != "paper"
                        or any(scenario.get(field) is not False for field in
                            ("live_capacity_verified", "live_costs_verified", "profit_measured"))):
                    raise ValueError("Paper submission requires reconciliation")
                return result
            finally:
                os.unlink(temporary)

    def storage(self):
        total, count = 0, 0
        for path in self.store.iterdir():
            info = path.lstat()
            if not stat.S_ISREG(info.st_mode):
                raise ValueError("Unsupported broker storage entry")
            total += info.st_size
            count += 1
        if total + 4 * 1024 * 1024 > 256 * 1024 * 1024 or count >= 4096:
            raise ValueError("Broker storage reservation unavailable")
        descriptor = os.open(self.store / "orders.jsonl", os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            info = os.fstat(descriptor)
            if not stat.S_ISREG(info.st_mode) or info.st_size == 0:
                raise ValueError("Existing broker history required")
        finally:
            os.close(descriptor)

    def dispatch(self, uid, request):
        role = self.roles.get(uid)
        if role is None or not isinstance(request, dict):
            raise ValueError("Paper caller refused")
        action = request.get("action")
        if action == "status" and set(request) == {"action"}:
            return self.status()
        if role == "web" and action in ("configure", "pause"):
            return self.change(request, pause=action == "pause")
        if role == "web" and action in ("research_read", "research_add", "research_reply"):
            return self.research(request)
        if role == "supervisor" and action == "submit":
            return self.submit(request)
        if role == "supervisor" and action == "observe":
            return self.observe(request)
        if role == "supervisor" and action == "cancel":
            return self.cancel(request)
        raise ValueError("Paper operation refused for caller")


def configured():
    for path in (CONFIG, MODEL, READER):
        protected(path)
    value = decode(read(CONFIG))
    if (not isinstance(value, dict) or set(value) != {"version", "account_id", "operator_email", "model_sha256"}
            or value["version"] != VERSION or not text(value["operator_email"], 254)
            or "@" not in value["operator_email"] or not isinstance(value["account_id"], str)
            or str(uuid.UUID(value["account_id"])) != value["account_id"]
            or not isinstance(value["model_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", value["model_sha256"])
            or hashlib.sha256(read(MODEL)).hexdigest() != value["model_sha256"]
            or hashlib.sha256(read(READER, 32 * 1024 * 1024)).hexdigest() != READER_HASH):
        raise ValueError("Expected pinned protected paper configuration")
    model = decode(read(MODEL))
    if (not isinstance(model, dict) or set(model) != {"version", "account_id", "max_quantity_nanoshares", "fee_per_side_microusd", "slippage_bps_per_side"}
            or model["version"] != "alpaca-paper-stress-v1" or model["account_id"] != value["account_id"]
            or not integer(model["max_quantity_nanoshares"], 1) or not integer(model["fee_per_side_microusd"], 1)
            or type(model["slippage_bps_per_side"]) is not int or not 1 <= model["slippage_bps_per_side"] <= 10000):
        raise ValueError("Expected account-bound paper scenario")
    identities = [pwd.getpwnam(name).pw_uid for name in ("guardian", "radar-paper-supervisor", "radar-agent")]
    if len(set(identities)) != 3:
        raise ValueError("Paper principals must be separate")
    control = Control(value, {identities[0]:"web", identities[1]:"supervisor"}, store=STORE)
    control.storage()
    descriptor = os.open(STORE / ".lock", os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ValueError("Existing observation lock required")
    finally:
        os.close(descriptor)
    return control


def serve_one(connection, control):
    try:
        _, uid, _ = struct.unpack("3i", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if uid not in control.roles:
            raise ValueError("Paper caller refused")
        connection.settimeout(5)
        with connection.makefile("rb") as reader:
            line = reader.readline(65537)
        if len(line) > 65536 or not line.endswith(b"\n"):
            raise ValueError("Incomplete or oversized control request")
        result = control.dispatch(uid, decode(line))
        packet = {"result":result}
    except Exception:
        # Never echo keys, vendor bodies, request text or process stderr. A lost
        # response can mean an accepted write/order: observe before any retry.
        packet = {"error":"Paper control outcome unresolved; read status and reconcile before retry"}
    try:
        connection.sendall(encoded(packet) + b"\n")
    except OSError:
        pass
    finally:
        connection.close()


def serve():
    if os.environ.get("LISTEN_PID") != str(os.getpid()) or os.environ.get("LISTEN_FDS") != "1":
        raise SystemExit("Start through radar-paper-control.socket")
    control = configured()
    listener = socket.socket(fileno=3)
    slots = threading.BoundedSemaphore(8)
    def handle(connection):
        try:
            serve_one(connection, control)
        finally:
            slots.release()
    while True:
        connection, _ = listener.accept()
        if not slots.acquire(blocking=False):
            connection.close()
            continue
        threading.Thread(target=handle, args=(connection,), daemon=True).start()


if __name__ == "__main__":
    serve()
