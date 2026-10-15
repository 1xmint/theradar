#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply the exact cancellation replay survivor on GitHub, then restore bytes."""

from pathlib import Path
import shlex
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
source = root / "crates/radar-journal/src/broker.rs"
original = source.read_bytes()
# CI 38305306698 survivor at broker.rs:251:88. Formatting moved this
# expression; its unique complete predicate identifies the exact same operator.
correct = b"progress.broker_order_id == *id && !progress.terminal"
wrong = b"progress.broker_order_id == *id || !progress.terminal"
test = "final_cancellation_record_must_match_nonterminal_checkpoint_identity"
if original.count(correct) != 1:
    sys.exit("Expected exact broker.rs:251:88 cancellation replay predicate")
try:
    source.write_bytes(original.replace(correct, wrong))
    result = subprocess.run(
        shlex.split(sys.argv[1]) + ["test", "--locked", "-p", "radar-journal", test, "--", "--nocapture"],
        cwd=root, capture_output=True, text=True, timeout=120,
    )
    output = result.stdout + result.stderr
    if (result.returncode == 0 or test not in output
            or "test result: FAILED. 0 passed; 1 failed;" not in output):
        print(output)
        sys.exit("Cancellation survivor not caught by executed test")
    print("Caught exact regression: broker.rs:251:88 && to || in cancellation replay")
finally:
    source.write_bytes(original)
