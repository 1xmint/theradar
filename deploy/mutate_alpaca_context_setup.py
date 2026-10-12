#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Exact installer boundary regressions, exercised only on GitHub."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
source = (directory / "setup-alpaca-context.py").read_text()
tests = (directory / "test_alpaca_context_setup.py").read_text()
for old, new in [
    ('now + 120000 < start', 'now + 120000 <= start'),
    ('start <= now + 7 * 86400000', 'start < now + 7 * 86400000'),
    ('type(start) is not int', 'False'),
    ('type(plan.get("interval_ms")) is not int', 'False'),
    ('experiment.get("strategy_id") != "snapshot-context-v1"', 'False'),
    ('experiment.get("horizon_ms") != 600000', 'False'),
    ('hashlib.sha256(raw).hexdigest() != expected', 'hashlib.sha256(raw).hexdigest() == expected'),
    ('set(result) != set(FILES)', 'set(result) == set(FILES)'),
    ('hashlib.sha256(reader).hexdigest() != READER_SHA256', 'hashlib.sha256(reader).hexdigest() == READER_SHA256'),
    ('stat.S_IMODE(info.st_mode) != 0o600', 'stat.S_IMODE(info.st_mode) == 0o600'),
    ('info.st_uid != 0 or info.st_mode & 0o022', 'False'),
    ('os.geteuid() != 0', 'os.geteuid() == 0'),
    ('if target.exists() or target.is_symlink():', 'if False:'),
    ('"start", "radar-context-research.service"', '"enable", "radar-context-research.timer"'),
]:
    if source.count(old) != 1:
        sys.exit(f"Expected unique installer mutation: {old}")
    with tempfile.TemporaryDirectory(prefix="radar-research-setup-mutation-") as temporary:
        target = Path(temporary)
        (target / "setup-alpaca-context.py").write_text(source.replace(old, new))
        (target / "test_alpaca_context_setup.py").write_text(tests)
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
                                 "-p", "test_alpaca_context_setup.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran 8 tests" not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Installer mutation not caught by executed tests: {old}")
        print(f"Caught research setup mutation: {old} -> {new}")
