#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Run actual paper-model regressions on GitHub; compiler refusals are not kills."""

from pathlib import Path
import re
import shlex
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
model = root / "crates/radar-cli/src/alpaca_paper_model.rs"
preflight = root / "crates/radar-cli/src/alpaca_preflight.rs"
originals = {path:path.read_bytes() for path in (model, preflight)}
unit = ["--bin", "radar", "alpaca_paper_model::tests::"]
risk = ["--bin", "radar", "paper_model_preflight_reports_assumptions"]
cases = [
    (model, b"self.account_id != account", b"self.account_id != account && account.is_empty()",
     "accept another account's simulation", unit, 4),
    (model, b"Some(self.max_quantity_nanoshares)", b"Some(u64::MAX)",
     "remove finite scenario capacity", risk, 1),
    (model, b"Some(self.fee_per_side_microusd)", b"Some(0)",
     "discard required fee reserve", risk, 1),
    (model, b".checked_add(spread)", b".checked_add(spread.saturating_sub(spread))",
     "omit spread from stress costs", unit, 4),
    (model, b"value.div_ceil(scale)", b"value / scale", "round scenario costs down", unit, 4),
    (model, b".checked_mul(2)", b".checked_mul(1)", "charge only one trade side", unit, 4),
    # Preserve production field/function reads so this actually executes tests.
    (preflight, b"super::alpaca_paper_model::unchanged(&inputs.model, inputs.model_path)?;",
     b"super::alpaca_paper_model::unchanged(&None, inputs.model_path.filter(|_| false))?;",
     "skip model reload at submission boundary",
     ["--bin", "radar", "paper_model_submit_reloads_before_risk"], 1),
    (preflight, b"model.validate(&intent.account_id)?;", b"let _ = model;",
     "defer initial model validation past credential boundary",
     ["--test", "alpaca_paper_model_process"], 1),
]

try:
    for path, correct, wrong, name, target, count in cases:
        original = originals[path]
        if original.count(correct) != 1:
            sys.exit(f"Paper model regression predicate changed: {name}")
        path.write_bytes(original.replace(correct, wrong))
        result = subprocess.run(shlex.split(sys.argv[1]) + ["test", "--locked", "-p", "radar-cli"]
            + target + ["--", "--nocapture"], cwd=root, capture_output=True, text=True, timeout=120)
        output = result.stdout + result.stderr
        executed = re.search(r"test result: FAILED\. (\d+) passed; (\d+) failed;", output)
        if (result.returncode == 0 or executed is None or sum(map(int, executed.groups())) != count
                or int(executed[2]) == 0):
            print(output)
            sys.exit(f"Paper model regression not caught by {count} executed tests: {name}")
        print(f"Caught paper model regression: {name}")
        path.write_bytes(original)
finally:
    for path, original in originals.items():
        path.write_bytes(original)
