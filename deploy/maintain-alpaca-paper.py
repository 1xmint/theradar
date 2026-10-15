#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""One isolated paper recovery tick; protected config alone selects stop mode."""

import fcntl
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import uuid

READER = "/usr/local/lib/radar-paper-execution/radar"
STORE = Path("/var/lib/radar-paper-execution")
CONFIG = Path("/etc/radar/alpaca-maintenance.json")
MAX_BYTES = 256 * 1024 * 1024
RESERVATION = 4 * 1024 * 1024


def configuration(path, owner=0):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        info = os.fstat(stream.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != owner
                or stat.S_IMODE(info.st_mode) & 0o022):
            raise ValueError("Maintenance configuration is not protected")
        raw = stream.read(65537)
    if len(raw) > 65536:
        raise ValueError("Maintenance configuration exceeds limit")
    value = json.loads(raw)
    if (not isinstance(value, dict)
            or set(value) != {"version", "account_id", "max_pages", "mode"}
            or value["version"] != "alpaca-maintenance-v1"
            or not isinstance(value["account_id"], str)
            or str(uuid.UUID(value["account_id"])) != value["account_id"]
            or type(value["max_pages"]) is not int or not 1 <= value["max_pages"] <= 10
            or value["mode"] not in ("observe", "cancel_working")):
        raise ValueError("Invalid fixed paper maintenance configuration")
    return value


def broker_environment(environment):
    child_env = {"PATH": "/usr/bin:/bin"}
    for name in ("RADAR_ALPACA_PAPER_KEY_ID", "RADAR_ALPACA_PAPER_SECRET_KEY"):
        if name in environment:
            child_env[name] = environment[name]
    return child_env


def tick(directory, config, run, environment, load=configuration, max_bytes=MAX_BYTES, max_files=4096):
    plan = load(config)
    descriptor = os.open(directory / ".lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        used = 0
        count = 0
        for path in directory.iterdir():
            info = path.lstat()
            if not stat.S_ISREG(info.st_mode):
                raise ValueError("Unsupported execution storage entry")
            used += info.st_size
            count += 1
        # Never recreate missing history or silently discard reservations.
        journal = directory / "orders.jsonl"
        if not journal.is_file():
            raise ValueError("Provisioned paper order journal is missing")
        if used + RESERVATION > max_bytes or count > max_files:
            return {"maintenance": "paused", "reason": "storage_limit", "new_order_authority": False}
        argv = [READER, "alpaca-paper-maintain", "--journal", str(journal),
                "--account-id", plan["account_id"], "--max-pages", str(plan["max_pages"])]
        if plan["mode"] == "cancel_working":
            argv.append("--cancel-working")
        result = run(argv, check=True, capture_output=True, text=True, timeout=75,
                     env=broker_environment(environment))
        if len(result.stdout) > 4 * 1024 * 1024:
            raise ValueError("Maintenance response exceeds limit")
        report = json.loads(result.stdout)
        if (not isinstance(report, dict) or type(report.get("version")) is not int
                or report["version"] != 1 or report.get("venue") != "alpaca"
                or report.get("environment") != "paper"
                or report.get("expected_account_id") != plan["account_id"]
                or report.get("checkpoint_refreshed") is not True
                or report.get("new_order_authority") is not False
                or report.get("retry_authorized") is not False
                or report.get("claim_release_authorized") is not False):
            raise ValueError("Invalid maintenance response")
        counts = {}
        for field in ("working_before_cancellation", "unresolved",
                      "cancellation_pending_before_tick", "terminal"):
            clients = report.get(field)
            if not isinstance(clients, list) or any(not isinstance(client, str) for client in clients):
                raise ValueError("Invalid maintenance order classification")
            counts[field] = len(clients)
        cancellation = report.get("cancellation_attempt")
        if ("cancellation_attempt" not in report
                or (cancellation is not None and (plan["mode"] != "cancel_working"
                    or not isinstance(cancellation, dict)
                    or cancellation.get("cancellation") != "request_accepted"))):
            raise ValueError("Invalid maintenance cancellation response")
        return {"maintenance": "observed", "mode": plan["mode"], "orders": counts,
                "cancellation_requested": cancellation is not None,
                "cancellation_confirmed": False, "new_order_authority": False}


def main():
    if sys.argv[1:]:
        raise ValueError("Maintenance worker takes no arguments")
    print(json.dumps(tick(STORE, CONFIG, subprocess.run, os.environ), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Broker stderr, response bodies and credential environment stay private.
        sys.exit("Paper maintenance failed; retained claims require reconciliation")
