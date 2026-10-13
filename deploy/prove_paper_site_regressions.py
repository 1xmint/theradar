#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Actual router/socket regressions; compiler failures never count as coverage."""

from pathlib import Path
import re
import shlex
import subprocess
import sys

cargo = sys.argv[1] if len(sys.argv) == 2 else "cargo"
root = Path(__file__).resolve().parent.parent
cases = [
    ("lib.rs", "request.extensions_mut().insert(Operator(identity));",
     "request.extensions_mut().insert(identity);", 1),
    ("paper.rs", ".map(|Extension(Operator(identity))| identity)",
     '.map(|Extension(Operator(identity))| identity).or_else(|| Some(crate::access::Identity { email:"forged".into(), subject:"forged".into() }))', 1),
    ("paper.rs", 'Some("same-origin")', 'Some("cross-site")', 1),
    ("paper.rs", 'Some("1")', 'Some("0")', 1),
    ("paper.rs", "generation.checked_add(1)", "generation.checked_add(0)", 1),
    ("paper.rs", 'report.environment != "paper"', "false", 2),
    ("paper.rs", "|| report.execution_authority", "|| false", 2),
    ("paper.rs", "p.enabled != report.handover_enabled", "p.enabled == report.handover_enabled", 1),
    ("paper.rs", ' !raw.ends_with(b"\\n")', " false", 1),
    ("paper.rs", "raw.len() as u64 > LIMIT", "false", 1),
]

for filename, correct, wrong, count in cases:
    path = root / "crates/radar-serve/src" / filename
    source = path.read_text()
    if source.count(correct) != count:
        sys.exit(f"Paper website regression predicate changed: {filename}: {correct}")
    try:
        path.write_text(source.replace(correct, wrong))
        result = subprocess.run(shlex.split(cargo) + ["test", "--locked", "-p", "radar-serve", "--lib", "paper::tests::"],
            cwd=root, capture_output=True, text=True, timeout=180)
        output = result.stdout + result.stderr
        executed = re.search(r"test result: FAILED\. (\d+) passed; (\d+) failed;", output)
        if (result.returncode == 0 or executed is None or sum(map(int, executed.groups())) != 7
                or int(executed[2]) == 0 or "could not compile" in output):
            print(output)
            sys.exit(f"Paper website regression not caught by executed tests: {filename}: {correct}")
        print(f"Caught paper website regression: {filename}: {correct}")
    finally:
        path.write_text(source)
