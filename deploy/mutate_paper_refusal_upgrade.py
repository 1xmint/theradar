#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Executed coordinated-upgrade regressions; compiler/import errors never count."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
source = (directory / "upgrade-paper-refusals.py").read_text()
cases = [
    ('if os.geteuid() != 0:', 'if False:'),
    ('digest(raw) != expected', 'False'),
    ('digest(reader) != READER_HASH', 'False'),
    ('member.name in result', 'False'),
    ('not member.isfile()', 'False'),
    ('set(result) != {"paper-control.py", "paper-supervisor.py"}', 'False'),
    ('digest(before[name]) != expected_old', 'False'),
    ('info.st_mode & 0o022 or digest(before[name])', 'False or digest(before[name])'),
    ('if backup.exists() or backup.is_symlink():', 'if False:'),
    ('if (state / "control.json").exists() or (state / "control.json").is_symlink() or list(state.glob("effective-policy-*.json")):', 'if False:'),
    ('self.run(["/usr/bin/systemctl", "show", service, "--property=ActiveState", "--value"]).stdout.strip() != "inactive"', 'False'),
    ('not read(state / "orders.jsonl").startswith(journal)', 'False'),
    ('any(read(state / name, 65536) != value for name, value in markers.items())', 'any(read(state / name, 65536) != value and False for name, value in markers.items())'),
    ('os.link(temporary, path)', 'os.replace(temporary, path)'),
]
for index, (correct, wrong) in enumerate(cases, 1):
    if source.count(correct) != 1:
        sys.exit("Upgrade mutation predicate changed: " + correct)
    with tempfile.TemporaryDirectory(prefix="radar-upgrade-mutation-") as temporary:
        root = Path(temporary)
        (root / "upgrade-paper-refusals.py").write_text(source.replace(correct, wrong))
        (root / "test_paper_refusal_upgrade.py").write_text((directory / "test_paper_refusal_upgrade.py").read_text())
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
            "-p", "test_paper_refusal_upgrade.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran 8 tests" not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or any(marker in result.stderr for marker in ("SyntaxError", "ImportError", "ModuleNotFoundError"))):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Upgrade regression {index} not caught by executed tests")
        print(f"Caught paper upgrade regression {index}: {correct}")
