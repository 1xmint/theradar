#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply opening-history mistakes on GitHub; compiler failures are not kills."""

from pathlib import Path
import re
import shlex
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
cash = root / "crates/radar-cli/src/alpaca_cash.rs"
journal = root / "crates/radar-journal/src/broker.rs"
originals = {path:path.read_bytes() for path in (cash, journal)}
unit = ["--bin", "radar", "alpaca_cash::tests::opening_"]
cases = [
    (cash, b"cash(&current.account)? != initial", b"cash(&current.account)? != initial.saturating_add(amount)",
     "double-count acknowledged opening credit", None, unit, 4),
    (cash, b"event.id != opening.activity_id", b"false", "forget selected opening event", None, unit, 4),
    (cash, b"log.entries().map_err(|e| e.to_string())?.next().is_some()", b"false",
     "acknowledge after retained orders", (b"fn opening_plan(", b"fn opening_delta("), unit, 4),
    (journal, b".next()", b".last()", "replace original opening with latest checkpoint",
     (b"pub fn opening_cash(", b"pub fn claims("), unit, 4),
    (cash, b'observation["opening_cash_acknowledgement"] = acknowledgement.clone();', b"let _ = acknowledgement;",
     "omit durable opening acknowledgement", None, unit, 4),
    (cash, b'"--already-in-opening-balance"', b"_", "accept another acknowledgement flag", None,
     ["--test", "alpaca_opening_process"], 1),
]

try:
    for path, correct, wrong, name, scope, target, count in cases:
        original = originals[path]
        begin, end = (0, len(original)) if scope is None else (original.index(scope[0]), original.index(scope[1]))
        section = original[begin:end]
        if section.count(correct) != 1:
            sys.exit(f"Opening regression predicate changed: {name}")
        path.write_bytes(original[:begin] + section.replace(correct, wrong) + original[end:])
        result = subprocess.run(shlex.split(sys.argv[1]) + ["test", "--locked", "-p", "radar-cli"]
            + target + ["--", "--nocapture"], cwd=root, capture_output=True, text=True, timeout=120)
        output = result.stdout + result.stderr
        executed = re.search(r"test result: FAILED\. (\d+) passed; (\d+) failed;", output)
        if (result.returncode == 0 or executed is None or sum(map(int, executed.groups())) != count
                or int(executed[2]) == 0):
            print(output)
            sys.exit(f"Opening regression not caught by {count} executed tests: {name}")
        print(f"Caught opening regression: {name}")
        path.write_bytes(original)
finally:
    for path, original in originals.items():
        path.write_bytes(original)
