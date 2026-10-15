#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Apply one reviewed paper opening acknowledgement; preserve history and reader."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import types

ADMIN = Path("/usr/local/sbin/radar-paper-admin")
ADMIN_HASH = "e740c7a992f7e7ac91f843c6b8a1a3fc25a4fac41e0033e9f91b62071d75e1bf"
READER_HASH = "3f21fc1ae3b54b2333a5b809eda916c296be0948d69dba1f9da618adec9733d2"
SOURCE = Path("/tmp/radar-opening-cash.new")
TARGET = Path("/usr/local/lib/radar-paper-execution/radar-opening-acknowledge")
STATE = Path("/var/lib/private/radar-paper-execution")
ACTIVITY = "20261012000000000::dde9ccf3-cf7e-4926-aa97-6f522edcc4ff"
AMOUNT = "100000000000000"


def read(path, limit):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("Expected regular acknowledgement input")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Acknowledgement input exceeds bound")
    return raw


def admin_module():
    info = ADMIN.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
        raise ValueError("Expected protected installed administration")
    raw = read(ADMIN, 65536)
    if hashlib.sha256(raw).hexdigest() != ADMIN_HASH:
        raise ValueError("Installed administrative source differs")
    module = types.ModuleType("verified_paper_administration")
    # Execute only the hash-pinned installed bytes, never a caller-supplied module.
    exec(compile(raw, str(ADMIN), "exec"), module.__dict__)
    module.protected(ADMIN)
    return module


def prefix(path, length=None):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        info = os.fstat(stream.fileno())
        if (not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= 256 * 1024 * 1024
                or (length is not None and not 0 < length <= info.st_size)):
            raise ValueError("Expected bounded retained history")
        remaining = info.st_size if length is None else length
        total = remaining
        digest = hashlib.sha256()
        while remaining:
            raw = stream.read(min(remaining, 1024 * 1024))
            if not raw:
                raise ValueError("Retained history changed while reading")
            digest.update(raw)
            remaining -= len(raw)
    return total, digest.hexdigest()


def publish(path, raw):
    descriptor, temporary = tempfile.mkstemp(prefix=".opening-reader-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fchmod(stream.fileno(), 0o755)
            os.fsync(stream.fileno())
        os.link(temporary, path)
        descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    finally:
        os.unlink(temporary)


def verify(report, account):
    if (not isinstance(report, dict) or type(report.get("version")) is not int or report["version"] != 1
            or report.get("venue") != "alpaca" or report.get("environment") != "paper"
            or report.get("expected_account_id") != account or report.get("checkpoint_written") is not True
            or any(report.get(field) is not False for field in
                   ("execution_authority", "retry_authorized", "claim_release_authorized", "profit_measured"))):
        raise ValueError("Invalid opening acknowledgement report")
    checkpoint = report.get("checkpoint")
    claims = report.get("claims")
    if (not isinstance(checkpoint, dict) or checkpoint.get("account_id") != account
            or checkpoint.get("orders") != {} or not isinstance(claims, dict) or claims.get("claims") != []):
        raise ValueError("Opening acknowledgement contains unexpected order claims")
    observation = checkpoint.get("observation")
    acknowledgement = observation.get("opening_cash_acknowledgement") if isinstance(observation, dict) else None
    if (not isinstance(acknowledgement, dict) or acknowledgement.get("activity_id") != ACTIVITY
            or acknowledgement.get("amount_nanos") != AMOUNT
            or acknowledgement.get("operator_acknowledged_already_in_opening_cash") is not True
            or acknowledgement.get("broker_posting_time_verified") is not False):
        raise ValueError("Opening acknowledgement differs from reviewed event")


def apply(expected, load_admin=admin_module, run=subprocess.run):
    if os.geteuid() != 0 or not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("Use sudo with the verified acknowledgement reader SHA256")
    admin = load_admin()
    plan = admin.plan()
    admin.protected(admin.READER)
    if hashlib.sha256(read(admin.READER, 32 * 1024 * 1024)).hexdigest() != READER_HASH:
        raise ValueError("Existing maintenance reader differs")
    raw = read(SOURCE, 32 * 1024 * 1024)
    if hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("Acknowledgement reader hash differs")
    admin.protected(TARGET.parent / "maintain-alpaca-paper.py")
    for name in (admin.SERVICE, admin.TIMER):
        admin.protected(Path("/etc/systemd/system") / name)
    if TARGET.exists() or TARGET.is_symlink():
        raise ValueError("Acknowledgement previously staged; inspect before recovery")
    admin.call(["/usr/bin/systemctl", "stop", admin.TIMER])
    descriptor = os.open(STATE / ".lock", os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as lock:
        if not stat.S_ISREG(os.fstat(lock.fileno()).st_mode):
            raise ValueError("Expected existing observation lock")
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        before = prefix(STATE / "orders.jsonl")
        publish(TARGET, raw)
        argv = admin.diagnostic("maintain", plan["max_pages"])
        index = argv.index(str(admin.READER))
        argv = ["--unit=radar-paper-opening-ack" if value.startswith("--unit=") else value for value in argv[:index]]
        argv += [str(TARGET), "alpaca-opening-cash-acknowledge", "--journal", "/var/lib/radar-paper-execution/orders.jsonl",
                 "--account-id", admin.ACCOUNT, "--activity-id", ACTIVITY, "--max-pages", str(plan["max_pages"]),
                 "--already-in-opening-balance"]
        result = run(argv, check=True, capture_output=True, text=True, timeout=100, env={"PATH":"/usr/bin:/bin"})
        if len(result.stdout) > 4 * 1024 * 1024:
            raise ValueError("Opening acknowledgement response exceeds bound")
        verify(json.loads(result.stdout), admin.ACCOUNT)
        if (prefix(STATE / "orders.jsonl", before[0]) != before
                or prefix(STATE / "orders.jsonl")[0] <= before[0]):
            raise ValueError("Original journal prefix differs; keep scheduling stopped")
    # Release the observation lock before probing the unchanged recurring worker.
    admin.call(["/usr/bin/systemctl", "start", admin.SERVICE])
    admin.call(["/usr/bin/systemctl", "enable", "--now", admin.TIMER])
    print(json.dumps({"opening_acknowledged":True, "cash_usd":"100000", "original_prefix_preserved":True,
                      "maintenance_started":True, "new_order_authority":False}, sort_keys=True))


if __name__ == "__main__":
    try:
        if len(sys.argv) != 2:
            raise ValueError("One verified reader hash required")
        apply(sys.argv[1])
    except Exception:
        sys.exit("Opening application failed; retain history and staged reader, inspect before any retry")
