#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply private-installation bugs; syntax/import refusals never count."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
cases = [
    ("setup-paper-autonomy.py", 'if os.geteuid() != 0:', 'if False:'),
    ("setup-paper-autonomy.py", 'digest(raw) != expected', 'False'),
    ("setup-paper-autonomy.py", 'digest(reader) != READER_HASH', 'False'),
    ("setup-paper-autonomy.py", 'entry.name in result', 'False'),
    ("setup-paper-autonomy.py", 'if set(result) != set(FILES):', 'if False:'),
    ("setup-paper-autonomy.py", 'entry.isfile()', 'True'),
    ("setup-paper-autonomy.py", 'digest(read(path)) != expected', 'False'),
    ("setup-paper-autonomy.py", 'stat.S_IMODE(info.st_mode) != 0o600', 'False'),
    ("setup-paper-autonomy.py", 'if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:', 'if False:'),
    ("setup-paper-autonomy.py", 'if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022 or digest(read(path)) != expected:', 'if digest(read(path)) != expected:'),
    ("setup-paper-autonomy.py", 'os.link(temporary, path)', 'os.replace(temporary, path)'),
    ("setup-paper-autonomy.py", 'json.loads(markers["genesis-complete.json"]) != {"genesis":"initialized", "account_id":ACCOUNT, "new_order_authority":False}', 'False'),
    ("setup-paper-autonomy.py", 'if (state / "control.json").exists() or list(state.glob("effective-policy-*.json")):', 'if False:'),
    ("setup-paper-autonomy.py", 'self.command("/usr/bin/systemctl", "disable", "--now", name + ".timer")', 'pass'),
    ("setup-paper-autonomy.py", '.stdout.strip() != "inactive"', '.stdout.strip() == "never"'),
    ("setup-paper-autonomy.py", '.stdout.strip() not in ("0", "1")', '.stdout.strip() == "never"'),
    ("setup-paper-autonomy.py", 'not after.startswith(journal)', 'False'),
    ("setup-paper-autonomy.py", 'any(read(state / name) != value for name, value in markers.items())', 'False'),
    ("setup-paper-autonomy.py", 'self.command("/usr/bin/systemctl", "start", "radar-paper-supervisor.service")', 'pass'),
    ("provision-paper-supervisor.py", 'if any(store.iterdir()):', 'if False:'),
    ("radar-paper-autonomy-admin.py", 'sys.argv[1] not in ACTIONS', 'False'),
    ("radar-paper-autonomy-admin.py", 'os.geteuid() != 0', 'False'),
    ("radar-paper-autonomy-admin.py", 'env={"PATH":"/usr/bin:/bin"}', 'env=dict(os.environ)'),
]
files = ("setup-paper-autonomy.py", "provision-paper-supervisor.py", "radar-paper-autonomy-admin.py", "paper-control.py", "test_paper_install.py")
for index, (filename, correct, wrong) in enumerate(cases, 1):
    source = (directory / filename).read_text()
    expected = 2 if correct.startswith('if (state / "control.json")') else 1
    if source.count(correct) != expected:
        sys.exit(f"Private installation mutation predicate changed: {correct}")
    with tempfile.TemporaryDirectory(prefix="radar-paper-install-mutation-") as temporary:
        root = Path(temporary)
        for name in files: (root / name).write_text((directory / name).read_text())
        (root / filename).write_text(source.replace(correct, wrong))
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
            "-p", "test_paper_install.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran 9 tests" not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or any(marker in result.stderr for marker in ("SyntaxError", "ImportError", "ModuleNotFoundError"))):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Private installation regression {index} not caught by executed tests")
        print(f"Caught private installation regression {index}: {filename}: {correct}")
