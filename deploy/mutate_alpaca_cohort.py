#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Exact cohort watermark and denominator regressions for GitHub."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
source = (directory / "report-alpaca-cohort.py").read_text()
tests = (directory / "test_alpaca_cohort.py").read_text()
for old, new in [
    ('at <= horizon', 'at < horizon'),
    ('at <= horizon', 'True'),
    ('horizon <= as_of', 'horizon < as_of'),
    ('started > as_of', 'False'),
    ('finished > as_of', 'False'),
    ('len(paths) != 1', 'False'),
    ('range(plan["slots"])', 'range(plan["slots"] - 1)'),
    ('claim.get("campaign_sha256") != digest', 'False'),
    ('proposal.get(key) != value', 'False'),
]:
    if source.count(old) != 1:
        sys.exit(f"Expected exact cohort mutation: {old}")
    with tempfile.TemporaryDirectory(prefix="radar-cohort-mutation-") as temporary:
        target = Path(temporary)
        (target / "report-alpaca-cohort.py").write_text(source.replace(old, new))
        (target / "test_alpaca_cohort.py").write_text(tests)
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
                                 "-p", "test_alpaca_cohort.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran 6 tests" not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Cohort mutation not caught by executed tests: {old}")
        print(f"Caught cohort mutation: {old} -> {new}")
