#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply exact historical-context survivors on GitHub, restoring source bytes."""

from pathlib import Path
import shlex
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
source = root / "crates/radar-cli/src/alpaca_context.rs"
original = source.read_bytes()
test = "ohlc_edges_accept_equality_and_reject_each_outside_price"
# CI 38309254603: select the complete unique predicate, including the final
# comparison, so the earlier high < open comparison cannot be substituted.
cases = [
    ("48:12 ==", b"low > open || low > close", b"low == open || low > close"),
    ("48:12 >=", b"low > open || low > close", b"low >= open || low > close"),
    ("48:19", b"low > open || low > close", b"low > open && low > close"),
    ("48:26 ==", b"low > open || low > close", b"low > open || low == close"),
    ("48:26 >=", b"low > open || low > close", b"low > open || low >= close"),
    ("48:34", b"low > close || high < open", b"low > close && high < open"),
    ("48:42 ==", b"high < open || high < close", b"high == open || high < close"),
    ("48:42 <=", b"high < open || high < close", b"high <= open || high < close"),
    ("48:57", b"high < open || high < close", b"high < open || high <= close"),
]
try:
    for location, correct, wrong in cases:
        if original.count(correct) != 1:
            sys.exit("Expected exact alpaca_context.rs:" + location + " predicate")
        source.write_bytes(original.replace(correct, wrong))
        result = subprocess.run(
            shlex.split(sys.argv[1]) + ["test", "--locked", "-p", "radar-cli", test, "--", "--nocapture"],
            cwd=root, capture_output=True, text=True, timeout=120,
        )
        output = result.stdout + result.stderr
        if (result.returncode == 0 or test not in output
                or "test result: FAILED. 0 passed; 1 failed;" not in output):
            print(output)
            sys.exit("Context survivor not caught by executed test: " + location)
        print("Caught exact regression: alpaca_context.rs:" + location)
finally:
    source.write_bytes(original)
