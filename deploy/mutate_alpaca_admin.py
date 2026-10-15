#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Executed-test reapplications for the narrowly granted root helper."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
groups = [
    ("radar-paper-admin.py", "test_alpaca_admin.py", [
        ("os.geteuid() != 0", "False"),
        ("len(sys.argv) != 2", "False"),
        ('value["account_id"] != ACCOUNT', "False"),
        ('value["mode"] != "observe"', "False"),
        ('type(value["max_pages"]) is not int', "False"),
        ('env={"PATH": "/usr/bin:/bin"}', 'env=dict(os.environ)'),
        ('if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:',
         'if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o022:'),
        ('call(["/usr/bin/systemctl", "start", SERVICE])', "pass"),
        ('argv += ["--journal", "/var/lib/radar-paper-execution/orders.jsonl"]', "pass"),
    ]),
    ("setup-alpaca-admin.py", "test_alpaca_admin_setup.py", [
        ("os.geteuid() != 0", "False"),
        ("hashlib.sha256(raw).hexdigest() != expected", "False"),
        ("if path.exists() or path.is_symlink():", "if False:"),
        ("info.st_uid != 0", "False"),
        ("os.link(temporary, path)", "os.replace(temporary, path)"),
        ('subprocess.run(["/usr/sbin/visudo", "-cf", temporary], check=True,\n'
         '                       env={"PATH": "/usr/bin:/bin"}, capture_output=True)', "pass"),
        ('("inspect", "activities", "maintain", "stop", "start")',
         '("inspect", "activities", "maintain", "stop", "start", "shell")'),
    ]),
]

for filename, testfile, cases in groups:
    source = (directory / filename).read_text()
    for correct, wrong in cases:
        if source.count(correct) != 1:
            sys.exit(f"Administrative mutation predicate changed: {filename}: {correct}")
        with tempfile.TemporaryDirectory(prefix="radar-admin-mutation-") as temporary:
            target = Path(temporary)
            (target / filename).write_text(source.replace(correct, wrong))
            (target / testfile).write_text((directory / testfile).read_text())
            result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
                                     "-p", testfile], capture_output=True, text=True)
            if (result.returncode == 0 or "Ran 6 tests" not in result.stderr
                    or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                    or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
                print(result.stdout)
                print(result.stderr)
                sys.exit(f"Administrative mutation not caught by executed tests: {filename}: {correct}")
            print(f"Caught administrative regression: {filename}: {correct}")
