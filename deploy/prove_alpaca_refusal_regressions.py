#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply refusal/reconciliation boundary bugs on GitHub, never count compiler failures."""

from pathlib import Path
import shlex
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
source = root / "crates/radar-cli/src/alpaca_submit.rs"
original = source.read_bytes()
cases = [
    (b'.any(|entry| entry.intent.client_order_id == intent.client_order_id)',
     b'.any(|entry| entry.intent.client_order_id == intent.client_order_id && false)',
     "retained_identity_never_gets_a_non_dispatch_receipt_even_when_risk_refuses"),
    (b'return Ok(refused(intent, "risk"));',
     b'return Err("risk refusal outcome unknown".into());',
     "initial_unknowns_and_expired_permissions_never_reserve_or_send"),
    (b'return Ok(refused(intent, "authorization"));',
     b'return Err("authorization refusal outcome unknown".into());',
     "initial_unknowns_and_expired_permissions_never_reserve_or_send"),
]
try:
    for correct, wrong, test in cases:
        if original.count(correct) != 1:
            sys.exit("Expected exact refusal predicate: " + correct.decode())
        source.write_bytes(original.replace(correct, wrong))
        result = subprocess.run(shlex.split(sys.argv[1]) + ["test", "--locked", "-p", "radar-cli",
            "--bin", "radar", test, "--", "--nocapture"], cwd=root,
            capture_output=True, text=True, timeout=120)
        output = result.stdout + result.stderr
        if result.returncode == 0 or "test result: FAILED. 0 passed; 1 failed;" not in output or test not in output:
            print(output)
            sys.exit("Refusal regression not caught by an executed test: " + test)
        print("Caught exact refusal regression: " + correct.decode())
finally:
    source.write_bytes(original)
