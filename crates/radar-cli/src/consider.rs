// SPDX-License-Identifier: Apache-2.0
//! `radar consider` — the whole decision lane, on recorded data.
//!
//! Runs everything Radar knows how to do, in order, against tokens actually
//! recorded on this instance: assemble candidates at the watermark, apply the
//! strategy, pay for exit analysis only where it could change an answer, then
//! put whatever survives through the risk kernel.
//!
//! It commits nothing. Under [`Policy::CLOSED`] — the default, and what ships —
//! the kernel refuses everything, so the output is a complete account of what
//! the system *would* do and why it would not do it. That is the point: the
//! evidence Josh gated deploying capital on is exactly this report, run over
//! enough days to mean something.
//!
//! # The tiering is real, not described
//!
//! The strategy runs twice. The first pass costs nothing: no exit report, so
//! every candidate fails on at least `NoExitSimulated`, and the ones failing on
//! *only* that are the ones where a paid look could change the answer. The
//! second pass spends a network call on those alone.
//!
//! At ~35,000 launches a day this is the difference between a few calls and tens
//! of thousands. It also makes the tier falsifiable: the report says how many
//! candidates the paid tier was spent on and how many it changed.

use std::collections::BTreeMap;

use radar_asof::AsOf;
use radar_backfill::launch_block::CryptoHouseBlocks;
use radar_graph::LaunchBlockSource;
use radar_risk::{Policy, PortfolioState, Proposal, Verdict, evaluate};
use radar_sim::{JupiterQuoter, RpcClient};
use radar_store::Reader;
use radar_strategy::{Candidate, CreatorEdge, Decision, PassReason, Strategy, Universe, universe};
use radar_types::{Address, Custody, Market, MicroUsd, Portfolio};

/// How many candidates the paid tier will be spent on in one pass.
///
/// A cap rather than a budget in dollars, because the calls here are free —
/// Jupiter's lite tier and a public RPC. It exists to bound *time*, and to stop
/// a first run against a large store from making thousands of requests to
/// somebody's free endpoint.
const PAID_TIER_CAP: usize = 25;

/// Where operations against the account are recorded.
///
/// Alongside the analyst's journal rather than inside it: the two are read by
/// different commands and one of them is on the money path. Nothing writes this
/// file yet — execution is shut — and an absent one opens as a log with nothing
/// outstanding, which is the honest reading for an instance that has never
/// operated.
const OPERATIONS_JOURNAL: &str = "data/execution/operations.jsonl";

/// Runs the lane.
///
/// # Errors
///
/// Returns a message if the store cannot be read or has recorded nothing.
pub fn run(
    reader: &Reader,
    window: u64,
    cap: usize,
    record_to: Option<&str>,
    pricing: Pricing,
) -> Result<(), String> {
    let watermark = reader
        .watermark()
        .map_err(|e| format!("cannot read the store: {e}"))?
        .ok_or("the store has recorded nothing yet")?;
    let as_of = AsOf::at(watermark);

    let universe = universe(reader, as_of).map_err(|e| format!("cannot read the store: {e}"))?;
    let recent = universe.recent(window);

    println!("watermark    : slot {watermark}");
    println!("launches     : {} recorded", universe.launches.len());
    println!("creators     : {}", universe.creators.len());
    println!(
        "considering  : {} launched within {window} slots\n",
        recent.len()
    );

    if recent.is_empty() {
        println!("Nothing recent enough to consider. Widen --window, or let the recorder run.");
        return Ok(());
    }

    // Tier 0 and 1: free. No exit report, so every candidate fails at least on
    // NoExitSimulated, and the interesting ones fail on nothing else.
    let strategy = CreatorEdge::default();
    let FreeTier {
        tally,
        worth_paying_for,
    } = free_tier(&universe, &strategy, &recent, pricing);

    println!(
        "free tier — why {} candidates were passed over:",
        recent.len()
    );
    for (reason, count) in &tally {
        println!("  {count:>6}  {reason:?}");
    }

    println!(
        "\n{} candidate(s) fail on nothing a paid look cannot resolve.",
        worth_paying_for.len()
    );
    if worth_paying_for.is_empty() {
        println!("Spending on exit analysis would change no answer, so nothing is spent.");
        return Ok(());
    }

    let budget = worth_paying_for.len().min(cap);
    if worth_paying_for.len() > budget {
        println!("Examining the first {budget} of them this pass.");
    }

    // One aggregator call whichever instrument is chosen: the SOL price has no
    // other source here, and a wrong one silently rescales every position in the
    // system. One call a pass against 320 is the whole of this change.
    let quoter = JupiterQuoter::default();
    println!("exit pricing : {}", pricing.label());
    let Some(sol_price) = radar_sim::sol_price_micro_usd(&quoter) else {
        // A wrong SOL price silently rescales every position in the system, so
        // an absent one stops the pass rather than defaulting.
        println!("\nSOL price unavailable — refusing to size anything without it.");
        return Ok(());
    };
    println!("SOL price    : ${:.2}\n", price_dollars(sol_price));

    let mut examined: Vec<(radar_store::Decision, Address)> = Vec::new();
    // Constructed here rather than inside `paid_tier`, so the function stays
    // callable without a network. The clock stays at the edge: the launch-block
    // window is a fixed width back from *now* rather than from a calendar date,
    // so its cost does not grow every day -- see `launch_block::LOOKBACK_HOURS`.
    let blocks = CryptoHouseBlocks::new(
        radar_backfill::cryptohouse::Client::default(),
        &radar_store::from_epoch(radar_store::now_epoch()),
    );
    let rpc = RpcClient::default();
    let pass = paid_tier(
        &universe,
        &strategy,
        worth_paying_for.iter().take(budget),
        &Sources {
            blocks: &blocks,
            structures: &rpc,
            quoter: &quoter,
            pricing,
        },
        sol_price,
        watermark,
        &mut examined,
    );

    let verdicts_by_mint = verdicts(&pass.proposals, watermark, reader)?;

    if let Some(dir) = record_to {
        // The kernel's verdict is folded in only now, because a decision is not
        // complete until the thing with the authority has seen it.
        for (record, mint) in &mut examined {
            if let Some(v) = verdicts_by_mint.get(mint) {
                record.kernel_outcome = Some(match v {
                    Verdict::Authorised(_) => radar_store::KernelOutcome::Authorised,
                    Verdict::Refused { .. } => radar_store::KernelOutcome::Refused,
                });
                if let Verdict::Refused { reasons } = v {
                    record.kernel_reasons = reasons.iter().map(|r| format!("{r:?}")).collect();
                }
            }
        }
        write_decisions(dir, &examined)?;
    }
    Ok(())
}

/// What the free tier learned: why each candidate was passed over, and which
/// ones a paid look could still change the answer for.
struct FreeTier {
    /// How many candidates each reason was raised against.
    tally: BTreeMap<PassReason, usize>,
    /// The mints whose only failures a paid look removes.
    worth_paying_for: Vec<Address>,
}

/// The free tier, and the gate that decides what the paid one is spent on.
///
/// Split out of [`run`] because it is the only part of the pass that touches
/// no network: `run` cannot be tested past this point without one, and this
/// gate closing on everything is exactly how the command came to print its
/// tally and then do nothing at all. See
/// `the_free_tier_names_the_venue_the_paid_tier_will_price_on`.
fn free_tier(
    universe: &Universe,
    strategy: &CreatorEdge,
    recent: &[Address],
    pricing: Pricing,
) -> FreeTier {
    let mut tally: BTreeMap<PassReason, usize> = BTreeMap::new();
    let mut worth_paying_for = Vec::new();

    for mint in recent {
        let Some(candidate) = universe.candidate(mint, None, None) else {
            continue;
        };
        // The venue the paid tier *will* measure on, named before it is spent.
        // Not decoration: the strategy refuses a capacity it cannot attribute
        // to a venue (rule 9), and a free-tier candidate with no venue fails on
        // `VenueUnknown` — which no exit report removes, so the gate below
        // closes on every mint and the paid tier is never reached.
        let candidate = candidate.measured_on(pricing.market());
        let reasons = strategy.consider(&candidate).reasons().to_vec();
        for reason in &reasons {
            *tally.entry(*reason).or_default() += 1;
        }
        // NoExitSimulated and NoPrice are the two that a paid look removes.
        // Anything else failing means the answer cannot change, so the call
        // would buy nothing.
        if reasons
            .iter()
            .all(|r| matches!(r, PassReason::NoExitSimulated | PassReason::NoPrice))
        {
            worth_paying_for.push(*mint);
        }
    }

    FreeTier {
        tally,
        worth_paying_for,
    }
}

/// Appends the examined candidates to the store.
///
/// # Errors
///
/// Returns a message if the store cannot be opened or written. A recording
/// failure is loud rather than swallowed: the whole point of the pass is the
/// record, so a run that printed its findings and failed to keep them has not
/// done the job.
fn write_decisions(dir: &str, examined: &[(radar_store::Decision, Address)]) -> Result<(), String> {
    let mut writer = radar_store::Writer::open(dir, 512)
        .map_err(|e| format!("cannot open the store for writing: {e}"))?;
    for (record, _) in examined {
        writer
            .append_decision(record.clone())
            .map_err(|e| format!("cannot record a decision: {e}"))?;
    }
    writer
        .flush()
        .map_err(|e| format!("cannot flush decisions: {e}"))?;
    println!(
        "
recorded {} decision(s) to {dir}/decisions",
        examined.len()
    );
    Ok(())
}

/// The account on record, and the kernel's view of it.
///
/// # Why an unreadable read stops the pass
///
/// This used to end in `unwrap_or_default()`, and the comment above it said
/// that treating a failed read as "no positions" was safe *because nothing
/// writes a position yet*. That was true and it was a fuse with no date on it:
/// the day something does trade, a read that fails still returns an empty
/// portfolio, every limit is measured against zero deployed, and the kernel
/// authorises against capital it cannot see.
///
/// AGENTS.md rule 9 — absent is not zero, and unknown is not safe — so the
/// error propagates. Refusing a pass costs nothing that cannot be recovered by
/// running it again; sizing against an invented zero does not.
///
/// The same applies one step further in. A [`Portfolio`] that knows it cannot
/// account for something on record is a portfolio whose totals are **lower
/// bounds**, and a limit checked against a lower bound is a limit that binds
/// late. So [`Portfolio::incompleteness`] stops the pass too.
///
/// # Errors
///
/// Returns a message when the positions cannot be read, or when what comes back
/// cannot be fully accounted for.
fn inventory(
    reader: &Reader,
    watermark: radar_types::Slot,
) -> Result<(Portfolio, PortfolioState), String> {
    let rows = reader
        .read_positions(AsOf::at(watermark))
        .map_err(|e| format!("positions unreadable, so there is nothing to size against: {e}"))?;
    let folded = radar_store::fold_positions(rows);

    // `Custody::Unattributed` rather than an invented address. No position row
    // names a wallet and this instance has none configured, so the honest value
    // is the one that can hold nothing and reserve nothing (rule 8).
    let mut portfolio = radar_store::portfolio_from(Custody::Unattributed, watermark, &folded);

    // Capital an unfinished operation already claimed is not capital this pass
    // may size against, and the claim lives in a process that may have died
    // since. Re-taking it happens **before** the kernel sees the account: a
    // portfolio assembled from balances alone reports a wallet that looks
    // richer than it is by exactly the amount somebody else's transaction is
    // about to spend.
    //
    // The file does not exist on any instance today, and an operations journal
    // that was never written to opens empty — nothing outstanding, nothing
    // re-taken. That is a measurement rather than a silence: the log
    // distinguishes it from a file it could not read, which is an error here.
    let mut operations = radar_journal::OperationLog::open(OPERATIONS_JOURNAL).map_err(|e| {
        format!("the operations journal at {OPERATIONS_JOURNAL} cannot be read, so what is already claimed is unknown: {e}")
    })?;
    operations.rehold(&mut portfolio).map_err(|e| {
        format!(
            "an operation still outstanding cannot be re-held against the account ({e}); refusing to size new risk while a claim is unaccounted for"
        )
    })?;

    if let Some(gap) = portfolio.incompleteness() {
        return Err(format!(
            "the recorded inventory cannot be fully accounted for ({gap:?}); refusing to size new risk against totals that are lower bounds"
        ));
    }

    // `halted` and `consecutive_failures` are supplied rather than defaulted:
    // positions cannot know either, and the permissive answer arriving silently
    // from a component that does not know is the shape rule 9 warns about.
    // Zero failures is honest while nothing executes; there is no execution
    // record yet to read them from.
    let state = radar_strategy::state_from(
        &folded,
        watermark,
        radar_strategy::Operator {
            halted: false,
            consecutive_failures: 0,
        },
    );
    Ok((portfolio, state))
}

/// The paid tier: the two calls that cost money, in the order that spends least.
///
/// Split out because it is the part with a budget attached, and because reading
/// the free tier's tally should not mean scrolling past the spending.
/// Where a mint's account structure comes from.
///
/// A trait for one method, so [`paid_tier`] can be driven without a network.
/// The concrete implementation is [`RpcClient`]; the reason this exists is that
/// the alternative was a function nothing could call, and mutation testing
/// found the consequence: `paid_tier` could be replaced with "return no
/// proposals" and every test still passed.
///
/// That is not a hypothetical defect. It is [LEARNINGS](../../../LEARNINGS.md)
/// 10 exactly -- a live run over 41,254 candidates raised zero proposals, and
/// zero read as a fact about the market when it was a fact about the probe.
pub trait Structures {
    /// The mint account, or `None` when it could not be read.
    ///
    /// `None` rather than an error: the caller records absence and carries on,
    /// and the strategy refuses on an unreadable structure rather than treating
    /// it as clean (rule 9).
    fn mint_structure(&self, mint: &Address) -> Option<radar_sim::MintStructure>;

    /// A curve-backed quoter for this mint, or `None` when the curve could not
    /// be read.
    ///
    /// `None` is "cannot price the exit", never "no limit" — the caller builds a
    /// report with an empty curve, which is not exitable. Rule 9: an
    /// unmeasurable capacity is `None`, and `None` means cannot exit.
    fn depth(&self, mint: &Address) -> Option<radar_sim::curve::Depth>;
}

impl Structures for RpcClient {
    fn mint_structure(&self, mint: &Address) -> Option<radar_sim::MintStructure> {
        RpcClient::mint_structure(self, mint).ok()
    }

    fn depth(&self, mint: &Address) -> Option<radar_sim::curve::Depth> {
        RpcClient::depth(self, mint).ok()
    }
}

/// Where an exit's price comes from.
///
/// # Why this is a choice and not a constant
///
/// `radar consider` runs hourly on the box with `--cap 40`, and
/// `JupiterQuoter` asks the aggregator **eight times per candidate** to build a
/// quote ladder — about 320 calls an hour, against a lane shut by
/// `Policy::CLOSED` that cannot trade whatever the answer is. Radar is a guest
/// on that free tier (ADR 0002), and spending it on a question nothing acts on
/// is the shape of cost this repository keeps finding in its own instruments.
///
/// The curve is two account reads and pure arithmetic afterwards, and research
/// 0022 asks for it by name: *price the exit off the curve rather than off a
/// quote ladder*. It is also the same instrument on both sides, which is
/// `0016`'s lesson — the most expensive mistake in this repository's history was
/// an entry priced as a bid against an exit priced as a mid.
///
/// Jupiter stays reachable behind `--quoter jupiter` because the two are
/// different instruments and the difference is a measurement somebody should
/// take. LEARNINGS 18: two instruments compared as if they were one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pricing {
    /// The bonding curve itself.
    #[default]
    Curve,
    /// The aggregator's quote ladder.
    Jupiter,
}

impl Pricing {
    /// Reads the `--quoter` value.
    ///
    /// # Errors
    ///
    /// An unrecognised value is refused rather than defaulted: a typo that
    /// silently picked one instrument while the operator believed they had the
    /// other is the arrangement LEARNINGS 18 is about.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "curve" => Ok(Self::Curve),
            "jupiter" => Ok(Self::Jupiter),
            other => Err(format!(
                "unknown --quoter {other}; expected curve or jupiter"
            )),
        }
    }

    /// The venue a capacity measured with this instrument was measured on.
    ///
    /// This is the only place that knows. `Pricing::Curve` reads the bonding
    /// curve's own accounts, so the venue is exactly
    /// [`Market::PUMP_FUN_BONDING_CURVE`] and its `pool` is `None` because the
    /// curve account is `["bonding-curve", mint]` under that program.
    ///
    /// `Pricing::Jupiter` is `None`, and that is rule 9 rather than a gap. The
    /// aggregator returns a *route* — it picks whatever pools price the size
    /// best, may split across several, and does not report which. There is no
    /// single `Market` that depth belongs to, so the honest answer is that the
    /// venue is unknown and the proposal must refuse. Naming the curve here
    /// would put curve-shaped identity on AMM-measured depth, which is the
    /// collapse `crates/radar-risk/tests/two_markets_are_two_trades.rs` exists
    /// to prevent.
    #[must_use]
    pub const fn market(self) -> Option<Market> {
        match self {
            Self::Curve => Some(Market::PUMP_FUN_BONDING_CURVE),
            Self::Jupiter => None,
        }
    }

    /// What to print beside the pass, so a reader knows which instrument
    /// produced the capacity they are looking at.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Curve => "curve (two account reads per candidate)",
            Self::Jupiter => "jupiter (eight quotes per candidate)",
        }
    }
}

/// What a paid pass produced, and what it declined.
///
/// The counts are returned rather than only printed, and that is the point: as
/// locals feeding a `println!` they could be corrupted -- incremented by the
/// wrong operator, compared the wrong way round -- with nothing able to observe
/// it. Mutation testing found seven such spots in this function.
///
/// A count nobody can read is a count nobody can check, and these are the
/// numbers that say whether a gate is working or merely silent.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pass {
    /// Proposals the strategy raised.
    pub proposals: Vec<Proposal>,
    /// Candidates refused on launch-block shape, before any exit was probed.
    pub refused_on_shape: usize,
    /// Candidates whose launch block could not be read at all.
    ///
    /// Counted apart from "looked and found clean": a fetch that fails leaves
    /// the verdict absent, the strategy correctly declines to refuse on an
    /// absence, and the gate is then silently off.
    pub look_failed: usize,
}

/// The lines a pass prints about what it declined.
///
/// Extracted so the two thresholds are testable. Inline they were `if n > 0`
/// guards around a `println!`, which mutation testing could turn into `<`, `>=`
/// or `==` without any test noticing.
#[must_use]
pub fn render_pass_notes(pass: &Pass) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    if pass.refused_on_shape > 0 {
        let _ = writeln!(
            out,
            "{} candidate(s) refused on shape before any exit probe was paid for.",
            pass.refused_on_shape
        );
    }
    if pass.look_failed > 0 {
        out.push_str(
            "  A candidate whose launch block could not be read carries no verdict,
               and the strategy will not refuse on an absence — so those passed this
               gate without being examined rather than by being clean.
",
        );
    }
    out
}

/// The two paid calls, over one source of blocks, structures and quotes.
///
/// Every dependency arrives as an argument rather than being constructed
/// inside. That is what makes the function callable at all -- see
/// [`Structures`] for what it cost to have skipped it.
pub struct Sources<'s, B, S, Q> {
    /// Launch blocks, for the coordination read.
    pub blocks: &'s B,
    /// Mint accounts.
    pub structures: &'s S,
    /// The exit quoter, used only under [`Pricing::Jupiter`].
    pub quoter: &'s Q,
    /// Which instrument prices the exit.
    pub pricing: Pricing,
}

/// The exit report for one mint, from whichever instrument was chosen.
///
/// Both arms call the same search with the same budget; only the quoter
/// differs, which is what makes the two comparable at all. Split out of
/// [`paid_tier`] because it is the whole of the instrument choice, and that
/// function is already at its length.
fn capacity_of<B, S, Q>(
    sources: &Sources<'_, B, S, Q>,
    quoter: &Q,
    mint: &Address,
    structure: Option<radar_sim::MintStructure>,
    search: radar_sim::exit::Search,
) -> radar_sim::ExitReport
where
    S: Structures,
    Q: radar_sim::Quoter,
{
    match sources.pricing {
        Pricing::Jupiter => radar_sim::discover_capacity(quoter, mint, structure, search),
        Pricing::Curve => match sources.structures.depth(mint) {
            Some(depth) => radar_sim::discover_capacity(&depth, mint, structure, search),
            // Rule 9. A curve that could not be read is an unmeasurable exit,
            // and an empty report is not exitable: it is not a capacity of zero
            // and it is certainly not "no limit found".
            None => radar_sim::ExitReport::build(*mint, structure, Vec::new()),
        },
    }
}

fn paid_tier<'a, B, S, Q>(
    universe: &Universe,
    strategy: &CreatorEdge,
    mints: impl Iterator<Item = &'a Address>,
    sources: &Sources<'_, B, S, Q>,
    sol_price: MicroUsd,
    watermark: radar_types::Slot,
    examined: &mut Vec<(radar_store::Decision, Address)>,
) -> Pass
where
    B: radar_graph::LaunchBlockSource,
    B::Error: std::fmt::Display,
    S: Structures,
    Q: radar_sim::Quoter,
{
    let rpc = sources.structures;
    let blocks = sources.blocks;
    let quoter = sources.quoter;
    let mut proposals = Vec::new();
    let mut refused_on_shape = 0usize;
    // Counted apart from "looked and found clean". A fetch that fails leaves the
    // verdict absent, the strategy correctly declines to refuse on an absence,
    // and the gate is then silently off. Without this a broken source and a
    // clean population produce identical output.
    //
    // The shapes are kept rather than counted, because the count alone had the
    // same defect one level up: `read: 25, unreadable: 0` is what a detector
    // whose constant has moved prints too. See [`render_shapes`].
    let mut shapes = radar_graph::Distribution::new();
    let mut look_failed = 0usize;

    let prevalence_table = prevalence_table_of(blocks);

    for mint in mints {
        // The launch-block look runs first because it is the cheaper of the two
        // paid calls and it can end the question. Probing the exit of a token
        // whose curve was already bought out by whoever arranged it is money
        // spent to be told something the block said for less.
        // The shape as well as the verdict. Recording only the label is what
        // made the last drift invisible for nine days (ADR 0012): a threshold
        // fitted to a count the store discarded can only be re-fitted by
        // measuring the whole world again.
        let mut launch_shape = None;
        let coordination = match universe.launches.get(mint) {
            Some(facts) => match blocks.shape_at(mint, facts.slot) {
                Ok(shape) => {
                    shapes.observe(shape);
                    launch_shape = Some(shape);
                    let at_launch = radar_graph::assess(shape).coordination;

                    // The launch block is not the only place a bundle can
                    // appear. A token can sit dormant for years and be bundled
                    // by whoever picks it up, and reading only the launch leaves
                    // it labelled clean on exactly the day that matters.
                    //
                    // One query for the token's whole window, not one per slot.
                    // A failure here is *not* counted as `look_failed`: the
                    // launch block was read, so the coordination gate is not
                    // silently off, and conflating the two would make a broken
                    // sweep look like an unread launch.
                    let later = match blocks.bundle_slots(mint, facts.slot) {
                        Ok(rows) => radar_graph::ongoing::strongest(rows),
                        Err(e) => {
                            eprintln!("  {mint}  later blocks unreadable: {e}");
                            None
                        }
                    };
                    if let Some(sighting) = newly_bundled(later, at_launch) {
                        println!(
                            "  {mint}  bundled after launch: {:?} at {:?}",
                            sighting.coordination, sighting.when
                        );
                    }

                    // The stronger of the two verdicts. A launch that read clean
                    // and a later block that did not is a token to refuse, and
                    // taking the launch alone would refuse nothing.
                    Some(later.map_or(at_launch, |s| s.coordination.max(at_launch)))
                }
                Err(e) => {
                    look_failed += 1;
                    eprintln!("  {mint}  launch block unreadable: {e}");
                    None
                }
            },
            None => None,
        };

        // Only asked when the table can answer. A second cheap single-block
        // read, skipped entirely when the answer would be discarded.
        let prevalence = prevalence_table.as_ref().and_then(|table| {
            let facts = universe.launches.get(mint)?;
            let authorities = blocks.authorities_at(mint, facts.slot).ok()?;
            table.strongest_of(&authorities)
        });

        if coordination.is_some_and(radar_graph::Coordination::is_actionable) {
            refused_on_shape += 1;
            println!("  {mint}  launch block looks arranged — not probing the exit");
            // Recorded before the `continue`, because this is a decision and it
            // is the strongest one Radar makes. Skipping it made the decisions
            // table structurally incapable of holding a `Likely` verdict, so a
            // monitor counting them read 0 of 779 and reported a working
            // detector as one that had gone quiet -- a filter selecting the
            // sample, and the sample then supporting a confident conclusion
            // about the selection. LEARNINGS 7 and 10, a third time.
            if let Some(base) = universe.candidate(mint, None, Some(sol_price)) {
                // Named here too, for the same reason as the priced branch: the
                // instrument is chosen and known even though this candidate is
                // refused before it is used. Without it the recorded decision
                // carries a spurious `VenueUnknown` beside the finding that
                // actually stopped it, and a reader of the decisions table
                // cannot tell which one did.
                let base = base.measured_on(sources.pricing.market());
                let candidate = match coordination {
                    Some(verdict) => base.with_coordination(verdict),
                    None => base,
                };
                let decision = strategy.consider(&candidate);
                examined.push((
                    record_of(
                        &candidate,
                        &decision,
                        strategy,
                        None,
                        watermark,
                        prevalence,
                        launch_shape,
                    ),
                    candidate.mint,
                ));
            }
            continue;
        }

        let structure = rpc.mint_structure(mint);
        // Discovered, not assumed. This used to quote a hardcoded 1_000_000_000
        // base units for every token — roughly 0.00005% of a pump.fun supply —
        // so the "capacity" it measured was worth a fraction of a cent and every
        // candidate was refused as CapacityBelowFloor. Zero proposals read as a
        // fact about the market and was a fact about the probe. LEARNINGS 10.
        let exit = capacity_of(sources, quoter, mint, structure, search_for(strategy));
        let Some(candidate) = universe.candidate(mint, Some(exit), Some(sol_price)) else {
            continue;
        };
        // The venue that exit was measured on, from the instrument that measured
        // it. `capacity_of` above is the only thing that knows, and the strategy
        // used to write the curve in as a constant regardless.
        let candidate = candidate.measured_on(sources.pricing.market());
        let candidate = match coordination {
            Some(c) => candidate.with_coordination(c),
            // Carried as absent rather than as clean. The strategy will not
            // refuse on it, and it will not treat it as a pass either.
            None => candidate,
        };
        report_one(
            strategy,
            &candidate,
            &mut proposals,
            watermark,
            examined,
            prevalence,
            launch_shape,
        );
    }

    print!("{}", render_shapes(&shapes, look_failed));
    let pass = Pass {
        proposals,
        refused_on_shape,
        look_failed,
    };
    print!("{}", render_pass_notes(&pass));
    pass
}

/// The widest bar drawn, in characters.
const BAR_WIDTH: usize = 28;

/// The later sighting, but only when it says something the launch block did not.
///
/// Extracted from the line that prints it because a comparison inside a
/// `println!` is a comparison no test can reach: mutation testing flipped this
/// `>` to `==`, `<` and `>=` and every one of them survived. All three are
/// wrong in the same direction -- `>=` and `==` repeat the launch verdict as
/// though it were news, and `<` announces a *weaker* later reading as a
/// strengthening -- and the line exists precisely to surface the case the
/// launch block missed.
///
/// This is only what gets *printed*. The verdict itself takes the stronger of
/// the two regardless, so a wrong answer here is a silent report rather than a
/// silent trade.
fn newly_bundled(
    later: Option<radar_graph::ongoing::Sighting>,
    at_launch: radar_graph::Coordination,
) -> Option<radar_graph::ongoing::Sighting> {
    later.filter(|seen| seen.coordination > at_launch)
}

/// Renders what the sampled launch blocks looked like.
///
/// **Every row of the band is printed, at zero if that is what was observed.**
/// Their absence is the thing worth noticing: [`radar_graph::BUNDLE_CENTRE`] is
/// a bundler tool's default setting, and when that default moves the detector
/// goes quiet without saying so. A histogram that omitted empty rows would
/// report a moved constant exactly the way it reports a clean population, which
/// is the failure LEARNINGS 5 names — a check that reports absence the same way
/// it reports success is not a check.
///
/// The two rates at the foot are the comparison that makes decay visible at all.
fn render_shapes(dist: &radar_graph::Distribution, unreadable: usize) -> String {
    use core::fmt::Write as _;

    let mut out = String::new();
    let _ = write!(
        out,
        "\nlaunch blocks read: {}, unreadable: {unreadable}\n",
        dist.total()
    );

    if dist.is_empty() {
        // Never "0 at the centre". Nothing was looked at, so nothing is known,
        // and saying otherwise is the exact confusion this function exists to
        // prevent.
        out.push_str(
            "  No launch block was read, so the coordination gate did not run.\n\
             \x20 That is not the same as finding nothing.\n",
        );
        return out;
    }

    // The band always appears; observed values are merged in.
    let mut rows: std::collections::BTreeMap<u64, usize> = radar_graph::BUNDLE_BAND
        .clone()
        .map(|r| (r, dist.count(r)))
        .collect();
    for (recipients, count) in dist.iter() {
        rows.insert(recipients, count);
    }

    let peak = rows.values().copied().max().unwrap_or(0).max(1);
    out.push_str("  recipients  observed\n");
    for (recipients, count) in rows {
        let filled = count * BAR_WIDTH / peak;
        let note = if recipients == radar_graph::BUNDLE_CENTRE {
            "  <- centre, refused"
        } else if radar_graph::BUNDLE_BAND.contains(&recipients) {
            "  <- band"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  {recipients:>10}  {:<width$} {count:>4}{note}",
            "#".repeat(filled),
            width = BAR_WIDTH
        );
    }

    let v = dist.verdicts();
    let centre = dist.centre_rate_bps().unwrap_or(0);
    let band = dist.band_rate_bps().unwrap_or(0);
    let _ = write!(
        out,
        "
  at the centre: {} of {} ({centre} bps)
  in the band  : {} of {} ({band} bps)
",
        v.likely,
        dist.total(),
        v.likely + v.suspected,
        dist.total(),
    );
    // Stated as a different population rather than as a target, because it is
    // one. 0008 measured across *all* launches; everything here has already
    // survived the creator filters, so the shapes should differ and a reader
    // comparing the two numbers directly would be drawing a conclusion about
    // the selection -- the trap LEARNINGS 7, 10 and 11 each record. Only a
    // sustained collapse is evidence about the detector.
    let _ = write!(
        out,
        "  0008 measured {} bps at the centre and {} bps in the band, over all
           launches. These have already survived the creator filters, so a
           different shape is expected; a sustained zero is what would suggest
           the bundler default has moved off {}.
",
        radar_graph::MEASURED_CENTRE_RATE_BPS,
        radar_graph::MEASURED_BAND_RATE_BPS,
        radar_graph::BUNDLE_CENTRE,
    );

    out.push_str(&render_calibration(dist));
    out
}

/// What one base unit was worth when the decision was taken, scaled by
/// [`radar_store::PRICE_SCALE`].
///
/// Read off the **smallest rung** of the realised price ladder the exit probe
/// already built, so it is the price the sizing was derived from and cannot
/// disagree with it. The smallest rung because impact grows with size: the
/// largest rung is what a full exit would realise, and the smallest is the
/// closest thing the ladder holds to an untouched mid.
///
/// Scaled to match [`radar_store::Outcome`]'s price columns exactly, because
/// the only thing this number is for is being compared with them.
fn entry_price_of(exit: &radar_sim::ExitReport) -> Option<u64> {
    let rung = exit
        .curve
        .iter()
        .filter(|q| q.size_tokens > 0 && q.out_lamports > 0)
        .min_by_key(|q| q.size_tokens)?;
    // u128 throughout: lamports times a 1e18 scale leaves u64 immediately, and a
    // wrapped entry price would make every return computed from it nonsense in
    // a way that still looks like a number.
    let scaled = u128::from(rung.out_lamports)
        .checked_mul(radar_store::PRICE_SCALE)?
        .checked_div(u128::from(rung.size_tokens))?;
    u64::try_from(scaled).ok()
}

/// Turns one examined candidate into the row that outlives the run.
///
/// # What is recorded, and what deliberately is not
///
/// Only candidates that reached the **paid tier**. The line is not cost, it is
/// **reproducibility**: a free-tier refusal is a pure function of data already
/// in the store, so it can be re-derived at any time by replaying `disqualify`
/// and the creator record at the same watermark. Recording 41,721 rows an hour
/// to store an answer that is already implied would be storing a derivation.
///
/// A paid-tier decision cannot be re-derived. It rests on a live Jupiter price
/// ladder and a CryptoHouse launch block, neither of which is recorded anywhere
/// and neither of which answers the same way tomorrow. If it is not written
/// down as it happens it is gone — which is the same argument
/// [`radar_research`] makes for digesting inputs rather than copying them, run
/// the other way.
///
/// [`radar_research`]: https://github.com/hey-vera/radar
/// The pass's prevalence table, or `None` if it cannot be trusted.
///
/// One query for the whole pass. Asked per candidate this took 32 seconds
/// against the real endpoint ([research 0012](../../docs/research/0012-recipient-sets-cannot-recur-authorities-can.md)),
/// which at forty candidates an hour is twenty minutes of query time per hour on
/// an endpoint Radar is a guest on.
///
/// A truncated table is `None`, not a short one. Every authority the row cap cut
/// would otherwise read as `Ordinary` — the least alarming answer available —
/// and a decision would record that as though it had been measured. Rule 9.
fn prevalence_table_of<B>(blocks: &B) -> Option<radar_graph::prevalence::Table>
where
    B: LaunchBlockSource,
    B::Error: std::fmt::Display,
{
    match blocks.prevalence_table() {
        Ok(table) if table.is_complete() => {
            println!(
                "launch-block authorities at or above the repeat floor: {}",
                table.len()
            );
            Some(table)
        }
        Ok(_) => {
            eprintln!(
                "  prevalence table hit the row cap and cannot be trusted — recorded as absent"
            );
            None
        }
        Err(e) => {
            eprintln!("  prevalence table unreadable: {e}");
            None
        }
    }
}

fn record_of(
    candidate: &Candidate,
    decision: &Decision,
    strategy: &CreatorEdge,
    verdict: Option<&Verdict>,
    decided_at: radar_types::Slot,
    prevalence: Option<radar_graph::prevalence::Prevalence>,
    launch_shape: Option<radar_graph::LaunchBlockShape>,
) -> radar_store::Decision {
    let proposal = match decision {
        Decision::Propose(p) => Some(p),
        Decision::Pass(_) => None,
    };
    radar_store::Decision {
        mint: candidate.mint,
        creator: candidate.creator,
        decided_at,
        launch_slot: candidate.launch_slot,
        strategy: strategy.name().to_owned(),
        strategy_version: strategy.version().to_owned(),
        conclusion: if proposal.is_some() {
            radar_store::Conclusion::Proposed
        } else {
            radar_store::Conclusion::Passed
        },
        reasons: decision
            .reasons()
            .iter()
            .map(|r| format!("{r:?}"))
            .collect(),
        notional_micro_usd: proposal.map(|p| p.notional.get()),
        exit_capacity_micro_usd: proposal
            .and_then(|p| p.simulated_exit_capacity)
            .map(MicroUsd::get),
        assumed_round_trip_bps: strategy.thresholds.assumed_round_trip_bps,
        // Absent because the source could not answer, never because the launch
        // looked clean. Collapsing those would quietly clear a bundle.
        coordination: candidate.coordination.map(|c| format!("{c:?}")),
        // The numbers the verdict above was computed from, beside the label
        // rather than instead of it (ADR 0012). Saturating rather than
        // truncating: a count past `u32::MAX` is not a launch block, and
        // wrapping it would record a small number for an enormous one.
        launch_recipients: launch_shape.map(|s| u32::try_from(s.recipients).unwrap_or(u32::MAX)),
        launch_transactions: launch_shape
            .map(|s| u32::try_from(s.transactions).unwrap_or(u32::MAX)),
        // Recorded, never acted on. 0012 measured who recurs and not whether
        // recurrence predicts anything about money; this is what makes that
        // second question answerable later.
        authority_prevalence: prevalence.map(|p| p.label().to_owned()),
        entry_price: candidate.exit.as_ref().and_then(entry_price_of),
        kernel_outcome: verdict.map(|v| match v {
            Verdict::Authorised(_) => radar_store::KernelOutcome::Authorised,
            Verdict::Refused { .. } => radar_store::KernelOutcome::Refused,
        }),
        kernel_reasons: match verdict {
            Some(Verdict::Refused { reasons }) => {
                reasons.iter().map(|r| format!("{r:?}")).collect()
            }
            _ => Vec::new(),
        },
        // The same digest a replay compares on, so a recorded decision can be
        // checked against the store later without a separate recording file.
        inputs_digest: radar_research::Digest::of(candidate)
            .map_or_else(|_| String::new(), |d| d.0),
    }
}

/// The verdict on whether the detector is still calibrated.
///
/// Rendered separately from the histogram because a reader should not have to
/// derive it. The histogram says what was seen; this says whether what was seen
/// is consistent with the measurement the threshold rests on.
fn render_calibration(dist: &radar_graph::Distribution) -> String {
    use core::fmt::Write as _;
    let mut out = String::new();
    let _ = match radar_graph::calibration(dist) {
        radar_graph::Calibration::NotEnoughData { observed, needed } => write!(
            out,
            "
  CALIBRATION: {observed} block(s) is too few to say; {needed} more.
               Not a clean bill of health -- a detector nobody has sampled and one
               that works look the same from here.
"
        ),
        radar_graph::Calibration::Consistent { centre_rate_bps } => write!(
            out,
            "
  CALIBRATION: consistent — {centre_rate_bps} bps at the centre.
"
        ),
        radar_graph::Calibration::Silent {
            centre_rate_bps,
            expected_bps,
            observed,
        } => write!(
            out,
            "
  CALIBRATION: SILENT — {centre_rate_bps} bps at the centre over {observed}
               block(s), against {expected_bps} measured. The band has gone quiet, which
               is the direction that fails permissive: a moved bundler default makes
               `is_actionable` stop firing, and nothing raises an error.
"
        ),
        radar_graph::Calibration::Elevated {
            centre_rate_bps,
            expected_bps,
            observed,
        } => write!(
            out,
            "
  CALIBRATION: ELEVATED — {centre_rate_bps} bps at the centre over {observed}
               block(s), against {expected_bps} measured. Either the market moved or this
               sample is not what it is believed to be; both invalidate the threshold.
"
        ),
    };
    out
}

/// Prints what the strategy made of one paid-for candidate.
fn report_one(
    strategy: &CreatorEdge,
    candidate: &Candidate,
    proposals: &mut Vec<radar_risk::Proposal>,
    watermark: radar_types::Slot,
    examined: &mut Vec<(radar_store::Decision, Address)>,
    prevalence: Option<radar_graph::prevalence::Prevalence>,
    launch_shape: Option<radar_graph::LaunchBlockShape>,
) {
    let decision = strategy.consider(candidate);
    // Recorded before the kernel runs, and updated with its verdict afterwards.
    // A proposal the kernel never saw is a different state from one it refused.
    examined.push((
        record_of(
            candidate,
            &decision,
            strategy,
            None,
            watermark,
            prevalence,
            launch_shape,
        ),
        candidate.mint,
    ));
    match decision {
        Decision::Pass(ref reasons) => {
            println!("  {}  passed: {reasons:?}", candidate.mint);
        }
        Decision::Propose(ref proposal) => {
            println!(
                "  {}  PROPOSED ${:.2} (exit capacity ${:.2})",
                candidate.mint,
                price_dollars(proposal.notional),
                proposal.simulated_exit_capacity.map_or(0.0, price_dollars)
            );
            proposals.push((**proposal).clone());
        }
    }
}

/// The exit search this strategy asks for.
///
/// Extracted rather than written inline, and the reason is a real defect rather
/// than tidiness. Inline, deleting the `max_impact_bps` line left
/// `..Search::DEFAULT` supplying 100 -- which is *also* what the shipped
/// strategy configures, so the two agreed by coincidence and no test could tell
/// them apart. Mutation testing found it.
///
/// That coincidence is the dangerous kind: it makes
/// [`capacity_impact_bps`](radar_strategy::creator_edge::Thresholds) decorative.
/// Change the strategy's budget and the search would keep measuring at 1%, while
/// every figure derived from it -- and
/// [research 0022](https://github.com/hey-vera/radar/blob/main/docs/research/0022-capacity-was-a-budget-not-a-ceiling.md)
/// establishes that this one setting determined the capacity figure 0018 built
/// its whole case on -- said otherwise.
fn search_for(strategy: &CreatorEdge) -> radar_sim::Search {
    radar_sim::Search {
        max_impact_bps: strategy.thresholds.capacity_impact_bps,
        ..radar_sim::Search::DEFAULT
    }
}

/// Puts every proposal through the kernel under the shipped policy.
///
/// # Errors
///
/// Returns a message when the inventory cannot be read or cannot be fully
/// accounted for. See [`inventory`]: a verdict reached against a portfolio
/// nobody could read is a verdict about nothing.
fn verdicts(
    proposals: &[radar_risk::Proposal],
    watermark: radar_types::Slot,
    reader: &Reader,
) -> Result<BTreeMap<Address, Verdict>, String> {
    let mut by_mint = BTreeMap::new();
    println!(
        "
{} proposal(s) raised.",
        proposals.len()
    );
    if proposals.is_empty() {
        return Ok(by_mint);
    }

    // The shipped policy. Building the trading lane deploys no capital; only
    // changing this does, and changing it is a decision with an owner.
    //
    // `Policy::SHIPPED` rather than `Policy::CLOSED`, and the difference is not
    // cosmetic: `radar-serve`'s funnel reports the same constant, so opening the
    // policy here cannot leave the interface telling a customer that nothing can
    // trade. It used to read `CLOSED` on its own.
    let policy = Policy::SHIPPED;
    // Rebuilt from what was recorded rather than assumed empty. It *is* empty
    // today, because nothing has ever traded -- but `flat()` would keep saying
    // so on the day something does, and a position limit measured against a
    // portfolio that is always empty is not a limit.
    //
    // The `?` is the point of this slice: an unreadable inventory stops the
    // pass instead of arriving as an empty account. See `inventory`.
    let (portfolio, state) = inventory(reader, watermark)?;
    println!(
        "
portfolio    : {} holding(s), {} unaccounted, realised {}",
        portfolio.holdings().count(),
        portfolio.unaccounted().count(),
        portfolio.results().realised
    );

    println!("\nrisk kernel, under the policy this instance actually holds:");
    for proposal in proposals {
        let verdict = evaluate(proposal, &state, &policy);
        match &verdict {
            Verdict::Authorised(auth) => {
                println!(
                    "  {}  AUTHORISED up to ${:.2}",
                    proposal.mint,
                    price_dollars(auth.max_notional)
                );
            }
            Verdict::Refused { reasons } => {
                // Split rather than dumped. Under the shipped policy all seven
                // reasons are artifacts of a policy of zeros, and printing them
                // as a flat list tells a reader there are seven problems when
                // there is one.
                let (policy_bound, about_this) = radar_risk::partition_refusals(reasons, &policy);
                if about_this.is_empty() {
                    println!(
                        "  {}  refused by policy ({} limit(s)); nothing about the token",
                        proposal.mint,
                        policy_bound.len()
                    );
                } else {
                    println!("  {}  refused: {about_this:?}", proposal.mint);
                }
            }
        }
        by_mint.insert(proposal.mint, verdict);
    }
    // Derived from the policy this run actually judged against. It printed
    // "Radar ships with Policy::CLOSED, which refuses everything" as a literal,
    // so it said so whatever the policy was -- the fourth place that could not
    // stop reassuring, and the one a `repo-conformance` check found rather than
    // a reader.
    if policy.is_closed() {
        println!(
            "
Radar ships with a policy that refuses everything. Nothing above was acted
         on, and nothing will be until that policy is changed deliberately."
        );
    } else {
        println!(
            "
CAPITAL IS ARMED: autonomy {:?}, max position {}. What is above was judged
         against a policy that can authorise. The signer holds its own policy
         (ADR 0008) and clamps against it unconditionally; it is not readable
         from here, so this is not a statement that anything was signed.",
            policy.autonomy, policy.max_position
        );
    }
    Ok(by_mint)
}

/// Micro-USD as dollars, for display only.
#[expect(
    clippy::cast_precision_loss,
    reason = "display only; both halves are far inside f64's exact integer range,               and every calculation upstream of this is integer micro-USD"
)]
fn price_dollars(amount: MicroUsd) -> f64 {
    let whole = amount.get() / 1_000_000;
    let fraction = amount.get() % 1_000_000;
    whole as f64 + fraction as f64 / 1e6
}

/// The paid-tier cap, exposed for the CLI's flag handling.
#[must_use]
pub const fn default_cap() -> usize {
    PAID_TIER_CAP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_stronger_later_reading_is_announced() {
        // Swept rather than sampled, because the interesting case is the
        // boundary: a later block that merely *matches* the launch verdict is
        // not news, and printing it would bury the case the sweep exists for.
        use radar_graph::Coordination;
        use radar_graph::ongoing::{Sighting, When};
        let seen = |c| {
            Some(Sighting {
                shape: radar_graph::LaunchBlockShape {
                    recipients: 6,
                    transactions: 1,
                },
                coordination: c,
                when: When::Later { slots_after: 900 },
            })
        };

        let ladder = [
            Coordination::Unremarkable,
            Coordination::Suspected,
            Coordination::Likely,
        ];
        for (i, &later) in ladder.iter().enumerate() {
            for (j, &launch) in ladder.iter().enumerate() {
                let announced = newly_bundled(seen(later), launch).is_some();
                assert_eq!(
                    announced,
                    i > j,
                    "later {later:?} against launch {launch:?}: only a strictly \
                     stronger later reading is news"
                );
            }
        }

        // An unread sweep announces nothing, which is distinct from a sweep that
        // read clean.
        assert!(newly_bundled(None, Coordination::Unremarkable).is_none());
    }

    /// A launch-block source that answers however a test needs it to.
    struct StubBlocks(Result<radar_graph::prevalence::Table, String>);

    /// The same, plus a shape to return -- for the tests that reach the
    /// coordination read rather than only the prevalence table.
    struct ShapedBlocks(radar_graph::LaunchBlockShape);

    /// A source whose launch block reads clean and whose later blocks do not.
    ///
    /// The year-three case: nothing about the launch was remarkable, and the
    /// token was bundled long afterwards by whoever picked it up.
    struct BundledLater {
        launch: radar_graph::LaunchBlockShape,
        later: Vec<(u64, radar_graph::LaunchBlockShape)>,
    }

    impl LaunchBlockSource for BundledLater {
        type Error = String;

        fn shape_at(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<radar_graph::LaunchBlockShape, Self::Error> {
            Ok(self.launch)
        }

        fn bundle_slots(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<Vec<(u64, radar_graph::LaunchBlockShape)>, Self::Error> {
            Ok(self.later.clone())
        }

        fn authorities_at(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<Vec<String>, Self::Error> {
            Ok(Vec::new())
        }

        fn prevalence_table(&self) -> Result<radar_graph::prevalence::Table, Self::Error> {
            Err("not used by this test".to_owned())
        }
    }

    /// A source whose launch block reads, and whose sweep does not.
    struct SweepFails(radar_graph::LaunchBlockShape);

    impl LaunchBlockSource for SweepFails {
        type Error = String;

        fn shape_at(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<radar_graph::LaunchBlockShape, Self::Error> {
            Ok(self.0)
        }

        fn bundle_slots(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<Vec<(u64, radar_graph::LaunchBlockShape)>, Self::Error> {
            Err("the sweep timed out".to_owned())
        }

        fn authorities_at(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<Vec<String>, Self::Error> {
            Ok(Vec::new())
        }

        fn prevalence_table(&self) -> Result<radar_graph::prevalence::Table, Self::Error> {
            Err("not used by this test".to_owned())
        }
    }

    /// A universe holding one launch, for the sweep tests.
    fn one_launch(mint: Address, slot: radar_types::Slot) -> Universe {
        let mut launches = std::collections::BTreeMap::new();
        launches.insert(
            mint,
            radar_strategy::assemble::LaunchFacts {
                creator: Address::new([9u8; 32]),
                slot,
                observed_at: slot,
            },
        );
        Universe {
            launches,
            creators: std::collections::BTreeMap::new(),
            creators_observed_at: std::collections::BTreeMap::new(),
            as_of: AsOf::at(slot),
        }
    }

    impl LaunchBlockSource for ShapedBlocks {
        type Error = String;

        fn shape_at(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<radar_graph::LaunchBlockShape, Self::Error> {
            Ok(self.0)
        }

        fn authorities_at(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<Vec<String>, Self::Error> {
            Ok(Vec::new())
        }

        fn prevalence_table(&self) -> Result<radar_graph::prevalence::Table, Self::Error> {
            Err("not used by this test".to_owned())
        }
    }

    /// A structure source that reads nothing, which is a real state: rule 9
    /// says an unreadable mint is refused rather than treated as clean.
    struct NoStructures;

    impl Structures for NoStructures {
        fn mint_structure(&self, _: &Address) -> Option<radar_sim::MintStructure> {
            None
        }

        // And no curve either. These tests are about what happens when the
        // chain cannot be read at all, so both halves have to be absent or the
        // fixture describes a state the product does not have.
        fn depth(&self, _: &Address) -> Option<radar_sim::curve::Depth> {
            None
        }
    }

    /// A quoter with nothing to sell at any size.
    ///
    /// `NoRoute` rather than an error: it is the answer, and one of the more
    /// important ones -- a token with no sell route cannot be exited.
    struct NoDepth;

    impl radar_sim::Quoter for NoDepth {
        fn quote_sell(
            &self,
            _: &Address,
            size_tokens: u64,
        ) -> Result<radar_sim::QuotePoint, radar_sim::QuoteError> {
            Err(radar_sim::QuoteError::NoRoute { size_tokens })
        }
    }

    /// A quoter that is never reached by these tests, and says so if it is.
    struct UnusedQuoter;

    impl radar_sim::Quoter for UnusedQuoter {
        fn quote_sell(
            &self,
            _: &Address,
            _: u64,
        ) -> Result<radar_sim::QuotePoint, radar_sim::QuoteError> {
            panic!("the exit must not be probed for a launch already refused on shape")
        }
    }

    impl LaunchBlockSource for StubBlocks {
        type Error = String;

        fn shape_at(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<radar_graph::LaunchBlockShape, Self::Error> {
            Err("not used by these tests".to_owned())
        }

        fn authorities_at(
            &self,
            _: &radar_types::Address,
            _: radar_types::Slot,
        ) -> Result<Vec<String>, Self::Error> {
            Err("not used by these tests".to_owned())
        }

        fn prevalence_table(&self) -> Result<radar_graph::prevalence::Table, Self::Error> {
            self.0.clone()
        }
    }

    #[test]
    fn a_truncated_prevalence_table_is_refused_rather_than_used() {
        // The load-bearing guard. A table that hit the thousand-row cap is
        // missing the authorities the cut removed, and every one of them would
        // then read as `Ordinary` -- the least alarming answer available --
        // recorded on a decision as though it had been measured. Rule 9.
        let capped: Vec<(String, u64)> = (0..radar_graph::prevalence::ROW_CAP)
            .map(|i| (format!("authority-{i:04}"), 50))
            .collect();
        let truncated = radar_graph::prevalence::Table::new(capped);
        assert!(
            !truncated.is_complete(),
            "the fixture is actually truncated"
        );

        assert_eq!(
            prevalence_table_of(&StubBlocks(Ok(truncated))),
            None,
            "a truncated table must not be used"
        );
    }

    #[test]
    fn a_complete_prevalence_table_is_used() {
        // The other direction, and it is not decoration: a guard that refused
        // every table would disable the feature entirely while looking like a
        // safety measure, and nothing else in the pass would say so.
        let table = radar_graph::prevalence::Table::new([("factory".to_owned(), 8)]);
        assert!(table.is_complete());

        let kept = prevalence_table_of(&StubBlocks(Ok(table))).expect("a complete table is used");
        assert_eq!(
            kept.of("factory"),
            Some(radar_graph::prevalence::Prevalence::Repeat)
        );
        assert_eq!(
            kept.of("never-seen"),
            Some(radar_graph::prevalence::Prevalence::Ordinary),
            "below the floor, which is what the query means"
        );
    }

    #[test]
    fn an_unreadable_prevalence_table_records_absence_rather_than_failing_the_pass() {
        // A prevalence the pass could not fetch must not stop it deciding. The
        // decision is still worth recording; what it carries is an absent
        // prevalence, which is the honest value.
        assert_eq!(
            prevalence_table_of(&StubBlocks(Err("endpoint down".to_owned()))),
            None
        );
    }

    /// One launch, recent, by a creator whose measured record qualifies.
    ///
    /// Built to fail on *only* the two things a paid look removes, so that
    /// anything else the free tier raises against it is the defect under test
    /// rather than the fixture.
    fn a_universe_worth_paying_for() -> radar_strategy::Universe {
        let mint = radar_types::Address::new([7u8; 32]);
        let creator = radar_types::Address::new([8u8; 32]);
        let mut universe = radar_strategy::Universe {
            launches: BTreeMap::new(),
            creators: BTreeMap::new(),
            creators_observed_at: BTreeMap::new(),
            as_of: radar_asof::AsOf::at(radar_types::Slot(500_000)),
        };
        universe.launches.insert(
            mint,
            radar_strategy::assemble::LaunchFacts {
                creator,
                slot: radar_types::Slot(499_000),
                observed_at: radar_types::Slot(499_000),
            },
        );
        universe.creators.insert(
            creator,
            radar_strategy::CreatorRecord {
                launches: 20,
                measured: 20,
                stillborn: 10,
                graduated: 3,
                graduated_organic: 3,
                launches_per_day: None,
            },
        );
        universe
            .creators_observed_at
            .insert(creator, radar_types::Slot(499_000));
        universe
    }

    #[test]
    fn the_free_tier_names_the_venue_the_paid_tier_will_price_on() {
        // The gate the whole command hangs on. A free-tier candidate is built
        // with no exit report, so it must fail on `NoExitSimulated` and
        // `NoPrice` and nothing else -- those two are what the paid look
        // removes. Leaving `market` absent adds `VenueUnknown`, which no exit
        // report can remove, so the gate closes on every mint, `run` returns at
        // "0 candidate(s) fail on nothing a paid look cannot resolve", and no
        // proposal or decision is ever produced. The workspace suite stayed
        // green throughout, because nothing exercised this.
        let universe = a_universe_worth_paying_for();
        let recent = universe.recent(6_000);
        assert_eq!(recent.len(), 1, "the fixture is not recent enough to run");

        let free = free_tier(&universe, &CreatorEdge::default(), &recent, Pricing::Curve);
        assert_eq!(
            free.worth_paying_for, recent,
            "the paid tier would never be reached; the free tier said: {:?}",
            free.tally
        );
        assert_eq!(
            free.tally.get(&PassReason::VenueUnknown),
            None,
            "the instrument is chosen, so the venue is knowable here"
        );
    }

    #[test]
    fn a_route_priced_exit_is_not_worth_paying_for() {
        // The other half, and rule 9 rather than a gap: `--quoter jupiter`
        // measures depth across whatever pools the aggregator picked, which
        // belongs to no single `Market`. The strategy refuses such a capacity,
        // so an exit report would change no answer and the call is not made.
        let universe = a_universe_worth_paying_for();
        let recent = universe.recent(6_000);

        let free = free_tier(
            &universe,
            &CreatorEdge::default(),
            &recent,
            Pricing::Jupiter,
        );
        assert!(
            free.worth_paying_for.is_empty(),
            "paid for an exit the strategy will refuse regardless"
        );
        assert_eq!(free.tally.get(&PassReason::VenueUnknown), Some(&1));
    }

    fn a_candidate() -> Candidate {
        Candidate {
            mint: radar_types::Address::new([7u8; 32]),
            creator: radar_types::Address::new([8u8; 32]),
            launch_slot: radar_types::Slot(1_000),
            as_of: radar_asof::AsOf::at(radar_types::Slot(10_000)),
            exit: None,
            market: Some(radar_types::Market::PUMP_FUN_BONDING_CURVE),
            creator_record: radar_strategy::CreatorRecord::default(),
            coordination: None,
            sol_price_micro_usd: None,
            token_observed_at: radar_types::Slot(9_900),
            creator_observed_at: radar_types::Slot(9_900),
        }
    }

    fn quote(size_tokens: u64, out_lamports: u64) -> radar_sim::exit::QuotePoint {
        radar_sim::exit::QuotePoint {
            size_tokens,
            out_lamports,
            impact_bps: 20,
        }
    }

    fn report(curve: Vec<radar_sim::exit::QuotePoint>) -> radar_sim::ExitReport {
        radar_sim::ExitReport {
            mint: radar_types::Address::new([7u8; 32]),
            structure: None,
            curve,
            no_route_at: Vec::new(),
            structural_threats: Vec::new(),
            can_be_stopped: false,
            can_be_diluted: false,
            confidence: radar_sim::exit::Confidence::Measured,
        }
    }

    #[test]
    fn the_entry_price_comes_from_the_smallest_rung() {
        // Impact grows with size, so the largest rung is what a full exit would
        // realise and the smallest is the closest the ladder holds to an
        // untouched price. Taking the wrong end would systematically understate
        // the entry and overstate every return measured from it.
        let exit = report(vec![
            quote(1_000_000_000_000, 20_000_000),
            quote(1_000_000_000, 30_000),
            quote(10_000_000_000_000, 150_000_000),
        ]);
        // 30_000 lamports for 1e9 base units = 3e-5 lamports each, times 1e18.
        assert_eq!(entry_price_of(&exit), Some(30_000_000_000_000));
    }

    #[test]
    fn a_rung_with_no_route_does_not_become_a_price_of_zero() {
        // A zero-output rung is "no route at this size", not "worthless". Taking
        // it would record an entry price of zero, which `return_bps` then
        // refuses -- so the decision would silently become unscoreable.
        let exit = report(vec![quote(1_000_000_000, 0), quote(2_000_000_000, 60_000)]);
        assert_eq!(entry_price_of(&exit), Some(30_000_000_000_000));
    }

    #[test]
    fn an_empty_curve_has_no_entry_price() {
        // Absent, not zero. A token with no route was never priced, and a
        // decision about it cannot be scored later.
        assert_eq!(entry_price_of(&report(Vec::new())), None);
        assert_eq!(entry_price_of(&report(vec![quote(0, 0)])), None);
    }

    #[test]
    fn the_entry_price_is_on_the_same_scale_as_a_recorded_outcome() {
        // The only purpose of this number is to be compared with the outcome
        // table's prices. A scale mismatch would make every return wrong by
        // eighteen orders of magnitude while still looking like a number --
        // which is the shape LEARNINGS 12 and 14 both record.
        //
        // One lamport per base unit must land exactly on PRICE_SCALE.
        let exit = report(vec![quote(1_000, 1_000)]);
        assert_eq!(
            u128::from(entry_price_of(&exit).expect("priced")),
            radar_store::PRICE_SCALE
        );
    }

    #[test]
    fn a_price_too_large_to_represent_is_refused_rather_than_wrapped() {
        // A wrapped entry price produces returns that are confidently wrong.
        let exit = report(vec![quote(1, u64::MAX)]);
        assert_eq!(entry_price_of(&exit), None);
    }

    #[test]
    fn a_refusal_records_the_reasons_the_kernel_gave() {
        // Deleting this arm leaves every refusal with an empty reason list,
        // which reads as "refused for no stated reason" -- and the reasons are
        // the entire point of recording a refusal. A mutant doing exactly that
        // survived the first version of these tests.
        let strategy = CreatorEdge::default();
        let candidate = a_candidate();
        let decision = strategy.consider(&candidate);
        let verdict = Verdict::Refused {
            reasons: vec![
                radar_risk::Refusal::NoAutonomy,
                radar_risk::Refusal::InputsTooStale,
            ],
        };

        let record = record_of(
            &candidate,
            &decision,
            &strategy,
            Some(&verdict),
            radar_types::Slot(10_000),
            None,
            None,
        );
        assert_eq!(
            record.kernel_reasons,
            vec!["NoAutonomy".to_owned(), "InputsTooStale".to_owned()]
        );
        assert_eq!(
            record.kernel_outcome,
            Some(radar_store::KernelOutcome::Refused)
        );
    }

    #[test]
    fn a_decision_the_kernel_never_saw_carries_no_verdict_and_no_reasons() {
        // Absent is not a refusal. A proposal that never reached the kernel is a
        // gap in the pipeline; recording it as refused would hide that.
        let strategy = CreatorEdge::default();
        let candidate = a_candidate();
        let decision = strategy.consider(&candidate);

        let record = record_of(
            &candidate,
            &decision,
            &strategy,
            None,
            radar_types::Slot(10_000),
            None,
            None,
        );
        assert_eq!(record.kernel_outcome, None);
        assert!(record.kernel_reasons.is_empty());
    }

    #[test]
    fn a_record_carries_the_watermark_and_the_cost_the_rule_assumed() {
        // Both are what makes a decision comparable later. The assumed cost
        // moved by a factor of four on 2026-08-25, and a decision either side of
        // that was judged against a different bar -- comparing them without
        // knowing which would be comparing two rules.
        let strategy = CreatorEdge::default();
        let candidate = a_candidate();
        let decision = strategy.consider(&candidate);
        let record = record_of(
            &candidate,
            &decision,
            &strategy,
            None,
            radar_types::Slot(441_734_987),
            None,
            None,
        );

        // ADR 0012's first commitment: the count travels with the verdict.
        // `None` here because this call passes no shape, and the next test is
        // the one that proves a shape is recorded rather than dropped.
        assert_eq!(record.launch_recipients, None);
        assert_eq!(record.launch_transactions, None);
    }

    #[test]
    fn the_launch_block_count_is_recorded_beside_the_verdict() {
        // ADR 0012, and the whole reason it exists. `Decision.coordination`
        // kept the label and threw the number away, so a threshold fitted to
        // that number could only be re-fitted by scanning the chain again --
        // which is why the last drift went unnoticed for nine days and was
        // found by hand.
        //
        // Recording it is what makes the next re-derivation a query.
        let strategy = CreatorEdge::default();
        let candidate = a_candidate();
        let decision = strategy.consider(&candidate);

        let shape = radar_graph::LaunchBlockShape {
            recipients: 11,
            transactions: 4,
        };
        let record = record_of(
            &candidate,
            &decision,
            &strategy,
            None,
            radar_types::Slot(441_734_987),
            None,
            Some(shape),
        );

        assert_eq!(record.launch_recipients, Some(11));
        assert_eq!(record.launch_transactions, Some(4));

        // Both halves, because a block with six recipients across six
        // transactions and one with six across one are different arrangements,
        // and a threshold derived from recipients alone cannot tell them apart.
        assert_ne!(record.launch_recipients, record.launch_transactions);

        assert_eq!(record.decided_at, radar_types::Slot(441_734_987));
        assert_eq!(record.launch_slot, candidate.launch_slot);
        assert_eq!(
            record.assumed_round_trip_bps,
            strategy.thresholds.assumed_round_trip_bps
        );
        assert_eq!(record.strategy, "creator_edge");
        assert!(
            !record.inputs_digest.is_empty(),
            "the digest is what lets a recorded decision be checked against the store"
        );
    }

    #[test]
    fn an_unread_launch_block_records_as_absent_not_as_clean() {
        // The distinction the whole coordination gate rests on: a source that
        // could not answer must never look like a launch that looked fine.
        let strategy = CreatorEdge::default();
        let mut candidate = a_candidate();
        candidate.coordination = None;
        let unread = record_of(
            &candidate,
            &strategy.consider(&candidate),
            &strategy,
            None,
            radar_types::Slot(10_000),
            None,
            None,
        );
        assert_eq!(unread.coordination, None);

        let clean = candidate.with_coordination(radar_graph::Coordination::Unremarkable);
        let looked = record_of(
            &clean,
            &strategy.consider(&clean),
            &strategy,
            None,
            radar_types::Slot(10_000),
            None,
            None,
        );
        assert_eq!(looked.coordination, Some("Unremarkable".to_owned()));
        assert_ne!(unread.coordination, looked.coordination);
    }

    #[test]
    fn dollars_render_from_integers() {
        assert!((price_dollars(MicroUsd::from_dollars(12.34)) - 12.34).abs() < 1e-9);
        assert!((price_dollars(MicroUsd::ZERO) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn a_huge_amount_still_renders_rather_than_saturating_into_nonsense() {
        // Micro-USD exceeds f64's exact integer range, which is why the split
        // exists. The whole-dollar half stays exact well past any real balance.
        let large = MicroUsd(u64::MAX);
        assert!(price_dollars(large) > 1.0e12);
    }

    fn dist(recipients: &[u64]) -> radar_graph::Distribution {
        let mut d = radar_graph::Distribution::new();
        for r in recipients {
            d.observe(radar_graph::LaunchBlockShape {
                recipients: *r,
                transactions: 4,
            });
        }
        d
    }

    #[test]
    fn nothing_read_does_not_render_as_nothing_found() {
        // The whole reason this function exists. Before it, `read: 25,
        // unreadable: 0` was printed for both a healthy gate on a clean sample
        // and a gate whose constant had moved, and the two were byte-identical.
        let unread = render_shapes(&dist(&[]), 0);
        let clean = render_shapes(&dist(&[1, 2, 2, 3, 3, 3]), 0);

        assert_ne!(unread, clean);
        assert!(
            unread.contains("did not run"),
            "an unread sample must say so: {unread}"
        );
        assert!(
            !unread.contains("at the centre"),
            "no sample means no rate to report: {unread}"
        );
        assert!(
            clean.contains("at the centre: 0 of 6"),
            "a clean sample has a rate and it is zero: {clean}"
        );
    }

    #[test]
    fn the_band_is_printed_even_when_empty() {
        // A moved bundler default shows up as holes where the band used to be.
        // Omitting zero rows would hide exactly that.
        let out = render_shapes(&dist(&[1, 1, 2, 2, 3]), 0);
        for recipients in radar_graph::BUNDLE_BAND {
            assert!(
                out.lines()
                    .any(|l| l.trim_start().starts_with(&format!("{recipients} "))),
                "band row {recipients} missing from:\n{out}"
            );
        }
        assert!(out.contains("<- centre, refused"), "{out}");
        assert!(out.contains("<- band"), "{out}");
    }

    #[test]
    fn the_measured_baseline_is_shown_beside_the_observed_rate() {
        // The comparison is the decay check. A rate with nothing to compare it
        // against is a number nobody can act on.
        let out = render_shapes(&dist(&[6, 1, 1, 1, 1, 1, 1, 1, 1, 1]), 0);
        assert!(out.contains("1000 bps"), "observed rate missing: {out}");
        assert!(
            out.contains(&format!(
                "{} bps at the centre",
                radar_graph::MEASURED_CENTRE_RATE_BPS
            )),
            "measured baseline missing: {out}"
        );
        assert!(
            out.contains("survived the creator filters"),
            "the baseline is a different population and the output must say so,              or a reader compares a selected sample against an unselected one: {out}"
        );
    }

    #[test]
    fn an_unreadable_block_is_reported_separately_from_a_read_one() {
        // A source that failed leaves the verdict absent and the gate silently
        // off. It must never be folded into the read count.
        let out = render_shapes(&dist(&[1, 2, 3]), 7);
        assert!(out.contains("read: 3, unreadable: 7"), "{out}");
    }

    #[test]
    fn the_centre_marker_sits_on_the_centre_row_and_nowhere_else() {
        // Asserting only that both labels appear somewhere is not evidence: with
        // the comparison inverted, every ordinary row gets "centre" and the
        // centre row gets "band", and both strings are still present. A mutant
        // doing exactly that survived the first version of this test.
        let out = render_shapes(&dist(&[1, 5, 6, 7, 40]), 0);
        let mut rows_checked = 0;
        // Only the histogram block. Scanning the whole output for "a line
        // starting with a number" caught the footer, because `"0008".parse()`
        // is `Ok(8)` -- a reminder that a loose heuristic in a test is a way to
        // assert something other than what was meant.
        for line in out
            .lines()
            .skip_while(|l| !l.contains("recipients  observed"))
            .skip(1)
            .take_while(|l| !l.trim().is_empty())
        {
            let Some(first) = line.split_whitespace().next() else {
                continue;
            };
            let recipients: u64 = first
                .parse()
                .unwrap_or_else(|_| panic!("histogram row is not numeric: {line}"));
            rows_checked += 1;
            assert_eq!(
                line.contains("<- centre"),
                recipients == radar_graph::BUNDLE_CENTRE,
                "row {recipients}: {line}"
            );
            assert_eq!(
                line.contains("<- band"),
                radar_graph::BUNDLE_BAND.contains(&recipients)
                    && recipients != radar_graph::BUNDLE_CENTRE,
                "row {recipients}: {line}"
            );
        }
        assert_eq!(rows_checked, 5, "expected one row per distinct count");
    }

    #[test]
    fn the_bar_is_proportional_to_the_count() {
        // Not merely "within the width". A bar that is always empty, or that
        // shrinks as the count grows, also fits inside the width and tells the
        // reader nothing -- two arithmetic mutants survived a width-only
        // assertion.
        let out = render_shapes(&dist(&[1, 1, 1, 1, 2, 2, 3]), 0);
        let bar_of = |r: u64| -> usize {
            let want = r.to_string();
            out.lines()
                .find(|l| l.split_whitespace().next() == Some(want.as_str()))
                .map_or_else(
                    || panic!("no row for {r} in\n{out}"),
                    |l| l.matches('#').count(),
                )
        };

        assert_eq!(
            bar_of(1),
            BAR_WIDTH,
            "the most frequent count fills the width"
        );
        assert!(
            bar_of(1) > bar_of(2),
            "four observations must draw wider than two"
        );
        assert!(bar_of(2) > bar_of(3), "two must draw wider than one");
        assert_eq!(bar_of(5), 0, "an unobserved band row draws nothing");

        // And still never overflows, including where the peak is one.
        for sample in [vec![1], vec![1; 500], vec![1, 6], (0..40).collect()] {
            for line in render_shapes(&dist(&sample), 0).lines() {
                assert!(
                    line.matches('#').count() <= BAR_WIDTH,
                    "bar overflowed on {sample:?}: {line}"
                );
            }
        }
    }

    #[test]
    fn the_paid_tier_is_capped() {
        // A first run against a large store must not make tens of thousands of
        // requests to somebody's free endpoint.
        assert!(default_cap() > 0 && default_cap() <= 100);
    }

    #[test]
    fn every_proposal_gets_a_verdict_under_the_shipped_policy() {
        // `verdicts` returns the map the caller records from, and nothing
        // asserted it was populated -- so replacing the whole body with an empty
        // map passed the suite. That is the shape LEARNINGS 10 records: a
        // function whose output nothing checks is a function that can stop
        // working silently.
        //
        // The store is empty, which is fine and is the point: the kernel is pure
        // (rule 2), so a verdict does not depend on there being history. What is
        // asserted is that every proposal handed in comes back out with one.
        let store = std::env::temp_dir().join("radar-verdicts-test");
        let reader = Reader::open(&store);

        let mints = [Address::new([1u8; 32]), Address::new([2u8; 32])];
        let proposals: Vec<radar_risk::Proposal> = mints
            .iter()
            .map(|mint| radar_risk::Proposal {
                mint: *mint,
                // The market and quote this lane trades: pre-graduation
                // pump.fun, direct to the bonding curve, settled in lamports.
                market: radar_types::Market::PUMP_FUN_BONDING_CURVE,
                quote: radar_types::Asset::Sol,
                creator: Address::new([9u8; 32]),
                action: radar_risk::Action::Buy,
                notional: radar_types::MicroUsd(5_000_000),
                estimated_round_trip_cost: radar_types::MicroUsd(100_000),
                oldest_input_slot: radar_types::Slot(999),
                simulated_exit_capacity: Some(radar_types::MicroUsd(50_000_000)),
            })
            .collect();

        let by_mint = verdicts(&proposals, radar_types::Slot(1_000), &reader)
            .expect("a readable, empty store");

        assert_eq!(by_mint.len(), proposals.len(), "one verdict per proposal");
        for mint in &mints {
            let verdict = by_mint.get(mint).expect("a verdict for every mint");
            // Under the shipped policy every one of them is refused, and saying
            // so here is what keeps this test from passing on the day the policy
            // is opened without anyone noticing this file.
            assert!(
                matches!(verdict, Verdict::Refused { .. }),
                "the shipped policy authorises nothing: {verdict:?}"
            );
        }
    }

    #[test]
    fn the_program_this_lane_trades_is_the_one_it_decodes() {
        // Two copies of one address. `radar-decode` owns `PROGRAM_ID` and cannot
        // be reached from `radar-types`, because the decoder depends on the
        // vocabulary and not the other way round -- so the constant is written
        // twice and this is the only crate that sees both.
        //
        // If they ever disagree, every proposal names a venue the decoder does
        // not recognise while both crates' own tests pass. Retype either
        // constant: this fails.
        assert_eq!(
            radar_types::Market::PUMP_FUN_PROGRAM,
            radar_decode::pumpfun::PROGRAM_ID,
            "a proposal must name the program the decoder recognises"
        );
    }

    #[test]
    fn the_exit_search_carries_the_strategys_own_impact_budget() {
        // Not `Search::DEFAULT`'s. The two happen to agree at 100 bps today, so
        // an inline struct literal that dropped the field passed every test --
        // which would make the strategy's budget decorative the moment anyone
        // changed it.
        //
        // research 0022's finding is that this single number determined the
        // capacity figure 0018 built its case on, so plumbing that silently
        // ignored it would be expensive rather than cosmetic.
        let mut strategy = CreatorEdge::default();
        strategy.thresholds.capacity_impact_bps = 850;
        assert_eq!(search_for(&strategy).max_impact_bps, 850);
        assert_ne!(
            search_for(&strategy).max_impact_bps,
            radar_sim::Search::DEFAULT.max_impact_bps,
            "the fixture must actually differ from the default"
        );

        // And the rest of the search still comes from the default, which is what
        // the struct-update syntax is there for.
        assert_eq!(
            search_for(&strategy).max_quotes,
            radar_sim::Search::DEFAULT.max_quotes
        );
    }

    #[test]
    fn a_pass_examines_the_candidates_it_is_given() {
        // The property LEARNINGS 10 is about. `paid_tier` could be replaced
        // wholesale with "return no proposals" and every test still passed --
        // which is exactly the shape of the 2026-08-25 run where 41,254
        // candidates produced zero proposals and zero read as a fact about the
        // market rather than about the probe.
        //
        // Asserting the *examinations* rather than the proposals is deliberate:
        // a candidate refused on shape is still a decision, and it is the
        // strongest one Radar makes. A function that examined nothing would
        // record nothing, which is the failure worth catching.
        let creator = Address::new([9u8; 32]);
        let mint = Address::new([1u8; 32]);
        let slot = radar_types::Slot(1_000);

        let mut launches = std::collections::BTreeMap::new();
        launches.insert(
            mint,
            radar_strategy::assemble::LaunchFacts {
                creator,
                slot,
                observed_at: slot,
            },
        );
        let universe = Universe {
            launches,
            creators: std::collections::BTreeMap::new(),
            creators_observed_at: std::collections::BTreeMap::new(),
            as_of: AsOf::at(slot),
        };

        // Six recipients in the launch block is `BUNDLE_CENTRE` -- the shape
        // research 0008 measures as arranged in advance, and the one verdict
        // that is actionable. So this candidate is refused before any exit is
        // probed, which `UnusedQuoter` asserts by panicking if it is not.
        let blocks = ShapedBlocks(radar_graph::LaunchBlockShape {
            recipients: radar_graph::BUNDLE_CENTRE,
            transactions: 6,
        });

        let mints = [mint];
        let mut examined = Vec::new();
        let pass = paid_tier(
            &universe,
            &CreatorEdge::default(),
            mints.iter(),
            &Sources {
                blocks: &blocks,
                structures: &NoStructures,
                quoter: &UnusedQuoter,
                pricing: Pricing::Jupiter,
            },
            radar_types::MicroUsd::from_dollars(200.0),
            slot,
            &mut examined,
        );

        assert_eq!(
            examined.len(),
            1,
            "the candidate must be recorded even though it was refused"
        );
        assert_eq!(examined[0].1, mint);
        assert!(
            pass.proposals.is_empty(),
            "an arranged launch must not become a proposal"
        );
        // The count is asserted, not just the behaviour. As a local feeding a
        // `println!` it could be incremented by the wrong operator and nothing
        // could tell.
        assert_eq!(pass.refused_on_shape, 1);
        assert_eq!(
            pass.look_failed, 0,
            "the block was read, and it was arranged"
        );
    }

    #[test]
    fn a_pass_reports_only_the_declines_it_actually_had() {
        // The two thresholds. Inline they were `if n > 0` around a `println!`,
        // which could become `<`, `>=` or `==` with nothing noticing -- so a
        // pass that refused nothing would announce refusals, or one that refused
        // plenty would stay quiet.
        assert_eq!(render_pass_notes(&Pass::default()), "", "nothing to report");

        let refused = Pass {
            refused_on_shape: 1,
            ..Pass::default()
        };
        let notes = render_pass_notes(&refused);
        assert!(notes.contains("1 candidate(s) refused on shape"), "{notes}");
        assert!(
            !notes.contains("could not be read"),
            "nothing failed to read: {notes}"
        );

        let unread = Pass {
            look_failed: 2,
            ..Pass::default()
        };
        let notes = render_pass_notes(&unread);
        assert!(notes.contains("could not be read"), "{notes}");
        assert!(
            !notes.contains("refused on shape"),
            "nothing was refused on shape: {notes}"
        );
    }

    #[test]
    fn a_launch_block_that_cannot_be_read_is_counted_apart_from_a_clean_one() {
        // Rule 9, and the reason `look_failed` exists at all: a fetch that fails
        // leaves the verdict absent, the strategy correctly declines to refuse
        // on an absence, and the coordination gate is then *silently off*.
        // Without this count a broken source and a clean population produce
        // identical output.
        let creator = Address::new([9u8; 32]);
        let mint = Address::new([2u8; 32]);
        let slot = radar_types::Slot(1_000);

        let mut launches = std::collections::BTreeMap::new();
        launches.insert(
            mint,
            radar_strategy::assemble::LaunchFacts {
                creator,
                slot,
                observed_at: slot,
            },
        );
        let universe = Universe {
            launches,
            creators: std::collections::BTreeMap::new(),
            creators_observed_at: std::collections::BTreeMap::new(),
            as_of: AsOf::at(slot),
        };

        // `StubBlocks::shape_at` refuses, which is the case under test.
        let blocks = StubBlocks(Err("no table either".to_owned()));

        let mints = [mint];
        let mut examined = Vec::new();
        let pass = paid_tier(
            &universe,
            &CreatorEdge::default(),
            mints.iter(),
            &Sources {
                blocks: &blocks,
                structures: &NoStructures,
                quoter: &NoDepth,
                pricing: Pricing::Jupiter,
            },
            radar_types::MicroUsd::from_dollars(200.0),
            slot,
            &mut examined,
        );

        assert_eq!(pass.look_failed, 1, "the unreadable block must be counted");
        assert_eq!(
            pass.refused_on_shape, 0,
            "an absent verdict is not a refusal"
        );
        assert!(
            pass.proposals.is_empty(),
            "nothing with no measurable exit should be proposed"
        );
    }

    #[test]
    fn a_token_bundled_after_launch_is_refused_even_though_its_launch_was_clean() {
        // The whole point of wiring the ongoing detector. Before this the launch
        // block was read once and the verdict stood forever -- so a token that
        // sat dormant and was bundled years later stayed labelled clean on
        // precisely the day it mattered.
        //
        // `UnusedQuoter` carries the second half of the claim: it panics if the
        // exit is probed, so this asserts the candidate is refused *before*
        // anything is paid for.
        let mint = Address::new([4u8; 32]);
        let slot = radar_types::Slot(1_000);
        let blocks = BundledLater {
            // Three recipients is unremarkable, and `assess` reads it as clean.
            launch: radar_graph::LaunchBlockShape {
                recipients: 3,
                transactions: 3,
            },
            // Six, five million slots later: the bundle centre.
            later: vec![(
                5_000_000,
                radar_graph::LaunchBlockShape {
                    recipients: radar_graph::BUNDLE_CENTRE,
                    transactions: 6,
                },
            )],
        };

        let mints = [mint];
        let mut examined = Vec::new();
        let pass = paid_tier(
            &one_launch(mint, slot),
            &CreatorEdge::default(),
            mints.iter(),
            &Sources {
                blocks: &blocks,
                structures: &NoStructures,
                quoter: &UnusedQuoter,
                pricing: Pricing::Jupiter,
            },
            radar_types::MicroUsd::from_dollars(200.0),
            slot,
            &mut examined,
        );

        assert_eq!(
            pass.refused_on_shape, 1,
            "a bundle after launch must refuse the candidate"
        );
        assert_eq!(examined.len(), 1, "and the refusal is still recorded");
        assert!(pass.proposals.is_empty());
    }

    #[test]
    fn a_coordination_refusal_records_only_what_actually_refused_it() {
        // The recorded decision is the research artefact, so the reasons on it
        // have to be the ones that stopped this candidate. This branch refuses
        // before the exit is probed, but the instrument is already chosen --
        // leaving the venue unnamed here stored a spurious `VenueUnknown`
        // beside `LaunchLooksCoordinated`, and a later reader counting bundles
        // could not tell which finding did the work.
        let mint = Address::new([4u8; 32]);
        let slot = radar_types::Slot(1_000);
        let blocks = BundledLater {
            launch: radar_graph::LaunchBlockShape {
                recipients: radar_graph::BUNDLE_CENTRE,
                transactions: 6,
            },
            later: Vec::new(),
        };

        let mints = [mint];
        let mut examined = Vec::new();
        paid_tier(
            &one_launch(mint, slot),
            &CreatorEdge::default(),
            mints.iter(),
            &Sources {
                blocks: &blocks,
                structures: &NoStructures,
                quoter: &UnusedQuoter,
                // The curve: an instrument that *can* name its venue, which is
                // the only case where the omission is visible.
                pricing: Pricing::Curve,
            },
            radar_types::MicroUsd::from_dollars(200.0),
            slot,
            &mut examined,
        );

        let (record, _) = examined.first().expect("the refusal is recorded");
        assert!(
            record.reasons.iter().any(|r| r == "LaunchLooksCoordinated"),
            "recorded the wrong refusal: {:?}",
            record.reasons
        );
        assert!(
            !record.reasons.iter().any(|r| r == "VenueUnknown"),
            "recorded a venue as unknown while the instrument names one: {:?}",
            record.reasons
        );
    }

    #[test]
    fn a_sweep_that_fails_does_not_read_as_an_unread_launch_block() {
        // The two failures mean different things and are counted apart. An
        // unreadable *launch* block leaves the coordination gate silently off,
        // which is what `look_failed` exists to surface. An unreadable *sweep*
        // leaves the launch verdict intact, so the gate is still doing its
        // original job -- and counting it there would report a working gate as a
        // broken one.
        let mint = Address::new([5u8; 32]);
        let slot = radar_types::Slot(1_000);
        // The launch itself is bundled, so the candidate is refused on that
        // alone and the exit is never probed.
        let blocks = SweepFails(radar_graph::LaunchBlockShape {
            recipients: radar_graph::BUNDLE_CENTRE,
            transactions: 6,
        });

        let mints = [mint];
        let mut examined = Vec::new();
        let pass = paid_tier(
            &one_launch(mint, slot),
            &CreatorEdge::default(),
            mints.iter(),
            &Sources {
                blocks: &blocks,
                structures: &NoStructures,
                quoter: &UnusedQuoter,
                pricing: Pricing::Jupiter,
            },
            radar_types::MicroUsd::from_dollars(200.0),
            slot,
            &mut examined,
        );

        assert_eq!(pass.refused_on_shape, 1, "the launch block still decided");
        assert_eq!(
            pass.look_failed, 0,
            "a failed sweep is not an unread launch block"
        );
    }

    #[test]
    fn the_quoter_flag_names_an_instrument_and_refuses_a_typo() {
        // LEARNINGS 18: two instruments compared as if they were one. A typo
        // that silently picked the aggregator while the operator believed they
        // had the curve would produce exactly that comparison, in a decision
        // record nobody could tell apart afterwards.
        assert_eq!(Pricing::parse("curve"), Ok(Pricing::Curve));
        assert_eq!(Pricing::parse("jupiter"), Ok(Pricing::Jupiter));
        assert!(Pricing::parse("Curve").is_err(), "case is not guessed at");
        assert!(Pricing::parse("").is_err());
        let why = Pricing::parse("jupitor").expect_err("a typo");
        assert!(why.contains("jupitor"), "the message must name it: {why}");
    }

    #[test]
    fn the_default_is_the_one_that_spends_nothing() {
        // The hourly cron runs with no `--quoter`. The aggregator costs eight
        // calls a candidate against a lane `Policy::CLOSED` will not let trade,
        // so the default has to be the curve or this change does nothing where
        // it matters.
        assert_eq!(Pricing::default(), Pricing::Curve);
    }

    #[test]
    fn each_instrument_says_which_it_is_and_what_it_costs() {
        // Printed above the pass. A capacity figure whose instrument is not on
        // the same screen is a figure somebody will compare with last week's.
        assert!(Pricing::Curve.label().contains("curve"));
        assert!(Pricing::Curve.label().contains("account reads"));
        assert!(Pricing::Jupiter.label().contains("jupiter"));
        assert!(Pricing::Jupiter.label().contains("eight"));
    }

    #[test]
    fn a_curve_that_cannot_be_read_is_not_exitable_rather_than_unbounded() {
        // Rule 9, at the point the instrument changed. `NoStructures` reads
        // neither the mint nor the curve, so the report has an empty ladder --
        // which must mean "cannot exit", never "no limit found". The two are
        // opposite facts and only one of them lets a position be opened.
        let report = radar_sim::ExitReport::build(Address::new([9u8; 32]), None, Vec::new());
        assert!(!report.is_exitable());
        assert_eq!(report.capacity_lamports(10_000), None);
    }

    /// A store whose positions directory holds one file the parquet reader
    /// cannot open — what a truncated or half-written partition looks like.
    ///
    /// `Reader::files` selects on the `.parquet` extension alone, so this is
    /// reached the same way a real corrupt partition would be.
    fn store_with_an_unreadable_positions_file() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let positions = dir.path().join("positions");
        std::fs::create_dir_all(&positions).expect("mkdir");
        std::fs::write(
            positions.join("00000000000000010000-0.parquet"),
            b"not parquet",
        )
        .expect("write");
        dir
    }

    #[test]
    fn an_unreadable_inventory_refuses_rather_than_reporting_an_empty_account() {
        // The named test, and the defect this slice exists to close. The call
        // site ended in `unwrap_or_default()`: a read that failed came back as
        // an empty `Vec<Position>`, the kernel was handed a portfolio with zero
        // deployed, and every exposure limit was measured against a number that
        // was a fact about the disk rather than about the account.
        //
        // Re-apply the bug -- put `.unwrap_or_default()` back in `inventory` in
        // place of the `?` -- and this returns a healthy empty portfolio.
        let dir = store_with_an_unreadable_positions_file();
        let refused = inventory(&Reader::open(dir.path()), radar_types::Slot(20_000))
            .expect_err("an unreadable inventory must refuse");
        assert!(
            refused.contains("positions unreadable"),
            "and says what it could not read: {refused}"
        );
    }

    #[test]
    fn the_pass_stops_on_an_unreadable_inventory_instead_of_judging_against_it() {
        // The refusal has to reach the caller, not merely exist. `verdicts` is
        // where the kernel is invoked, so this is the boundary that matters:
        // one proposal, one unreadable store, and no verdict is reached.
        let dir = store_with_an_unreadable_positions_file();
        let proposal = radar_risk::Proposal {
            mint: Address::new([1u8; 32]),
            market: radar_types::Market::PUMP_FUN_BONDING_CURVE,
            quote: radar_types::Asset::Sol,
            creator: Address::new([2u8; 32]),
            action: radar_risk::Action::Buy,
            notional: MicroUsd(5_000_000),
            estimated_round_trip_cost: MicroUsd(100_000),
            oldest_input_slot: radar_types::Slot(999),
            simulated_exit_capacity: Some(MicroUsd(50_000_000)),
        };
        let outcome = verdicts(
            std::slice::from_ref(&proposal),
            radar_types::Slot(20_000),
            &Reader::open(dir.path()),
        );
        assert!(
            outcome.is_err(),
            "a verdict against an unreadable portfolio is a verdict about nothing"
        );
    }

    #[test]
    fn an_empty_store_is_a_portfolio_that_holds_nothing_and_says_so() {
        // The state every deployment is in today, and the reason the refusal
        // above is not a check that fires on the normal case: a readable store
        // with no positions is a complete account of holding nothing.
        let dir = tempfile::tempdir().expect("tempdir");
        let (portfolio, state) = inventory(&Reader::open(dir.path()), radar_types::Slot(20_000))
            .expect("an empty store is readable");
        assert_eq!(portfolio.incompleteness(), None);
        assert_eq!(portfolio.holdings().count(), 0);
        assert_eq!(portfolio.wallet(), None, "no wallet is invented");
        assert_eq!(state.deployed, MicroUsd::ZERO);
    }
}
