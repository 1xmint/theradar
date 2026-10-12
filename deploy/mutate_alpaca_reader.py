#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Targeted reader boundary mutations, run on GitHub with fake provider/file tests."""

from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    directory = Path(__file__).parent
    source = (directory / "read-alpaca-paper.py").read_text()
    mutations = [
        ("hashlib.sha256(raw).hexdigest() != expected", "hashlib.sha256(raw).hexdigest() == expected"),
        ("info.st_uid != 0", "info.st_uid == 0"),
        ("info.st_mode & 0o022", "info.st_mode & 0o000"),
        ("if target.is_symlink():", "if False:"),
        ("stat.S_IMODE(info.st_mode) != 0o600", "stat.S_IMODE(info.st_mode) == 0o600"),
        ("not stat.S_ISREG(info.st_mode)", "stat.S_ISREG(info.st_mode)"),
        ('"DynamicUser=yes"', '"DynamicUser=no"'),
        ('"alpaca-read"]', '"alpaca-paper-submit"]'),
    ]
    for old, new in mutations:
        if old not in source:
            sys.exit(f"Mutation target missing: {old}")
        with tempfile.TemporaryDirectory(prefix="radar-reader-mutation-") as temporary:
            target = Path(temporary)
            (target / "read-alpaca-paper.py").write_text(source.replace(old, new))
            (target / "test_alpaca_reader.py").write_text((directory / "test_alpaca_reader.py").read_text())
            result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
                                     "-p", "test_alpaca_reader.py"], capture_output=True, text=True)
            # A compiler/import failure is not a test kill.
            if (result.returncode == 0 or "Ran 4 tests" not in result.stderr
                    or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                    or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
                print(result.stdout)
                print(result.stderr)
                sys.exit(f"Mutation not caught by executed tests: {old} -> {new}")
            print(f"Caught: {old} -> {new}")


if __name__ == "__main__":
    main()
