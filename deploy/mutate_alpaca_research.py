#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply targeted schedule, persistence and isolation bugs on GitHub."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
source = (directory / "run-alpaca-research.py").read_text()
tests = (directory / "test_alpaca_research.py").read_text()
mutations = [
    ('type(value[name]) is not int', 'False'),
    ('scheduled > at', 'scheduled >= at'),
    ('at < scheduled + plan["start_window_ms"]', 'at <= scheduled + plan["start_window_ms"]'),
    ('if begin.exists():', 'if False:'),
    ('read(frozen, 65536) != raw', 'read(frozen, 65536) == raw'),
    ('used + RESERVATION > max_bytes', 'used + RESERVATION >= max_bytes'),
    ('if not stat.S_ISREG(info.st_mode):', 'if False:'),
    ('os.fsync(stream.fileno())', 'pass'),
    ('sync(directory)\n\n\ndef campaign', 'pass\n\n\ndef campaign'),
    ('env=model_env', 'env=market_env'),
    ('read(capture, 1024 * 1024)', 'pass'),
    ('report.get("execution_authority") is not False', 'False'),
]
for old, new in mutations:
    if old not in source:
        sys.exit(f"Missing mutation site: {old}")
    with tempfile.TemporaryDirectory(prefix="radar-research-mutation-") as temporary:
        target = Path(temporary)
        (target / "run-alpaca-research.py").write_text(source.replace(old, new))
        (target / "test_alpaca_research.py").write_text(tests)
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
                                 "-p", "test_alpaca_research.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran 9 tests" not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Mutation not caught by executed tests: {old} -> {new}")
        print(f"Caught research mutation: {old} -> {new}")
