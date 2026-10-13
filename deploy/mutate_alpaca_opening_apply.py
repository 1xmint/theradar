#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Execute the six application tests against exact one-time repair regressions."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
source = (directory / "apply-alpaca-opening.py").read_text()
cases = [
    ("os.geteuid() != 0", "False"),
    ("hashlib.sha256(raw).hexdigest() != ADMIN_HASH", "False"),
    ("hashlib.sha256(raw).hexdigest() != expected", "False"),
    ("hashlib.sha256(read(admin.READER, 32 * 1024 * 1024)).hexdigest() != READER_HASH", "False"),
    ("if TARGET.exists() or TARGET.is_symlink():", "if False:"),
    ("os.link(temporary, path)", "os.replace(temporary, path)"),
    ('report.get("checkpoint_written") is not True', "False"),
    ('claims.get("claims") != []', "False"),
    ('acknowledgement.get("activity_id") != ACTIVITY', "False"),
    ('acknowledgement.get("broker_posting_time_verified") is not False', "False"),
    ('prefix(STATE / "orders.jsonl", before[0]) != before', "False"),
    ('prefix(STATE / "orders.jsonl")[0] <= before[0]', "False"),
    ('admin.call(["/usr/bin/systemctl", "start", admin.SERVICE])', "pass"),
    ('env={"PATH":"/usr/bin:/bin"}', "env=dict(os.environ)"),
]

for correct, wrong in cases:
    if source.count(correct) != 1:
        sys.exit(f"Opening application regression predicate changed: {correct}")
    with tempfile.TemporaryDirectory(prefix="radar-opening-application-") as temporary:
        target = Path(temporary)
        (target / "apply-alpaca-opening.py").write_text(source.replace(correct, wrong))
        for filename in ("test_alpaca_opening_apply.py", "radar-paper-admin.py"):
            (target / filename).write_bytes((directory / filename).read_bytes())
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
            "-p", "test_alpaca_opening_apply.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran 6 tests" not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or "SyntaxError" in result.stderr or "ImportError" in result.stderr):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Opening application regression not caught by executed tests: {correct}")
        print(f"Caught opening application regression: {correct}")
