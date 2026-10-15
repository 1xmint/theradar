#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply exact regressions in CI and require executed focused tests to fail."""

from pathlib import Path
import shlex
import subprocess
import sys

root = Path(__file__).resolve().parent.parent


def prove(path, correct, wrong, package, test, label):
    source = root / path
    original = source.read_bytes()
    if original.count(correct) != 1:
        raise SystemExit(f"Expected exactly one regression site: {label}")
    try:
        source.write_bytes(original.replace(correct, wrong))
        command = shlex.split(sys.argv[1]) + [
            "test", "--locked", "-p", package, test, "--", "--nocapture",
        ]
        result = subprocess.run(
            command, cwd=root, capture_output=True, text=True, timeout=120,
        )
        output = result.stdout + result.stderr
        if (result.returncode == 0
                or "test result: FAILED. 0 passed; 1 failed;" not in output
                or test not in output):
            print(output)
            raise SystemExit(f"Regression not caught by executed test: {label}")
        print(f"Caught exact regression: {label}")
    finally:
        source.write_bytes(original)


prove(
    "crates/radar-model/src/codex.rs",
    b'let prompt = format!("{}\\n\\n{}", request.system(), request.render());',
    b"let prompt = request.render();",
    "radar-model", "trusted_framing_reaches_cli_stdin",
    "omitted Request::system from CLI stdin",
)
# Initial CI survivor: alpaca_propose.rs:89:49; formatting moved it to line 105.
prove(
    "crates/radar-cli/src/alpaca_propose.rs",
    b"if experiment.strategy_id.trim().is_empty() || experiment.horizon_ms == 0 {",
    b"if experiment.strategy_id.trim().is_empty() && experiment.horizon_ms == 0 {",
    "radar-cli",
    "subscription_command_receives_instructions_and_only_records_valid_research",
    "alpaca_propose.rs:89:49 || to && before model call",
)
