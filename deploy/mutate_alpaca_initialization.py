#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply initialization/provisioning mistakes; require executed GitHub tests."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
groups = [
    ("initialize-alpaca-paper.py", "test_alpaca_initialization.py", 6, [
        ('if plan["mode"] != "observe" or any(directory.iterdir()):', 'if any(directory.iterdir()):'),
        ('if plan["mode"] != "observe" or any(directory.iterdir()):', 'if plan["mode"] != "observe":'),
        ("os.fsync(lock.fileno())", "pass"),
        ("        sync(directory)", "        pass"),
        ('report.get("expected_account_id") != plan["account_id"]', "False"),
        ('report.get("environment") != "paper"', "False"),
        ('report.get("checkpoint_written") is not True', "False"),
        ('report.get("claim_release_authorized") is not False', "False"),
        ('checkpoint.get("orders") != {}', "False"),
        ('claims.get("claims") != []', "False"),
        ("not 0 < before <= started <= completed <= finished", "False"),
        ("not 0 < info.st_size <= 4 * 1024 * 1024", "False"),
        ("os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW", "os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW"),
    ]),
    ("setup-alpaca-execution.py", "test_alpaca_execution_setup.py", 8, [
        ("hashlib.sha256(raw).hexdigest() != expected", "False"),
        ("hashlib.sha256(reader).hexdigest() != READER_SHA256", "False"),
        ("if set(result) != set(FILES):", "if False:"),
        ("entry.name in result", "False"),
        ("info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o600", "False"),
        ('plan["mode"] != "observe"', "False"),
        ('type(plan["max_pages"]) is not int', "False"),
        ("for prior in (library, *(Path(path) for path in STATES)):", "for prior in (library,):"),
        ("os.link(temporary, path)", "os.replace(temporary, path)"),
        ('subprocess.run(["/usr/bin/systemctl", "start", "radar-paper-initialize.service"], check=True, env=clean)', "pass"),
        ('subprocess.run(["/usr/bin/systemctl", "start", "radar-paper-maintenance.service"], check=True, env=clean)', "pass"),
    ]),
]

for filename, testfile, count, cases in groups:
    source = (directory / filename).read_text()
    for correct, wrong in cases:
        if source.count(correct) != 1:
            sys.exit(f"Initialization mutation predicate changed: {filename}: {correct}")
        with tempfile.TemporaryDirectory(prefix="radar-initialization-mutation-") as temporary:
            target = Path(temporary)
            for dependency in (filename, testfile, "maintain-alpaca-paper.py"):
                (target / dependency).write_text((directory / dependency).read_text())
            (target / filename).write_text(source.replace(correct, wrong))
            result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
                                     "-p", testfile], capture_output=True, text=True)
            if (result.returncode == 0 or f"Ran {count} tests" not in result.stderr
                    or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                    or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
                print(result.stdout)
                print(result.stderr)
                sys.exit(f"Initialization mutation not caught by executed tests: {filename}: {correct}")
            print(f"Caught initialization regression: {filename}: {correct}")
