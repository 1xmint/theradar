#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply each exact clock-parser survivor on GitHub; restore source bytes."""

from pathlib import Path
import shlex
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
source = root / "crates/radar-cli/src/alpaca_session.rs"
original = source.read_bytes()
test = "zoned_clock_preserves_nanoseconds_and_rejects_malformed_times"
# CI 38307389254 original positions; complete unique predicates disambiguate
# the two different > operators even after formatting moves their lines.
cases = [
    ("10:25", b"!text.is_ascii() || text.len() < 25", b"!text.is_ascii() && text.len() < 25"),
    ("22:14", b"hours > 23 || minutes > 59", b"hours >= 23 || minutes > 59"),
    ("22:30", b"hours > 23 || minutes > 59", b"hours > 23 || minutes >= 59"),
]
try:
    for location, correct, wrong in cases:
        if original.count(correct) != 1:
            sys.exit("Expected exact alpaca_session.rs:" + location + " predicate")
        source.write_bytes(original.replace(correct, wrong))
        result = subprocess.run(
            shlex.split(sys.argv[1]) + ["test", "--locked", "-p", "radar-cli", test, "--", "--nocapture"],
            cwd=root, capture_output=True, text=True, timeout=120,
        )
        output = result.stdout + result.stderr
        if (result.returncode == 0 or test not in output
                or "test result: FAILED. 0 passed; 1 failed;" not in output):
            print(output)
            sys.exit("Clock survivor not caught by executed test: " + location)
        print("Caught exact regression: alpaca_session.rs:" + location)
finally:
    source.write_bytes(original)
