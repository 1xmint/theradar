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
    ("paper.rs", "|| report.evidence.execution_authority", "|| false", 1),
    ("paper.rs", "|| report.effects.execution_authority", "|| false", 1),
    ("paper.rs", "p.enabled != report.handover_enabled", "p.enabled == report.handover_enabled", 1),
    ("paper.rs", 'if !raw.ends_with(b"\\n")', "if false", 1),
    ("paper.rs", "stream.take(LIMIT)", "stream.take(LIMIT * 2)", 1),
    # Original shard 3 survivors at paper.rs:217:22 (both request comparisons),
    # :220:51 (shrunk read bound), and :221:29 (inclusive response refusal).
    ("paper.rs", "raw.len() > 65536", "raw.len() == 65536", 1),
    ("paper.rs", "raw.len() > 65536", "raw.len() >= 65536", 1),
    ("paper.rs", "stream.take(LIMIT)", "stream.take(LIMIT - 1)", 1),
    ("paper.rs", 'if !raw.ends_with(b"\\n")', 'if raw.len() as u64 >= LIMIT || !raw.ends_with(b"\\n")', 1),
    # Exact survivor locations at original 9391352 paper.rs:14:22 and :14:29.
    ("paper.rs", "4 * 1024 * 1024", "4 + 1024 * 1024", 1),
    ("paper.rs", "4 * 1024 * 1024", "4 * 1024 + 1024", 1),
    # Exact status survivors at original 9391352 paper.rs:153:37 and :153:59.
    ("paper.rs", "report.handover_enabled && (", "report.handover_enabled || (", 1),
    ("paper.rs", "report.generation == 0 || report.preferences.is_none()",
     "report.generation != 0 || report.preferences.is_none()", 1),
    # Keep the route constructor used so this observes missing app wiring rather
    # than counting a dead-code compiler refusal as a routing regression.
    ("lib.rs", ".merge(paper::routes())", ".merge({ let _ = paper::routes(); Router::new() })", 1),
]

for filename, correct, wrong, count in cases:
    path = root / "crates/radar-serve/src" / filename
    source = path.read_text()
    if source.count(correct) != count:
        sys.exit(f"Paper website regression predicate changed: {filename}: {correct}")
    try:
        path.write_text(source.replace(correct, wrong))
        target = (["--test", "access_guard", "switching_access_off_serves_normally"]
            if correct == ".merge(paper::routes())" else ["--lib", "paper::tests::"])
        required = 1 if correct == ".merge(paper::routes())" else 7
        result = subprocess.run(shlex.split(cargo) + ["test", "--locked", "-p", "radar-serve"] + target,
            cwd=root, capture_output=True, text=True, timeout=180)
        output = result.stdout + result.stderr
        executed = re.search(r"test result: FAILED\. (\d+) passed; (\d+) failed;", output)
        if (result.returncode == 0 or executed is None or sum(map(int, executed.groups())) != required
                or int(executed[2]) == 0 or "could not compile" in output):
            print(output)
            sys.exit(f"Paper website regression not caught by executed tests: {filename}: {correct}")
        print(f"Caught paper website regression: {filename}: {correct}")
    finally:
        path.write_text(source)
