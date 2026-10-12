#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Read-only cohort inventory and fixed-horizon hypothetical comparisons."""

from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

READER = "/usr/local/lib/radar-research/radar"


def read(path, limit):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Expected regular research evidence")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Research evidence exceeds bound")
    return raw


def captures(tape):
    result = []
    size = 0
    for index, path in enumerate(tape.iterdir()):
        if index >= 65536:
            raise ValueError("Capture inventory exceeds bound")
        if path.name == ".lock":
            continue
        if not re.fullmatch(r"capture-[0-9a-f]{32}\.json", path.name):
            raise ValueError("Unexpected capture storage member")
        raw = read(path, 1024 * 1024)
        size += len(raw)
        if size > 256 * 1024 * 1024:
            raise ValueError("Capture bytes exceed bound")
        value = json.loads(raw)
        at = value.get("available_at_unix_ms")
        if (value.get("format") != "alpaca-iex-snapshot-v1" or type(at) is not int
                or not 0 < at < 2**64 or not isinstance(value.get("symbols"), list)):
            raise ValueError("Invalid capture inventory")
        result.append((at, path, value["symbols"]))
    return result


def score(decision, tape, symbol, horizon, run):
    eligible = [(at, path) for at, path, symbols in tape if at <= horizon and symbol in symbols]
    if not eligible:
        return {"status": "unscoreable", "reason": "no_available_exit_capture"}
    latest = max(at for at, _ in eligible)
    paths = [path for at, path in eligible if at == latest]
    if len(paths) != 1:
        return {"status": "unscoreable", "reason": "ambiguous_latest_capture"}
    try:
        result = run([READER, "alpaca-paper-decision-score", "--decision", str(decision),
                      "--exit", str(paths[0]), "--exit-as-of-unix-ms", str(horizon)],
                     check=True, capture_output=True, text=True, timeout=20, env={"PATH": "/usr/bin:/bin"})
        report = json.loads(result.stdout)
        if (report.get("status") not in ("scored", "unscoreable")
                or report.get("exit_as_of_unix_ms") != horizon
                or report.get("execution_authority") is not False):
            raise ValueError("Invalid scorer response")
        return {"status": report["status"], "comparison": report, "exit_capture": paths[0].name}
    except (subprocess.SubprocessError, OSError, ValueError, TypeError, AttributeError):
        return {"status": "unscoreable", "reason": "scorer_refused"}


def cohort(store, tape, as_of, run):
    raw = read(store / "campaign.json", 65536)
    plan = json.loads(raw)
    digest = hashlib.sha256(raw).hexdigest()
    if (plan.get("version") != "alpaca-research-v1" or type(plan.get("slots")) is not int
            or not 1 <= plan["slots"] <= 1000 or type(as_of) is not int or not 0 <= as_of < 2**64
            or type(plan.get("start_unix_ms")) is not int or plan["start_unix_ms"] <= 0
            or type(plan.get("interval_ms")) is not int or plan["interval_ms"] < 120000):
        raise ValueError("Invalid frozen cohort")
    rows = []
    for index in range(plan["slots"]):
        prefix = f"slot-{index:04d}"
        scheduled = plan["start_unix_ms"] + index * plan["interval_ms"]
        row = {"slot": index, "scheduled_unix_ms": scheduled, "comparison": None}
        begin, end = store / (prefix + ".begin.json"), store / (prefix + ".end.json")
        if begin.is_symlink() or end.is_symlink():
            raise ValueError("Unsupported cohort record link")
        if not begin.exists():
            if end.exists():
                raise ValueError("Terminal without claimed attempt")
            row["status"] = "future" if scheduled > as_of else "not_recorded"
        else:
            claim = json.loads(read(begin, 65536))
            if claim.get("campaign_sha256") != digest or claim.get("scheduled_unix_ms") != scheduled:
                raise ValueError("Cohort claim identity mismatch")
            started = claim.get("started_unix_ms")
            if type(started) is not int or started < scheduled:
                raise ValueError("Invalid claim availability")
            if started > as_of:
                row["status"] = "not_available_at_watermark"
                rows.append(row)
                continue
            row["status"] = "interrupted_unknown"
            if end.exists():
                terminal = json.loads(read(end, 65536))
                status = terminal.get("status")
                if status not in ("decision", "failed", "missed"):
                    raise ValueError("Invalid cohort terminal status")
                finished = terminal.get("finished_unix_ms")
                if type(finished) is not int or finished < started:
                    raise ValueError("Invalid terminal availability")
                if finished > as_of:
                    row["status"] = "terminal_not_available_at_watermark"
                    rows.append(row)
                    continue
                row["status"] = status
                if status == "decision":
                    path = store / (prefix + ".decision.json")
                    decision = json.loads(read(path, 2 * 1024 * 1024))
                    proposal = decision.get("proposal", {})
                    if (any(proposal.get(key) != value for key, value in plan["experiment"].items())
                            or decision.get("symbol") != plan["symbol"]
                            or decision.get("assumptions") != plan["assumptions"]):
                        raise ValueError("Decision differs from frozen campaign")
                    horizon = decision.get("outcome_not_before_unix_ms")
                    if type(horizon) is not int or not 0 < horizon < 2**64:
                        raise ValueError("Invalid decision horizon")
                    row.update({"status": "awaiting_horizon", "horizon_unix_ms": horizon})
                    if horizon <= as_of:
                        row.update(score(path, tape, plan["symbol"], horizon, run))
        rows.append(row)
    return {"campaign_sha256": digest, "as_of_unix_ms": as_of, "planned_slots": plan["slots"],
            "status_counts": dict(Counter(row["status"] for row in rows)), "slots": rows,
            "aggregate_profit_microusd": None, "execution_authority": False,
            "environment": "offline_simulation", "decision_origin_verified": False}


def main():
    if len(sys.argv) != 4:
        raise ValueError("usage: report-alpaca-cohort.py <research-store> <capture-tape> <as-of-unix-ms>")
    print(json.dumps(cohort(Path(sys.argv[1]), captures(Path(sys.argv[2])), int(sys.argv[3]),
                            subprocess.run), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        sys.exit("Cohort evidence refused; no performance or trading authority inferred")
