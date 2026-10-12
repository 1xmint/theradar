#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Targeted quota/provenance/install boundary mutations for GitHub."""

from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    directory = Path(__file__).parent
    sources = {name: (directory / name).read_text() for name in (
        "record-alpaca-market.py", "setup-alpaca-recorder.py", "test_alpaca_recorder.py")}
    mutations = {
        "record-alpaca-market.py": [
            ("used + MAX_CAPTURE > max_bytes", "used + MAX_CAPTURE >= max_bytes"),
            ("count >= max_files", "count > max_files"),
            ("not stat.S_ISREG(info.st_mode)", "stat.S_ISREG(info.st_mode)"),
            ('metadata.get("feed") != "iex"', 'metadata.get("feed") == "iex"'),
            ('metadata.get("capture_written") is not True', 'metadata.get("capture_written") is True'),
            ("type(available) is not int", "False"),
            ('capture.get("available_at_unix_ms") != available', 'capture.get("available_at_unix_ms") == available'),
            ("not 0 <= started < available", "not 0 <= started <= available"),
            ("quote_unknown.append(symbol)", "pass"),
        ],
        "setup-alpaca-recorder.py": [
            ("hashlib.sha256(raw).hexdigest() != expected", "hashlib.sha256(raw).hexdigest() == expected"),
            ("set(result) != set(FILES)", "set(result) == set(FILES)"),
            ("info.st_uid != 0", "info.st_uid == 0"),
            ("stat.S_IMODE(info.st_mode) != 0o600", "stat.S_IMODE(info.st_mode) == 0o600"),
            ('"start", "radar-paper-market.service"', '"enable", "radar-paper-market.timer"'),
            ("hashlib.sha256(reader.read_bytes()).hexdigest() != READER_SHA256",
             "hashlib.sha256(reader.read_bytes()).hexdigest() == READER_SHA256"),
        ],
    }
    for filename, variants in mutations.items():
        for old, new in variants:
            if old not in sources[filename]:
                sys.exit(f"Mutation target missing: {filename}: {old}")
            with tempfile.TemporaryDirectory(prefix="radar-recorder-mutation-") as temporary:
                target = Path(temporary)
                for name, source in sources.items():
                    (target / name).write_text(source.replace(old, new) if name == filename else source)
                result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
                                         "-p", "test_alpaca_recorder.py"], capture_output=True, text=True)
                if (result.returncode == 0 or "Ran 7 tests" not in result.stderr
                        or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                        or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
                    print(result.stdout)
                    print(result.stderr)
                    sys.exit(f"Mutation not caught by executed tests: {filename}: {old} -> {new}")
                print(f"Caught: {filename}: {old} -> {new}")


if __name__ == "__main__":
    main()
