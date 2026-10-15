#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply paper control regressions; only executed test failures count."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
source = (directory / "paper-control.py").read_text()
cases = [
    # Peer refusal is deliberately enforced at both socket and dispatcher.
    # Remove both to exercise the security property rather than a redundant line.
    [("if uid not in control.roles:", "if False:"),
     ("if role is None or not isinstance(request, dict):", "if not isinstance(request, dict):")],
    [('if role == "supervisor" and action == "submit":', 'if role in ("supervisor", "web") and action == "submit":')],
    [('if role == "web" and action in ("configure", "pause"):', 'if role in ("web", "supervisor") and action in ("configure", "pause"):')],
    [('value["email"] != self.config["operator_email"]', "False")],
    [('retained["operator"] != value', "False")],
    [('prior["generation"] != request["generation"] or prior["generation"] == MAX', 'prior["generation"] == MAX')],
    [('not prior["enabled"] or prior["generation"] != request["generation"]', 'prior["generation"] != request["generation"]'),
     ('not current["enabled"] or current["generation"] != prior["generation"]', 'current["generation"] != prior["generation"]')],
    [('if not current["enabled"] or current["generation"] != prior["generation"]:', 'if False:')],
    [('publish(self.policy_path(old), policy(self.config["account_id"]))', 'pass')],
    [("os.link(temporary, path)", "os.replace(temporary, path)")],
    [('hashlib.sha256(read(MODEL)).hexdigest() != value["model_sha256"]', "False")],
    [('hashlib.sha256(read(READER, 32 * 1024 * 1024)).hexdigest() != READER_HASH', "False")],
    [('environment = {"PATH": "/usr/bin:/bin"}', 'environment = dict(os.environ)')],
    [('except Exception:', 'except Exception as error:'),
     ('packet = {"error":"Paper control outcome unresolved; read status and reconcile before retry"}', 'packet = {"error":str(error)}')],
    [('type(value) is int and minimum <= value <= MAX', 'isinstance(value, int) and minimum <= value <= MAX')],
    [('now - start > 120', 'False')],
    [('len(set(identities)) != 3', 'False')],
]

for index, replacements in enumerate(cases):
    mutated = source
    for correct, wrong in replacements:
        expected = 2 if correct == 'not prior["enabled"] or prior["generation"] != request["generation"]' else 1
        if mutated.count(correct) != expected:
            sys.exit(f"Paper control mutation predicate changed: {correct}")
        mutated = mutated.replace(correct, wrong)
    with tempfile.TemporaryDirectory(prefix="radar-control-mutation-") as temporary:
        target = Path(temporary)
        (target / "paper-control.py").write_text(mutated)
        (target / "test_paper_control.py").write_text((directory / "test_paper_control.py").read_text())
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
            "-p", "test_paper_control.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran 12 tests" not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or any(marker in result.stderr for marker in ("SyntaxError", "ImportError", "ModuleNotFoundError"))):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Paper control regression {index + 1} not caught by executed tests")
        print(f"Caught paper control regression {index + 1}: {replacements[0][0]}")
