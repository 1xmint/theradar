#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reapply paper panel behavior faults through its eight real frontend tests."""

import os
from pathlib import Path
import re
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
path = root / "web/src/AlpacaPaper.tsx"
source = path.read_bytes()
cases = [
    (b'action === "enable"', b'action !== "enable"', 1),
    (b'await paper.pause(snapshot.generation)', b'await paper.configure(snapshot.generation, preferences, true)', 1),
    (b'setReady(false)', b'setReady(true)', 2),
    (b'await read();', b'await Promise.resolve();', 1),
    (b'const value = checked(await paper.status(signal));',
     b'await paper.configure(0, empty, true); const value = checked(await paper.status(signal));', 1),
    (b'value.environment !== "paper"', b'false', 1),
    (b'preferences, action === "enable"', b'{ ...preferences, agent_decides: empty.agent_decides }, action === "enable"', 1),
    (b'field !== "daily_loss_usd" || preferences.daily_loss_enabled', b'true', 1),
    (b'paper.configure(snapshot.generation,', b'paper.configure(0,', 1),
]

try:
    for correct, wrong, count in cases:
        if source.count(correct) != count:
            sys.exit(f"Paper panel regression predicate changed: {correct.decode()}")
        path.write_bytes(source.replace(correct, wrong))
        result = subprocess.run(["node", str(root / "web/node_modules/vitest/vitest.mjs"),
            "run", "src/AlpacaPaper.test.tsx"], cwd=root / "web", capture_output=True, text=True,
            timeout=90, env={**os.environ, "NO_COLOR":"1", "FORCE_COLOR":"0"})
        output = re.sub(r"\x1b\[[0-9;]*m", "", result.stdout + result.stderr)
        executed = re.search(r"Tests\s+(\d+) failed(?: \| (\d+) passed)? \(8\)", output)
        if (result.returncode == 0 or executed is None
                or int(executed[1]) + int(executed[2] or 0) != 8):
            print(output)
            sys.exit(f"Paper panel regression not caught by eight executed tests: {correct.decode()}")
        print(f"Caught paper panel regression: {correct.decode()}")
finally:
    path.write_bytes(source)
