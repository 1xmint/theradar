#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""One bounded IEX capture per timer invocation. No strategy or order authority."""

import fcntl
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import uuid

STORE = Path("/var/lib/radar-paper-tape")
MAX_BYTES = 256 * 1024 * 1024
MAX_CAPTURE = 1024 * 1024
MAX_FILES = 65536
READER = "/usr/local/lib/radar-paper/radar"


def usage(directory):
    total = 0
    count = 0
    for path in directory.iterdir():
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode):
            raise ValueError("Unsupported recorder storage entry")
        if path.name != ".lock":
            total += info.st_size
            count += 1
    return total, count


def observe(directory, run, max_bytes=MAX_BYTES, max_files=MAX_FILES):
    descriptor = os.open(directory / ".lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        used, count = usage(directory)
        if used + MAX_CAPTURE > max_bytes or count >= max_files:
            return {"recording": "paused", "reason": "storage_limit", "bytes": used,
                    "files": count, "execution_authority": False}
        path = directory / f"capture-{uuid.uuid4().hex}.json"
        result = run([READER, "alpaca-market-read", "--symbols", "SPY,QQQ", "--output", str(path)],
                     check=True, capture_output=True, text=True, timeout=20)
        metadata = json.loads(result.stdout)
        if (not isinstance(metadata, dict) or metadata.get("capture_written") is not True
                or metadata.get("feed") != "iex" or metadata.get("execution_authority") is not False):
            raise ValueError("Invalid capture metadata")
        available = metadata.get("available_at_unix_ms")
        if type(available) is not int or not 0 < available <= 2**64 - 1:
            raise ValueError("Invalid capture availability")
        with path.open("rb") as stream:
            raw = stream.read(MAX_CAPTURE + 1)
        if len(raw) > MAX_CAPTURE:
            raise ValueError("Capture exceeds storage reservation")
        capture = json.loads(raw)
        started = capture.get("read_started_unix_ms")
        if (capture.get("format") != "alpaca-iex-snapshot-v1"
                or capture.get("symbols") != ["SPY", "QQQ"]
                or capture.get("available_at_unix_ms") != available
                or type(started) is not int or not 0 <= started < available):
            raise ValueError("Capture provenance mismatch")
        snapshots = json.loads(capture["raw_response"])
        if not isinstance(snapshots, dict):
            raise ValueError("Invalid snapshot object")
        unknown = []
        quote_unknown = []
        for symbol in ("SPY", "QQQ"):
            snapshot = snapshots.get(symbol)
            if snapshot is None:
                unknown.append(symbol)
                quote_unknown.append(symbol)
            elif not isinstance(snapshot, dict):
                raise ValueError("Invalid symbol snapshot")
            elif snapshot.get("latestQuote") is None:
                quote_unknown.append(symbol)
            elif not isinstance(snapshot["latestQuote"], dict):
                raise ValueError("Invalid quote object")
        # CLI already synced the inode; sync its publication directory too.
        sync_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(sync_fd)
        finally:
            os.close(sync_fd)
        return {"recording": "captured", "capture": path.name, "feed": "iex",
                "consolidated_market": False, "available_at_unix_ms": available,
                "unknown_symbols": unknown, "unknown_quote_symbols": quote_unknown,
                "exchange_times_validated": False, "execution_authority": False}


def main():
    if sys.argv[1:]:
        raise ValueError("Recorder accepts no arguments")
    print(json.dumps(observe(STORE, subprocess.run), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Never print vendor bodies, credential environment or subprocess stderr.
        sys.exit("IEX recording failed; existing captures retained, trading disabled")
