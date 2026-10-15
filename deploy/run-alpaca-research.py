#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""One campaign tick: retain every scheduled slot, never submit broker orders."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import time

READER = "/usr/local/lib/radar-research/radar"
STORE = Path("/var/lib/radar-paper-research")
CONFIG = Path("/etc/radar/alpaca-research.json")
MAX_BYTES = 256 * 1024 * 1024
RESERVATION = 4 * 1024 * 1024
MAX_FILES = 65536


def read(path, limit):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Nonregular research input")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Research input exceeds limit")
    return raw


def sync(directory):
    descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def publish(directory, name, value):
    # Exclusive creation: a partial/crashed write refuses subsequent replay.
    descriptor = os.open(directory / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(json.dumps(value, sort_keys=True).encode())
        stream.flush()
        os.fsync(stream.fileno())
    sync(directory)


def campaign(raw):
    value = json.loads(raw)
    fields = {"version", "symbol", "start_unix_ms", "interval_ms", "start_window_ms",
              "slots", "experiment", "assumptions"}
    if (not isinstance(value, dict) or set(value) != fields or value["version"] != "alpaca-research-v1"
            or value["symbol"] not in ("SPY", "QQQ")):
        raise ValueError("Invalid research campaign")
    for name in ("start_unix_ms", "interval_ms", "start_window_ms", "slots"):
        if type(value[name]) is not int:
            raise ValueError("Campaign requires integer schedule")
    if (not 0 < value["start_unix_ms"] < 2**63
            or not 120000 <= value["interval_ms"] <= 86400000
            or not 1000 <= value["start_window_ms"] <= 90000
            or not 1 <= value["slots"] <= 1000):
        raise ValueError("Campaign schedule out of bounds")
    experiment = value["experiment"]
    if (not isinstance(experiment, dict)
            or set(experiment) != {"strategy_id", "quantity_nanoshares", "horizon_ms"}
            or not isinstance(experiment["strategy_id"], str) or not experiment["strategy_id"].strip()
            or any(type(experiment[key]) is not int or not 0 < experiment[key] < 2**64
                   for key in ("quantity_nanoshares", "horizon_ms"))
            or not isinstance(value["assumptions"], dict)):
        raise ValueError("Invalid fixed research experiment")
    return value


def tick(directory, config, run, clock, environment, max_bytes=MAX_BYTES):
    descriptor = os.open(directory / ".lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        used = 0
        files = 0
        for path in directory.iterdir():
            info = path.lstat()
            if not stat.S_ISREG(info.st_mode):
                raise ValueError("Unsupported research storage entry")
            used += info.st_size
            files += 1
        raw = read(config, 65536)
        plan = campaign(raw)
        digest = hashlib.sha256(raw).hexdigest()
        frozen = directory / "campaign.json"
        if frozen.exists():
            if read(frozen, 65536) != raw:
                raise ValueError("Campaign changed; use a separate research store")
        else:
            if used + RESERVATION > max_bytes or files + 7 > MAX_FILES:
                return {"status": "storage_limit", "execution_authority": False}
            descriptor = os.open(frozen, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(descriptor, "wb") as stream:
                stream.write(raw)
                stream.flush()
                os.fsync(stream.fileno())
            sync(directory)
            used += len(raw)
            files += 1
        at = clock()
        statuses = []
        for index in range(plan["slots"]):
            scheduled = plan["start_unix_ms"] + index * plan["interval_ms"]
            prefix = f"slot-{index:04d}"
            begin = directory / (prefix + ".begin.json")
            end = directory / (prefix + ".end.json")
            if begin.exists():
                claimed = json.loads(read(begin, 65536))
                if claimed.get("campaign_sha256") != digest or claimed.get("scheduled_unix_ms") != scheduled:
                    raise ValueError("Attempt identity mismatch")
                status = "interrupted_unknown"
                if end.exists():
                    terminal = json.loads(read(end, 65536))
                    if terminal.get("status") not in ("decision", "failed", "missed"):
                        raise ValueError("Invalid research terminal status")
                    status = terminal["status"]
                statuses.append(status)
                continue
            if end.exists():
                raise ValueError("Terminal record without attempt")
            if scheduled > at:
                statuses.append("future")
                continue
            if used + RESERVATION > max_bytes or files + 6 > MAX_FILES:
                statuses.append("storage_limit")
                continue
            publish(directory, begin.name, {"campaign_sha256": digest, "scheduled_unix_ms": scheduled,
                                           "started_unix_ms": at, "execution_authority": False})
            status = "missed"
            if at < scheduled + plan["start_window_ms"]:
                status = attempt(directory, prefix, plan, run, environment)
            publish(directory, end.name, {"status": status, "finished_unix_ms": clock(),
                                         "execution_authority": False})
            statuses.append(status)
            # One slot per invocation bounds subprocesses and storage reservation.
            return {"status": "processed", "slot": index, "outcome": status,
                    "execution_authority": False}
        return {"status": "observed", "slots": statuses, "execution_authority": False}


def attempt(directory, prefix, plan, run, environment):
    capture = directory / (prefix + ".capture.json")
    decision = directory / (prefix + ".decision.json")
    experiment = prefix + ".experiment.json"
    assumptions = prefix + ".assumptions.json"
    publish(directory, experiment, plan["experiment"])
    publish(directory, assumptions, plan["assumptions"])
    # The model process never receives brokerage credentials. Its adapter also
    # clears inherited credentials before contacting the separate CLI bridge.
    market_env = {"PATH": "/usr/bin:/bin"}
    for name in ("RADAR_ALPACA_PAPER_KEY_ID", "RADAR_ALPACA_PAPER_SECRET_KEY"):
        if name in environment:
            market_env[name] = environment[name]
    model_env = {"PATH": "/usr/bin:/bin", "RADAR_MODEL_CODEX": "/usr/local/bin/radar-codex",
                 "RADAR_MODEL_DAILY_USD": "unlimited"}
    try:
        if plan["experiment"]["strategy_id"] == "snapshot-context-v1":
            session = run([READER, "alpaca-research-session", "--horizon-ms",
                           str(plan["experiment"]["horizon_ms"])],
                          check=True, capture_output=True, text=True, timeout=10, env=market_env)
            guard = json.loads(session.stdout)
            if (guard.get("environment") != "paper" or guard.get("execution_authority") is not False
                    or guard.get("regular_session_eligible") is not True
                    or type(guard.get("session_close_unix_ms")) is not int
                    or guard["session_close_unix_ms"] <= 0
                    or guard.get("horizon_ms") != plan["experiment"]["horizon_ms"]):
                raise ValueError("Research session not eligible")
        run([READER, "alpaca-market-read", "--symbols", plan["symbol"], "--output", str(capture)],
            check=True, capture_output=True, text=True, timeout=20, env=market_env)
        read(capture, 1024 * 1024)
        result = run([READER, "alpaca-paper-propose", "--capture", str(capture), "--symbol", plan["symbol"],
                      "--experiment", str(directory / experiment), "--assumptions", str(directory / assumptions),
                      "--output", str(decision)],
                     check=True, capture_output=True, text=True, timeout=100, env=model_env)
        report = json.loads(result.stdout)
        if (report.get("execution_authority") is not False
                or report.get("record", {}).get("decision_written") is not True):
            raise ValueError("Invalid research response")
        retained = read(decision, 2 * 1024 * 1024)
        if plan["experiment"]["strategy_id"] == "snapshot-context-v1":
            outcome = json.loads(retained).get("outcome_not_before_unix_ms")
            if type(outcome) is not int or outcome >= guard["session_close_unix_ms"]:
                raise ValueError("Recorded research outcome reaches session close")
        sync(directory)
        return "decision"
    except (subprocess.SubprocessError, OSError, ValueError, TypeError, AttributeError):
        # Do not persist stderr, vendor bodies or keys. A partial decision is not
        # adopted; the immutable failed slot still contributes to the cohort.
        return "failed"


def main():
    global READER
    directory, config = STORE, CONFIG
    if sys.argv[1:] == ["--context-campaign"]:
        READER = "/usr/local/lib/radar-context-research/radar"
        directory = Path("/var/lib/radar-paper-context")
        config = Path("/etc/radar/alpaca-context-research.json")
    elif sys.argv[1:]:
        raise ValueError("Research worker accepts no arguments")
    print(json.dumps(tick(directory, config, subprocess.run,
                          lambda: time.time_ns() // 1000000, os.environ), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit("Research worker refused; retained attempts unchanged, no trading authority")
