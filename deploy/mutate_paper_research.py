#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply research-context bugs in executed tests on GitHub."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
cases = [
    ("paper-control.py", 'len(value["text"].encode()) > 4000', 'len(value["text"]) > 4000'),
    ("paper-control.py", 'if role == "web" and action in ("research_read", "research_add", "research_reply"):',
        'if role in ("web", "supervisor") and action in ("research_read", "research_add", "research_reply"):'),
    ("paper-control.py", 'retained[0]["operator"] != request["operator"]', 'False'),
    ("paper-control.py", 'if existing is not None:', 'if False:'),
    ("paper-control.py", 'len(state["messages"]) >= 128', 'len(state["messages"]) > 128'),
    ("paper-control.py", 'state["messages"][-1]["received_at_unix_ms"] > at', 'False'),
    ("paper-control.py", 'item["received_at_unix_ms"] <= at', 'True'),
    ("paper-control.py", 'if os.path.lexists(state_path):', 'if False:'),
    ("paper-supervisor.py", 'research["source_claims_verified"] is not False', 'False'),
    ("paper-supervisor.py", 'research["as_of_unix_ms"] != value["observed_at_unix_ms"]', 'False'),
    ("paper-supervisor.py", 'message["received_at_unix_ms"] > value["observed_at_unix_ms"]', 'False'),
    ("paper-supervisor.py", 'if key != "research"', 'if True'),
]
files = ("paper-control.py", "paper-supervisor.py", "test_paper_research.py", "test_paper_supervisor.py")
for index, (filename, correct, wrong) in enumerate(cases, 1):
    source = (directory / filename).read_text()
    if source.count(correct) != 1:
        sys.exit("Research regression predicate changed: " + correct)
    with tempfile.TemporaryDirectory(prefix="radar-research-mutation-") as temporary:
        root = Path(temporary)
        for name in files: (root / name).write_text((directory / name).read_text())
        (root / filename).write_text(source.replace(correct, wrong))
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
            "-p", "test_*.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran " not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or any(marker in result.stderr for marker in ("SyntaxError", "ImportError", "ModuleNotFoundError"))):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Research regression {index} not caught by executed tests")
        print(f"Caught paper research regression {index}: {filename}: {correct}")
