<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0045 — Retain the complete research denominator in outcome reports

**Status:** Implementation prepared; GitHub verification pending. Research timer running.

Branch feat/alpaca-cohort-report from 19cded4, stacked on PR 405. Parent
documentation CI 38287366026 completed successfully in all 14 jobs, inspected.
The fully autonomous trading goal remains active. This increment supplies a
manual read-only evaluator, not order authority or a portfolio simulator.

## Checklist

- [x] Independently verify research installation and first retained model decision.
- [x] Include every frozen scheduled slot, including unknown and failed attempts.
- [x] Gate claim, terminal and outcome availability by the requested watermark.
- [x] Select the latest uniquely available exit capture by the exact fixed horizon.
- [x] Delegate quote/cost arithmetic to the existing pinned Rust scorer.
- [x] Keep missing comparisons and aggregate portfolio profit unknown.
- [ ] Inspect GitHub tests and all nine exact cohort mutation reapplications.
- [ ] Stage the verified reporter and demonstrate a real fixed-horizon report.

## Runtime evidence

Owner setup succeeded. Independent SSH reads on 2026-10-12 verified timer active
and enabled, worker Result=success and ExecMainStatus=0. Installed CLI SHA-256
12efd7e4fd958138840dc0056f84fe6ba5690cb7de01ce02065131e6cb2112d0
matches release 38285575372. Worker/unit hashes are recorded below for comparison
with exact committed 4c733c5 bytes:

- Worker: 0afcfaefbaa88c384bb4fd62272bfb0fe879e70448a9575c5dc1d749d5089014.
- Service: 3ac21ffa29b3a8a5789b2330fa612cb1e02761611f3e2cf3c50f84bdf2ec70b3.
- Timer: a0a1e004ebe55bb5274b022ce61c4b35eb65743d06d59441db286c00b88c2c78.

Research state is guardian:guardian 0700, public configuration root:root 0644.
The frozen campaign hash is
2bb51f42cfc025a03f8f529800127caaa948feee72df76a838006008e1e0f789,
starting at 1791835973176 ms, twelve slots ten minutes apart. Slot zero started
1791835981733 and finished 1791835992329 with status decision. Its retained
proposal is wait, citing a single quote with no directional edge over the fixed
horizon after assumed costs. The decision froze at 1791835992311 and its outcome
horizon is 1791836592311. The raw quote timestamp was 20:12:49.597882722 UTC;
after-hours quotes were fresh enough for research despite regular session close.
This is evidence of the actual unattended linked-model path, not profitability,
tradability, authenticated decision provenance or broker execution.

## Design limits

Caller: the operator runs the report against the retained research store and
recorder tape. The recorder's private directory requires owner terminal access;
the reporter does not change directory permissions or read API keys. Frozen
claim identities and experiment inputs must match. A claim or terminal beyond
the report watermark cannot enter results. Every planned slot remains in the
denominator; absence is not inferred to be a zero-return trade.

The report bounds capture count and bytes, refuses links/nonregular files, and
rejects arbitrary capture tie-breaking. Rust independently validates the selected
quote at the exact horizon. Scorer refusal is unscoreable, with no vendor error
body printed. Local mutable evidence and separate reads are not atomic or
authenticated. Hypothetical decisions can overlap, so the report deliberately
does not sum their outcomes into portfolio P&L. A learned historical signal,
held-out expectancy, verified execution costs/capacity, isolated execution and
recovery, and site handover still remain before autonomous trading activation.

## Handback

Implementation prepared; no local test, build, lint or mutation jobs run. GitHub
proof and real evaluation are pending. Current timer remains active, the old
recorder is preserved, and no broker order or website deployment was made.
