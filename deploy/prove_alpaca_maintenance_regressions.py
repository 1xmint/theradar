#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Exercise recovery-ordering mistakes on GitHub and always restore source."""

from pathlib import Path
import re
import shlex
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
source = root / "crates/radar-cli/src/alpaca_maintain.rs"
original = source.read_bytes()
cases = [
    (b"refresh(log)?;", b"let _ = &refresh;", "skip protected checkpoint refresh"),
    (b"if progress.terminal {", b"if !progress.terminal {", "invert terminal classification"),
    (b"entry.cancel_unknown_order_id.is_some()", b"entry.cancel_unknown_order_id.is_none()",
     "invert pending cancellation classification"),
    (b".first()", b".last()", "select last working order instead of first"),
]

try:
    for correct, wrong, name in cases:
        if original.count(correct) != 1:
            sys.exit(f"Maintenance regression source predicate changed: {name}")
        source.write_bytes(original.replace(correct, wrong))
        result = subprocess.run(
            shlex.split(sys.argv[1]) + ["test", "--locked", "-p", "radar-cli", "--bin", "radar",
                                       "alpaca_maintain::tests::", "--", "--nocapture"],
            cwd=root, capture_output=True, text=True, timeout=120,
        )
        output = result.stdout + result.stderr
        executed = re.search(r"test result: FAILED\. (\d+) passed; (\d+) failed;", output)
        if (result.returncode == 0 or executed is None
                or sum(map(int, executed.groups())) != 4 or int(executed[2]) == 0):
            print(output)
            sys.exit(f"Maintenance regression not caught by four executed tests: {name}")
        print(f"Caught maintenance regression: {name}")
finally:
    source.write_bytes(original)
