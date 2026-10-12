#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Executed-test checks for the unattended maintenance worker on GitHub."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
source = (directory / "maintain-alpaca-paper.py").read_text()
tests = (directory / "test_alpaca_maintenance.py").read_text()
cases = [
    ("info.st_uid != owner", "False"),
    ("stat.S_IMODE(info.st_mode) & 0o022", "False"),
    ('if plan["mode"] == "cancel_working":', 'if plan["mode"] != "cancel_working":'),
    ("if not journal.is_file():", "if False:"),
    ("used + RESERVATION > max_bytes", "used + RESERVATION >= max_bytes"),
    ("count > max_files", "count >= max_files"),
    ('report.get("expected_account_id") != plan["account_id"]', "False"),
    ('report.get("environment") != "paper"', "False"),
    ('report.get("checkpoint_refreshed") is not True', "False"),
    ('report.get("claim_release_authorized") is not False', "False"),
    ('child_env = {"PATH": "/usr/bin:/bin"}', 'child_env = dict(environment)'),
]
for correct, wrong in cases:
    if source.count(correct) != 1:
        sys.exit(f"Maintenance mutation predicate changed: {correct}")
    with tempfile.TemporaryDirectory(prefix="radar-maintenance-mutation-") as temporary:
        target = Path(temporary)
        (target / "maintain-alpaca-paper.py").write_text(source.replace(correct, wrong))
        (target / "test_alpaca_maintenance.py").write_text(tests)
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
                                 "-p", "test_alpaca_maintenance.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran 7 tests" not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Maintenance mutation not caught by executed tests: {correct}")
        print(f"Caught maintenance worker regression: {correct}")
