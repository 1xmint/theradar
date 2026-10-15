#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Executed behavioral regressions for isolated paper ticks and inference roles."""

from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).parent
cases = [
    ("paper-control.py", 'if submission_refused(result, intent["client_order_id"]):', 'if False:'),
    ("paper-control.py", 'set(value) != fields or value.get("venue") != "alpaca"', 'value.get("venue") != "alpaca"'),
    ("paper-control.py", 'value.get("client_order_id") != client or value.get("broker_order_id") is not None', 'value.get("broker_order_id") is not None'),
    ("paper-control.py", '"reservation_created", "network_attempted",\n                "fills_reconciled"', '"fills_reconciled"'),
    ("paper-supervisor.py", 'if request["action"] == "submit" and shared.submission_refused(value, client):', 'if False:'),
    ("paper-supervisor.py", 'outcome="proposal_refused" if result.get("submission") ==', 'outcome="broker_response" if result.get("submission") =='),
    ("paper-supervisor.py", 'current = observation(collected, self.account, now)', 'current = collected'),
    ("paper-supervisor.py", 'not current["handover_enabled"] or not current["market_clock"]["is_open"]', 'False'),
    ("paper-supervisor.py", 'fresh["generation"] != current["generation"]', 'False'),
    ("paper-supervisor.py", 'not fresh["handover_enabled"]', 'False'),
    ("paper-supervisor.py", 'not fresh["market_clock"]["is_open"]', 'False'),
    ("paper-supervisor.py", 'self.clock() - current["observed_at_unix_ms"] > 120000', 'False'),
    ("paper-supervisor.py", 'if fresh["orders"]["working_before_cancellation"]:', 'if False:'),
    ("paper-supervisor.py", 'if record["client_order_id"] not in retained:', 'if False:'),
    ("paper-supervisor.py", 'and record["client_order_id"] not in current["orders"]["terminal"]', 'and False'),
    ("paper-supervisor.py", 'result = acknowledgement(result, request)', 'result = result'),
    ("paper-supervisor.py", 'older["observed_at_unix_ms"] < current["observed_at_unix_ms"]', 'True'),
    ("paper-supervisor.py", 'env={"PATH":"/usr/bin:/bin"}', 'env=dict(os.environ)'),
    ("paper-supervisor.py", 'shared.publish(path, record)  # Durable uncertainty precedes IPC.', 'pass  # Removed durable uncertainty.'),
    ("paper-supervisor.py", 'raise ValueError("Unknown retained supervisor entry; inspect rather than delete")', 'pass'),
    ("paper-control.py", 'if role == "supervisor" and action == "observe":', 'if role in ("web", "supervisor") and action == "observe":'),
    ("paper-control.py", 'if role == "supervisor" and action == "cancel":', 'if role in ("web", "supervisor") and action == "cancel":'),
    ("paper-control.py", 'if current != prior:', 'if False:'),
    ("paper-control.py", 'quote.get("as_of_unix_ms") != at', 'False'),
    ("paper-control.py", 'hashlib.sha256(raw_model).hexdigest() != self.config["model_sha256"]', 'False'),
    ("paper-control.py", 'result.get("cancellation") != "request_accepted"', 'False'),
    ("radar-codex-bridge.py", 'uid != expected_uid and command != ("exec", "-")', 'False'),
]
files = ("paper-supervisor.py", "paper-control.py", "radar-codex-bridge.py", "test_paper_supervisor.py",
    "test_paper_observation.py", "test_paper_control.py", "test_codex_bridge.py", "setup-private-connections.py")
for index, (filename, correct, wrong) in enumerate(cases, 1):
    source = (directory / filename).read_text()
    if source.count(correct) != 1:
        sys.exit(f"Paper supervisor mutation predicate changed: {correct}")
    with tempfile.TemporaryDirectory(prefix="radar-supervisor-mutation-") as temporary:
        root = Path(temporary)
        for name in files:
            (root / name).write_text((directory / name).read_text())
        (root / filename).write_text(source.replace(correct, wrong))
        # All three relevant suites run under discovery; original controller
        # tests remain present as fixtures without replacing real IPC behavior.
        result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", temporary,
            "-p", "test_*.py"], capture_output=True, text=True)
        if (result.returncode == 0 or "Ran " not in result.stderr
                or not any(marker in result.stderr for marker in ("FAIL: test_", "ERROR: test_"))
                or any(marker in result.stderr for marker in ("SyntaxError", "ImportError", "ModuleNotFoundError"))):
            print(result.stdout)
            print(result.stderr)
            sys.exit(f"Paper supervisor regression {index} not caught by executed tests")
        print(f"Caught paper supervisor regression {index}: {filename}: {correct}")
