// SPDX-License-Identifier: Apache-2.0
//! The loop the daemon runs, and the configuration it reads.
//!
//! # Why this is a module and not the binary
//!
//! The binary is four lines. Everything it does is here, so that one poll --
//! read, answer, log, publish, advance the cursor, save the ledger -- can be
//! driven by a test against a fake platform instead of only by systemd against
//! the real one.
//!
//! That is not a testing convenience. The orderings this loop enforces are the
//! ones that cost money or credibility when they are wrong: a mention refused
//! after the chain was read has already been paid for, a reply posted before it
//! was logged is a public statement with no record, and a cursor advanced past a
//! mention that was never answered is a question silently dropped.

use std::time::Duration;

use radar_onchain::Budget as CallBudget;
use radar_provider::Budget;
use radar_roast::{BaseRates, Billed};
use radar_types::{Address, MicroUsd};

use crate::admission::{Gate, Limits, Refused};
use crate::answer::{Answered, Answering};
use crate::poll;
use crate::publish::{DryRun, Publisher};
use crate::spend::{Cost, Prices, Spend};
use crate::x::X;

/// Where the loop keeps its files.
pub struct Paths {
    /// The reply log.
    pub log: String,
    /// The last answered mention.
    pub cursor: String,
    /// The spend ledger.
    pub ledger: String,
    /// The Telegram lane's own log. **Never read for the contest**: the
    /// leaderboard, the week-close job and the hunter tally read `log`, and a
    /// Telegram answer stays out of the record by being in a different file
    /// rather than by carrying a flag (design 0009 L5).
    pub telegram_log: String,
    /// The Telegram lane's `getUpdates` offset.
    pub telegram_cursor: String,
    /// Who the X gate refused, and when. The week-close job reads it: an
    /// account refused during the week does not win it (design 0007 §6.2).
    pub refusals: String,
    /// The account's own posts -- the weekly result, the daily "seven days
    /// later" -- recorded before they are said, like replies.
    pub posts: String,
    /// The contest's week records and the pool reading, which the public
    /// endpoints serve.
    pub contest_dir: String,
    /// The daily "seven days later" rows, written by `radar seven-days-later`
    /// on a timer and posted from here.
    pub daily_dir: String,
}

impl Paths {
    /// Under one directory, so an operator moves one thing.
    ///
    /// The contest directory is a sibling rather than a child: `radar-serve`
    /// reads it as `RADAR_CONTEST_DIR`, defaulting to `data/contest`, and the
    /// two defaults have to name the same place.
    #[must_use]
    pub fn under(dir: &str) -> Self {
        let contest_dir = std::path::Path::new(dir).parent().map_or_else(
            || "data/contest".to_owned(),
            |p| format!("{}/contest", p.display()),
        );
        Self {
            log: format!("{dir}/replies.jsonl"),
            cursor: format!("{dir}/cursor"),
            ledger: format!("{dir}/ledger.json"),
            telegram_log: format!("{dir}/telegram.jsonl"),
            telegram_cursor: format!("{dir}/telegram.cursor"),
            refusals: format!("{dir}/refusals.jsonl"),
            posts: format!("{dir}/posts.jsonl"),
            contest_dir,
            daily_dir: format!("{dir}/daily"),
        }
    }
}

/// What the account says to the second person who asks about the same mint.
///
/// A URL rather than a bare id, so it is one tap on a phone. The account's own
/// handle is not needed: X resolves `/i/web/status/<id>` to the post whoever
/// wrote it, which also means this line cannot go stale if the handle changes.
///
/// Deliberately short, and deliberately not a summary. Restating the answer
/// here would be a second public statement about a coin, made from a fact sheet
/// nobody rebuilt at a slot nobody read — which is the whole thing the fact
/// sheet boundary exists to stop.
#[must_use]
pub fn pointer_reply(reply_id: &str) -> String {
    format!("Asked and answered within the hour: https://x.com/i/web/status/{reply_id}")
}

/// Seconds since the epoch.
#[must_use]
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The accounting day a timestamp falls in.
///
/// Whole days since the epoch, UTC. The meter's window and the gate's are the
/// same day for the same reason a bill is: an operator reading "spent today"
/// and "replies today" should not have to ask which today.
#[must_use]
pub const fn day_of(secs: u64) -> u64 {
    secs / 86_400
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// The budget, or a closed one.
///
/// Takes a getter rather than reading the environment, so the rule can be tested
/// without setting process-wide variables that parallel tests would fight over —
/// the same shape `Prices::from_vars` uses, and for the same reason.
///
/// Dollars in, micro-USD out, because an operator writing a daily cap thinks in
/// dollars and the meter counts in millionths. A value that will not parse is
/// **closed**, not ignored: a typo in a spending ceiling must not read as
/// permission.
pub fn budget_from(get: &impl Fn(&str) -> Option<String>) -> Budget {
    let daily = get("RADAR_ANALYST_DAILY_USD")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(MicroUsd::from_dollars);
    let per_call = get("RADAR_ANALYST_PER_CALL_USD")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(MicroUsd::from_dollars);
    match (daily, per_call) {
        (Some(daily_max), Some(per_call_max)) => Budget {
            per_call_max,
            daily_max,
        },
        _ => Budget::CLOSED,
    }
}

/// What to say when the budget refuses everything, or `None`.
///
/// A function rather than an `if` inside [`run`], because `run` never returns
/// and nothing inside it can be tested. The decision — *is this instance
/// funded* — is worth pinning: an operator who mistypes a ceiling gets a
/// service that answers nothing, and this line is the whole difference between
/// that and a mystery.
#[must_use]
pub fn unfunded_notice(budget: Budget) -> Option<&'static str> {
    (budget == Budget::CLOSED).then_some(
        "radar-analyst: unfunded -- RADAR_ANALYST_DAILY_USD and \
         RADAR_ANALYST_PER_CALL_USD are not both set, so every call is refused.",
    )
}

/// The admission limits, or ones that refuse everything.
///
/// Takes a getter, for the reason [`budget_from`] does.
///
/// Unset means zero, and zero means refuse. `Limits` has no `Default` in the
/// library on purpose — a default here would be a spending policy invented by
/// whoever typed it — so this function is where the absence is turned into a
/// refusal rather than into a number.
pub fn limits_from(get: &impl Fn(&str) -> Option<String>) -> Limits {
    let n = |key: &str| get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    Limits {
        per_summoner_daily: n("RADAR_ANALYST_PER_SUMMONER_DAILY"),
        global_daily: n("RADAR_ANALYST_GLOBAL_DAILY"),
        // The one with a default, because a dedupe window is not a spending
        // decision -- it decides how long "already answered" lasts, and zero
        // would mean the same coin is answered again on the next poll. An hour
        // is the same figure the command uses.
        dedupe_seconds: get("RADAR_ANALYST_DEDUPE_SECONDS")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(3_600),
    }
}

/// The analyst's own token, or `None` when no token is special.
///
/// ADR 0013 constraint 5. Takes a getter, for the reason [`budget_from`] does.
///
/// Three answers, and they are deliberately not two:
///
/// - **Unset or blank is `Ok(None)`**: no mint is special, and every coin is
///   answered on the same rule. This does not bend rule 8 -- there is no spend
///   and no permission in it, only a rule with nothing to apply to. The token
///   does not exist until ADR 0013's launch gate is met, and until then the
///   correct configuration is no configuration.
/// - **A value that parses is `Ok(Some(mint))`.**
/// - **A value that does not parse is `Err`, and the caller must not run.** A
///   misspelt mint would silently switch the rule off for the real token, which
///   is the one direction ADR 0013 exists to prevent: the analyst stating its
///   own price. Same shape as a price list that will not parse -- the instance
///   says what is wrong and answers nothing. The value is not echoed, because
///   the likeliest wrong value is some other variable's secret pasted on the
///   wrong line.
///
/// # Errors
///
/// When the variable is set to something that is not a base58 address.
pub fn self_mint_from(get: &impl Fn(&str) -> Option<String>) -> Result<Option<Address>, String> {
    let Some(raw) = get("RADAR_SELF_MINT") else {
        return Ok(None);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    raw.parse::<Address>().map(Some).map_err(|_| {
        "RADAR_SELF_MINT is set and is not an address, so the analyst's own token cannot be \
         told apart and nothing is answered. Set it to the mint, or unset it if no token \
         exists yet."
            .to_owned()
    })
}

/// What the daemon says about which token, if any, is its own.
///
/// Said on every start, beside the publishing posture, so an operator reading
/// the journal after the token exists can see in one line whether the rule is
/// armed for the right mint.
#[must_use]
pub fn self_mint_notice(self_mint: Option<&Address>) -> String {
    match self_mint {
        None => "radar-analyst: no RADAR_SELF_MINT, so no token is the analyst's own and every \
                 coin is answered on the same rule."
            .to_owned(),
        Some(mint) => format!(
            "radar-analyst: RADAR_SELF_MINT={mint} -- its price and market capitalisation are \
             never stated; everything else about it is answered like any other coin."
        ),
    }
}

/// Whether this instance may actually say anything in public.
///
/// # Why this is not the credential
///
/// It was. The token was both the reader and the publisher, so pasting it into
/// `/etc/radar/analyst.env` turned a silent instance into a public account in
/// one step — and there was no way to read live mentions while answering
/// nobody.
///
/// That is the wrong shape for two reasons. The launch gate in design 0007 asks
/// for a hundred replies to be **read beside their fact sheets** before anybody
/// outside sees one, and with one switch that gate could only be satisfied by
/// publishing the hundred. And on 2026-09-04 two wrong figures were found in the
/// reply path in a single day — a cost 6.7× too high, and a charge signed as a
/// gain — both by looking at real output. The first hundred replies are exactly
/// where the next one gets found, and they should not be found in public.
///
/// So speaking is its own decision. Rule 8: the most consequential action here
/// requires somebody to type a word, not merely to have pasted a token.
///
/// Anything other than `on` is off, including `true`, `1` and `yes`. A ceiling
/// spelled wrongly must not read as permission, and neither must this — the
/// value is checked against exactly one word so that a typo fails closed and
/// the log says which state it is in.
#[must_use]
pub fn may_publish(get: &impl Fn(&str) -> Option<String>) -> bool {
    get("RADAR_X_PUBLISH").is_some_and(|v| v.trim().eq_ignore_ascii_case("on"))
}

/// What the daemon says about which state it is in.
///
/// Four, and each is a real situation somebody will be in:
///
/// 1. no bearer, so nothing is read and nothing is posted;
/// 2. reading, deliberately silent — the state the launch gate is read in;
/// 3. **switched on with no signing credential**, which is a misconfiguration;
/// 4. live.
///
/// The third exists because the platform needs two different credentials: a
/// bearer to read mentions, and four OAuth 1.0a values to post. Somebody who
/// sets the bearer, switches publishing on, and stops there has an instance that
/// answers every mention and delivers none of them — and states 2 and 3 look
/// identical from everywhere except the log, so the daemon names them apart on
/// every start.
#[must_use]
pub fn posture(has_credential: bool, can_post: bool, publishing: bool) -> &'static str {
    match (has_credential, can_post, publishing) {
        (false, _, _) => "radar-analyst: no credential, so nothing is read and nothing is posted.",
        (true, _, false) => {
            "radar-analyst: reading mentions and answering them to the log ONLY -- \
             set RADAR_X_PUBLISH=on to speak in public."
        }
        (true, false, true) => {
            "radar-analyst: RADAR_X_PUBLISH=on but there is no signing credential, so every \
             reply will be answered and none delivered. A bearer can read; posting needs \
             RADAR_X_API_KEY, RADAR_X_API_SECRET, RADAR_X_ACCESS_TOKEN and RADAR_X_ACCESS_SECRET."
        }
        (true, true, true) => "radar-analyst: LIVE -- replies are being posted publicly.",
    }
}

/// Which publisher the loop speaks through.
///
/// # Why this is a function
///
/// It was three lines inside [`run`], and `run` never returns, so nothing could
/// call it. Mutation testing said so precisely: deleting the arm that selects the
/// live client left every test passing, and the resulting daemon is one that
/// holds a valid credential, is switched on, and silently posts nothing.
///
/// That is the single most consequential line in this crate in the direction
/// nobody notices. A daemon that wrongly *posts* is caught within a minute by
/// anybody looking at the account; a daemon that wrongly *stays silent* looks
/// exactly like a quiet week, which the `analyst` check in `radar brief` is
/// deliberately built not to alarm about.
///
/// So the choice is out here where a test can make it, and `Publisher::name`
/// is what the test reads.
#[must_use]
pub fn publisher_for(x: Option<X>, publishing: bool) -> Box<dyn Publisher> {
    match (x, publishing) {
        (Some(client), true) => Box::new(client),
        _ => Box::new(DryRun),
    }
}

/// Who the gate never answers.
///
/// **The bot's own numeric id**, which is what a mention's `author_id` carries.
/// This was the literal string `radar` until 2026-09-06 -- a value no X account
/// id can equal, so the one entry the ignore list exists for was never in it,
/// and the account would have answered its own mention had anything produced
/// one. Research 0029, S21. The contest's own operator list has read
/// `x.user_id()` since #167; the gate did not.
///
/// With no credential there is no id and nothing to poll, so the list is empty
/// rather than carrying a placeholder that matches nobody.
#[must_use]
pub fn ignored(x: Option<&X>) -> Vec<String> {
    x.map(|x| vec![x.user_id().to_owned()]).unwrap_or_default()
}

/// What to say when the contest's directory cannot be written, or `None`.
///
/// # Why this is a start-up check and not a runtime one
///
/// The analyst writes the contest's records **once a week**, at 00:00 UTC on
/// the Monday. Everything else it does -- polling, answering, posting, metering
/// -- touches only its own directory. So a deployment that grants write to the
/// analyst's directory and not the contest's looks perfect for six days and
/// then cannot close the week, and the only symptom is a line in a journal at
/// midnight.
///
/// That is not hypothetical. `deploy/radar-analyst.service` had exactly one
/// `ReadWritePaths` entry, `Paths::under` puts the contest directory *beside*
/// the analyst's rather than inside it, and on 2026-09-07 the close said
/// `Read-only file system (os error 30)` every five minutes for ninety minutes
/// into a journal nobody was reading. The contest could not have closed at all.
///
/// So the probe runs at start, where somebody is looking, and it names the
/// systemd directive rather than the symptom -- the operator reading this has
/// a unit file open, not a strace.
///
/// It creates and removes a file rather than checking permissions, because the
/// thing that failed was not a permission: the directory was `0755` and owned
/// by the right user, and the kernel refused the write anyway because of a
/// sandbox the process cannot see from its own metadata.
#[must_use]
pub fn contest_writable_notice(contest_dir: &str) -> Option<String> {
    let probe = std::path::Path::new(contest_dir).join(".radar-write-probe");
    if std::fs::create_dir_all(contest_dir)
        .and_then(|()| std::fs::write(&probe, b""))
        .is_ok()
    {
        let _ = std::fs::remove_file(&probe);
        return None;
    }
    Some(format!(
        "radar-analyst: {contest_dir} CANNOT BE WRITTEN, so the week will not close and \
         no prize can be recorded. Replies still work. If this is a systemd unit, add it \
         to ReadWritePaths -- it is a sibling of the analyst's directory, not a child."
    ))
}

/// What to say when no model provider was built.
///
/// Rule 8's other half. An **unconfigured** provider is a resting state; a
/// **mis**-configured one is a mistake; and `radar_model::from_vars(&env).ok()`
/// made the two look identical. A key set with a price missing produced an
/// account that answered exactly as it had the day before -- `fellback:
/// NoProvider` on every reply -- with nothing anywhere saying the key had been
/// read and rejected. `Selection` names every missing variable precisely so
/// that an operator setting one up, at the point where nothing works yet and
/// there is no other signal, reads one line instead of guessing; this is what
/// lets it reach them.
///
/// A function rather than an `if` inside [`run`], for the reason
/// [`unfunded_notice`] is one: `run` never returns, so nothing inside it can be
/// tested.
#[must_use]
pub fn provider_notice(why: &radar_model::Selection) -> String {
    match why {
        // Not a fault, and it must not read as one. This is the state the
        // account has shipped in since it went live, and the template is a
        // working product rather than a degraded one.
        radar_model::Selection::None => {
            "radar-analyst: no model provider, so every reply is the deterministic template."
                .to_owned()
        }
        _ => format!(
            "radar-analyst: a model provider is configured and UNUSABLE, so every reply is \
             the template until it is fixed -- {why}"
        ),
    }
}

/// Runs the loop. Never returns.
#[allow(
    clippy::too_many_lines,
    reason = "the daemon's start-up, read once top to bottom"
)]
pub fn run() -> ! {
    let dir = env("RADAR_ANALYST_DIR").unwrap_or_else(|| "data/analyst".to_owned());
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("radar-analyst: cannot use {dir}: {e}");
        std::process::exit(1);
    }
    let paths = Paths::under(&dir);

    // The credential is the **source**. Speaking is a separate decision, and
    // `may_publish` says why. Absent the credential the loop reads nothing and
    // posts nothing; present but not switched on, it reads and answers into the
    // log, which is the state the launch gate is read in.
    let x = X::from_env();
    let publishing = may_publish(&env);
    let publisher = publisher_for(x.clone(), publishing);
    eprintln!(
        "{}",
        posture(x.is_some(), x.as_ref().is_some_and(X::can_post), publishing)
    );
    // The commit and the operator set, beside the posture, because both are
    // states an operator otherwise has to infer. The build sha closes the trap
    // that caught us twice -- a change believed deployed, debugged against a
    // process that predated it -- and the operator count is what makes Phase
    // 0.3 checkable: a duplicated or mistyped `RADAR_CONTEST_OPERATORS` line
    // silently leaves the managing account eligible to win the prize, and the
    // only visible difference is this number.
    eprintln!("{}", build_notice(operator_ids(x.as_ref()).len()));
    // Once, at start, where somebody is looking. See the doc comment: this
    // failure is silent for six days and then loses a week.
    if let Some(notice) = contest_writable_notice(&paths.contest_dir) {
        eprintln!("{notice}");
    }
    // The bio noticeboard, which is off unless somebody wrote a lead. Said at
    // start either way: "the bio is not being written" and "the bio is being
    // written and you did not know" are both things an operator should be able
    // to read off a restart.
    let bio = crate::bio::Bio::from_vars(&env);
    eprintln!("{}", bio_notice(bio.as_ref()));

    // The free lane, on its own token, its own switch, its own caps and its
    // own log (design 0009 L5). Same rule 8 shape as X: no token, nothing read.
    let telegram = crate::telegram::Telegram::from_env();
    let telegram_publishing = crate::telegram::may_publish(&env);
    let telegram_publisher = crate::telegram::publisher_for(telegram.clone(), telegram_publishing);
    eprintln!(
        "{}",
        crate::telegram::posture(telegram.is_some(), telegram_publishing)
    );
    let mut telegram_gate = Gate::new(crate::telegram::limits_from(&env), Vec::new());

    let Some(prices) = Prices::from_vars(&env) else {
        // Not an exit. A price list is a spending decision and its absence is a
        // configuration state, not a crash -- but nothing may be answered
        // without it, because an unpriced call cannot be metered and an
        // unmetered call is the open invoice this account cannot afford.
        eprintln!(
            "radar-analyst: no prices configured, so nothing can be metered and \
             nothing will be answered. Set RADAR_X_PRICE_MENTION_READ, \
             RADAR_X_PRICE_POST_READ, RADAR_X_PRICE_REPLY and \
             RADAR_MODEL_PER_CALL_USD_MICRO -- see deploy/analyst.env.example."
        );
        idle_forever();
    };

    let budget = budget_from(&env);
    if let Some(notice) = unfunded_notice(budget) {
        eprintln!("{notice}");
    }

    let limits = limits_from(&env);
    let mut gate = Gate::new(limits, ignored(x.as_ref()));
    // **Rebuilt from disk, not started empty.** Every count in the gate lived
    // only in memory, and this daemon runs under `Restart=always`: it restarted
    // three times on the night of 2026-09-06, and each restart handed every
    // summoner a fresh allowance, emptied the day's total and forgot every mint
    // answered in the last hour. A crash loop was a spending loop.
    //
    // Read through the same functions the rest of the crate uses, and a missing
    // file is no history rather than a failure to start -- a first run has
    // neither log.
    {
        let replies = crate::log::read(&paths.log).unwrap_or_default();
        gate.restore(&replies, now());
        eprintln!(
            "radar-analyst: gate restored — {} sent today, {} mints inside the dedupe window",
            gate.sent_today(),
            gate.answered_recently()
        );
    }
    let mut spend = Spend::open(budget, prices, paths.ledger.clone(), day_of(now()));

    let client = radar_onchain::RpcClient::from_vars(&env);
    // **A stale snapshot is dropped, not quoted.**
    //
    // `is_stale_at` has existed since the module was written and had one caller
    // — `radar roast`, which prints a warning to one person and then uses the
    // figures anyway. That is defensible for a debugging command. It is not
    // defensible here: this process publishes, and research 0024's own opening
    // argument is that 0008's headline was wrong by 2.7x **nine days** later
    // because the recipient distribution is a configuration of whatever tool
    // the launchers are running rather than a law.
    //
    // So a month-old distribution quoted as current is a measurement about a
    // population that no longer exists, said in public, by an account whose
    // whole claim is that its numbers are measured.
    //
    // Dropped rather than fatal. The daemon already handles `None` — the reply
    // carries no population context and says so — and taking the account off the
    // air over a research file would be a larger outage than the fault. The
    // threshold is `baserates::STALE_AFTER_DAYS`, not a second number invented
    // here: two thresholds for one question is how they drift apart.
    let rates = match BaseRates::load(radar_roast::baserates::DEFAULT_PATH) {
        Ok(loaded) if loaded.is_stale_at(&crate::daily::date_of(now())) => {
            eprintln!(
                "radar-analyst: base rates were measured on {} and are stale after {} days; \
                 dropping them, so replies will carry no population context until \
                 research 0024 is re-run",
                loaded.measured_on,
                radar_roast::baserates::STALE_AFTER_DAYS
            );
            None
        }
        Ok(loaded) => Some(loaded),
        Err(e) => {
            eprintln!(
                "radar-analyst: no base rates ({e}); replies will carry no population context"
            );
            None
        }
    };
    // The fact that makes one reply differ from another. Absent, every reply
    // about a fresh launch says the same thing, so its absence is reported
    // rather than left to be noticed in the output.
    let creators = radar_roast::CreatorIndex::read(radar_roast::creator::DEFAULT_PATH).ok();
    if creators.is_none() {
        eprintln!(
            "radar-analyst: no creator index; replies will say nothing about who launched              the token. Build one with `radar creator-index`."
        );
    }
    let provider = match radar_model::from_vars(&env) {
        Ok(provider) => Some(provider),
        Err(why) => {
            eprintln!("{}", provider_notice(&why));
            None
        }
    };

    // ADR 0013 constraint 5. A value that will not parse idles the instance
    // rather than running with the rule off: `self_mint_from` says why.
    let self_mint = match self_mint_from(&env) {
        Ok(mint) => mint,
        Err(e) => {
            eprintln!("radar-analyst: {e}");
            idle_forever();
        }
    };
    eprintln!("{}", self_mint_notice(self_mint.as_ref()));

    eprintln!(
        "radar-analyst: publisher={} source={} telegram={} dir={dir}",
        publisher.name(),
        if x.is_some() { "x" } else { "none" },
        if telegram.is_some() {
            telegram_publisher.name()
        } else {
            "off"
        }
    );

    let mut wait = poll::BUSY;
    loop {
        let found = tick(
            x.as_ref(),
            publisher.as_ref(),
            &mut gate,
            &mut spend,
            &client,
            rates.as_ref(),
            creators.as_ref(),
            provider.as_deref(),
            self_mint.as_ref(),
            &paths,
        );
        let found_telegram = crate::telegram::tick(
            telegram.as_ref(),
            telegram_publisher.as_ref(),
            &mut telegram_gate,
            &mut spend,
            &client,
            rates.as_ref(),
            creators.as_ref(),
            provider.as_deref(),
            self_mint.as_ref(),
            &paths,
        );
        // The week closes on the tick after Monday 00:00 UTC, once. The
        // record is written first; the posts are written from the record.
        // Every account the operator controls, not just the bot's own.
        //
        // The bot posts as itself and is managed from a person's own account.
        // Only the bot's id was excluded before 2026-09-06, so the managing
        // account could have entered its own contest and won -- the operator
        // paying themselves out of a pool the public is told is theirs.
        //
        // `RADAR_CONTEST_OPERATORS` is a comma-separated list of numeric ids;
        // the bot's own id is always in the set whether or not it is listed, so
        // forgetting the variable cannot make the bot eligible.
        let rules = radar_contest::Rules::published(operator_ids(x.as_ref()));
        match crate::contest::close_if_due(
            x.as_ref(),
            &paths,
            now(),
            &rules,
            limits.per_summoner_daily,
            &mut spend,
        ) {
            Ok(Some(record)) => announce_week(
                &record,
                publisher.as_ref(),
                telegram_publisher.as_ref(),
                &mut spend,
                &client,
                rates.as_ref(),
                creators.as_ref(),
                provider.as_deref(),
                self_mint.as_ref(),
                &paths,
            ),
            Ok(None) => {}
            Err(e) => eprintln!("radar-analyst: cannot write the week's record: {e}"),
        }
        // Every tick, not only at close: see the function's note.
        prompt_claim_if_due(publisher.as_ref(), &mut spend, &paths);
        // The bio, when one is configured. Last of the three, because it is
        // the only one that overwrites rather than appends, and the cheapest
        // to skip.
        if let Some(bio) = bio.as_ref() {
            write_bio_if_changed(x.as_ref(), bio, &mut spend, &paths);
        }
        // The daily post, from the rows the timer job wrote, once past the
        // hour. Priced as one top-level post when it goes out on X.
        announce_day(
            publisher.as_ref(),
            telegram_publisher.as_ref(),
            &mut spend,
            &paths,
        );
        wait = next_wait(found, found_telegram, wait);
        std::thread::sleep(wait);
    }
}

/// How long to sleep after a tick that found `found` X mentions and
/// `found_telegram` Telegram messages.
///
/// Either lane finding something keeps the loop busy: the two counts are
/// added and handed to [`poll::interval`]. A function rather than a line
/// inside [`run`] because `run` never returns, and CI's mutants replaced the
/// `+` with `*` and `-` with nothing failing -- a loop that went idle while
/// one lane was busy, or one that panicked on underflow, and no test could
/// see either.
#[must_use]
pub fn next_wait(found: usize, found_telegram: usize, previous: Duration) -> Duration {
    poll::interval(found + found_telegram, previous)
}

/// Settles a post's reservation when something was sent and releases it when
/// nothing was.
///
/// A reservation for a post that never left -- a dry run, a refused text, a
/// publisher that failed -- must go back, or the day's budget is spent on
/// posts nobody received; one that did leave is charged at what was reserved,
/// because the platform reports no per-call price. A function because CI's
/// mutants turned this `>` into `==` inside two functions nothing could call
/// from a test, and a meter that settles the empty case and releases the
/// real one is a meter that runs out on quiet weeks and never on busy ones.
/// What to say when the budget covered only part of the thread, or `None`.
///
/// A function rather than an `if` inside [`announce_week`], for the reason
/// [`unfunded_notice`] gives and one more: CI mutated this comparison into
/// `==`, `>` and `<=` and nothing failed, because `announce_week` needs a
/// platform to run at all. `>` in particular is the dangerous one -- it prints
/// a shortfall on every ordinary week and says nothing on the one week that
/// actually lost a post.
///
/// `None` when the whole thread is covered. Silence is the right output for
/// the ordinary case: a line on every close is a line nobody reads by the
/// third week.
#[must_use]
fn short_thread_notice(reserved: usize, wanted: usize) -> Option<String> {
    (reserved < wanted).then(|| {
        format!(
            "radar-analyst: budget covers {reserved} of the week's {wanted} posts; \
             the rest are not published"
        )
    })
}

/// Reserves a thread: one [`Cost::Post`] and a [`Cost::Reply`] for each post
/// after it.
///
/// Returns **as many reservations as the budget covers**, which may be fewer
/// than asked for and may be none. The caller cuts the thread to fit rather
/// than posting what it cannot pay for -- a shorter thread of true posts is a
/// smaller loss than a spend nobody authorised, and the summary is always
/// first, so what gets dropped is the least load-bearing end.
///
/// Split out and returning a `Vec` because the alternative -- one commitment
/// for the whole thread -- is what was wrong: `announce_week` charged a single
/// `Cost::Post` for up to three posts, so the day's cap was computed from a
/// number smaller than what was actually spent.
fn reserve_thread(spend: &mut Spend, posts: usize, day: u64) -> Vec<radar_provider::Commitment> {
    let mut out = Vec::with_capacity(posts);
    for n in 0..posts {
        let cost = if n == 0 { Cost::Post } else { Cost::Reply };
        match spend.authorize(cost, day) {
            Ok(c) => out.push(c),
            // The first refusal ends it. Reserving past one would leave a hole
            // in the middle of a thread, and a reply with no parent is not a
            // shorter thread, it is a different post.
            Err(_) => break,
        }
    }
    out
}

/// Settles a thread against what actually landed.
///
/// `publish_under` stops the thread when the platform refuses a post, so a
/// partial thread is an ordinary state and not an error. The posts that landed
/// are charged and the rest are given back -- the same rule
/// [`settle_if_sent`] applies to one post, applied per post.
fn settle_thread(spend: &mut Spend, reservations: Vec<radar_provider::Commitment>, sent: usize) {
    for (n, reservation) in reservations.into_iter().enumerate() {
        if n < sent {
            let charged = reservation.reserved();
            spend.settle(reservation, charged);
        } else {
            spend.release(reservation);
        }
    }
}

fn settle_if_sent(spend: &mut Spend, reservation: radar_provider::Commitment, sent: usize) {
    if sent > 0 {
        let charged = reservation.reserved();
        spend.settle(reservation, charged);
    } else {
        spend.release(reservation);
    }
}

/// Posts today's "seven days later" if it is due, metering the X post.
fn announce_day(
    publisher: &dyn Publisher,
    telegram: &dyn Publisher,
    spend: &mut Spend,
    paths: &Paths,
) {
    let at = now();
    if crate::daily::due(at, &paths.daily_dir).is_none() {
        return;
    }
    let vault = std::fs::read_to_string(format!("{}/pool.json", paths.contest_dir))
        .ok()
        .and_then(|text| radar_contest::Vault::from_json(&text).ok());
    let Ok(reservation) = spend.authorize(Cost::Post, day_of(at)) else {
        eprintln!("radar-analyst: budget spent; today's post is not published");
        return;
    };
    match crate::daily::post_if_due(
        at,
        &paths.daily_dir,
        vault.as_ref(),
        publisher,
        &paths.posts,
        telegram,
        &paths.telegram_log,
    ) {
        Ok(sent) => settle_if_sent(spend, reservation, sent),
        Err(e) => {
            spend.release(reservation);
            eprintln!("radar-analyst: cannot post the day: {e}");
        }
    }
}

/// Posts a closed week: the summary, then the winner's coin torn down as a
/// reply to it, on X and -- when a channel is configured -- on Telegram.
///
/// The teardown reads the chain once for the winning mint, the way a summoned
/// reply would, and is written by the same roaster under the same checks. A
/// week with no winner posts the summary alone.
#[allow(clippy::too_many_arguments)]
fn announce_week(
    record: &radar_contest::Record,
    publisher: &dyn Publisher,
    telegram: &dyn Publisher,
    spend: &mut Spend,
    client: &radar_onchain::RpcClient,
    rates: Option<&BaseRates>,
    creators: Option<&radar_roast::CreatorIndex>,
    provider: Option<&dyn radar_model::Provider>,
    self_mint: Option<&radar_types::Address>,
    paths: &Paths,
) {
    let at = now();
    let vault = std::fs::read_to_string(format!("{}/pool.json", paths.contest_dir))
        .ok()
        .and_then(|text| radar_contest::Vault::from_json(&text).ok());
    let mut posts = vec![crate::weekly::summary(record, vault.as_ref())];

    if let Some(winner) = record.ranking.winner() {
        match winner.entry.mint.parse::<radar_types::Address>() {
            Ok(mint) => {
                let mut budget = radar_onchain::budget::Budget::default();
                match radar_onchain::build(client, &mut budget, &mint) {
                    Ok(dossier) => {
                        let (sheet, reply) =
                            radar_roast::roast(&dossier, rates, creators, provider, self_mint);
                        posts.push(crate::weekly::teardown(&sheet, &reply));
                    }
                    Err(e) => {
                        eprintln!("radar-analyst: no teardown, the chain could not be read: {e}");
                    }
                }
            }
            Err(_) => eprintln!("radar-analyst: no teardown, the winning mint is not an address"),
        }
    }

    // The hunters, from the board the week close already wrote beside the
    // record. Read rather than recomputed: the board on disk is the one the
    // public endpoint serves, and a post naming a different three would be a
    // second answer to the same question.
    let board: Vec<radar_contest::hunter::Placing> =
        std::fs::read_to_string(crate::contest::hunter_path(&paths.contest_dir, record.week))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
    if let Some(post) = crate::weekly::hunters(record, &board) {
        posts.push(post);
    }

    // A post and a reply for each one after it, priced as what they are. This
    // charged **one** `Cost::Post` for the whole thread until 2026-09-06 --
    // already short by the teardown, and short by two once the hunters post
    // joined it. A meter that under-reports the one billable thing a stranger
    // can trigger is worse than no meter, because the daily cap is computed
    // from it.
    //
    // The thread is cut to what the budget covers rather than posted in full
    // and charged for part of it. A thread of two true posts is a smaller loss
    // than a spend nobody authorised, and the summary is first in the list, so
    // what gets dropped is the least load-bearing end.
    let today = day_of(at);
    let reservations = reserve_thread(spend, posts.len(), today);
    if reservations.is_empty() {
        eprintln!("radar-analyst: budget spent; the week's post is not published");
        return;
    }
    if let Some(notice) = short_thread_notice(reservations.len(), posts.len()) {
        eprintln!("{notice}");
    }
    // Unconditional, because `truncate` to a length at or past the end is a
    // no-op. The comparison that decides whether to *say* something lives in
    // `short_thread_notice`, where it can be tested -- CI mutated it here into
    // `==`, `>` and `<=` and none of them failed, because nothing can call
    // `announce_week` without a platform.
    posts.truncate(reservations.len());
    match crate::weekly::publish(
        publisher,
        &paths.posts,
        &format!("weekly:{}", record.week.0),
        &posts,
        at,
    ) {
        Ok(sent) => settle_thread(spend, reservations, sent),
        Err(e) => {
            for r in reservations {
                spend.release(r);
            }
            eprintln!("radar-analyst: cannot write {}: {e}", paths.posts);
            return;
        }
    }
    // Free, and recorded in the same file under the same id so a reader sees
    // both lanes side by side.
    if let Err(e) = crate::weekly::publish(
        telegram,
        &paths.telegram_log,
        &format!("weekly:{}", record.week.0),
        &posts,
        at,
    ) {
        eprintln!("radar-analyst: cannot write {}: {e}", paths.telegram_log);
    }
}

/// What the process says about the bio writer on start.
///
/// A line whether or not it is on. "The bio is not being written" and "the bio
/// is being written and you did not know" are both states an operator should be
/// able to read off a restart -- and the second one matters more, because a bio
/// write overwrites the only copy of whatever was there.
#[must_use]
pub fn bio_notice(bio: Option<&crate::bio::Bio>) -> String {
    match bio {
        None => "radar-analyst: the bio is not written (RADAR_BIO_LEAD unset), so the \
                 account's own copy stands."
            .to_owned(),
        Some(b) => format!(
            "radar-analyst: the bio IS written, at most hourly, as \"{}\" plus the week's \
             status. Whatever is in the profile now will be replaced.",
            b.lead
        ),
    }
}

/// The marker a bio write leaves: when, and what it said.
///
/// One file rather than two, because the two facts are only useful together --
/// "written an hour ago" and "said this" answer one question, which is whether
/// to write again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BioMarker {
    /// When the last accepted write happened.
    pub at: u64,
    /// Exactly what it wrote.
    pub text: String,
}

impl BioMarker {
    /// Parses the marker file, or `None` when there is not one.
    ///
    /// A file that will not parse is `None`, which means *write again*. That
    /// is the safe direction here: the alternative is a torn marker freezing
    /// the bio at whatever it happened to say.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (head, rest) = text.split_once('\n')?;
        Some(Self {
            at: head.trim().parse().ok()?,
            text: rest.to_owned(),
        })
    }

    /// The file's contents for a write at `at` saying `text`.
    #[must_use]
    pub fn render(at: u64, text: &str) -> String {
        format!("{at}\n{text}")
    }
}

/// The bio to write now, or `None` when there is nothing to do.
///
/// **Every decision about whether to write is here**, so the function that
/// actually writes has none left. That is not tidiness: `write_bio_if_changed`
/// needs an X client and a filesystem, so nothing in it can be reached by a
/// test -- and CI proved the point three times, killing no mutant of any
/// comparison that lived there.
///
/// `None` for each of five reasons, and they are genuinely five:
///
/// - the week's record says nothing worth a bio ([`crate::bio::state_of`] --
///   a voided week, a week past its claim window, a winner with no handle);
/// - the render would not fit in [`crate::bio::MAX`] characters, and a bio the
///   platform cuts mid-figure is a wrong figure with no record it was right;
/// - it was written less than an hour ago;
/// - the text has not changed;
/// - the render fails the checks a reply passes.
///
/// The last is worth its own note. A refused reply falls back to the template;
/// a refused **bio** falls back to the bio that is already there, which was
/// checked when it was written. So refusing is strictly safe here, in a way it
/// is not for a reply.
fn bio_to_write(
    bio: &crate::bio::Bio,
    record: &radar_contest::Record,
    marker: Option<&BioMarker>,
    now: u64,
) -> Option<String> {
    let state = crate::bio::state_of(record, now)?;
    let text = bio.render(&state)?;
    if !bio_write_due(now, marker, &text) {
        return None;
    }
    match crate::bio::check(&text, &state.authorised()) {
        Ok(()) => Some(text),
        Err(why) => {
            eprintln!(
                "radar-analyst: the bio was refused by its own checks and not written: {why}"
            );
            None
        }
    }
}

/// Whether to write the bio now.
///
/// Two conditions, both of them cheap and both of them protecting the same
/// thing -- a metered endpoint that overwrites the only copy of a public
/// profile:
///
/// - **At most once an hour.** The bio changes at most three times a week, so
///   a writer that tried on every tick would spend the day's budget
///   discovering that nothing had changed.
/// - **Only when the text differs.** The ordinary case is that it does not.
///
/// Pure, and split out of [`write_bio_if_changed`] because the two comparisons
/// are the whole of the decision and neither is observable from a function that
/// needs a platform to run. CI proved that twice: inverting the freshness guard
/// writes *only* when it is too soon, and inverting the text comparison writes
/// *only* when nothing changed -- which is a bio frozen at its first value
/// forever, being paid for on every tick.
#[must_use]
pub fn bio_write_due(now: u64, marker: Option<&BioMarker>, text: &str) -> bool {
    let Some(marker) = marker else {
        // Never written here. Write it.
        return true;
    };
    if now.saturating_sub(marker.at) < 3_600 {
        return false;
    }
    marker.text != text
}

/// Writes the bio if the week's state changed and an hour has passed.
///
/// # The three things that stop a write
///
/// 1. **Nothing to say.** `state_of` returns `None` for a voided week, a week
///    past its claim window, and a winner whose handle was never read. The
///    previous bio stands, which was true when it was written.
/// 2. **The same text.** Compared against the marker's contents, so an
///    unchanged week costs nothing. This is the ordinary case: the bio changes
///    at most three times a week.
/// 3. **A failed check.** `bio::check` is the two checks a reply passes. A
///    render that fails is recorded and not written -- and *not written* is the
///    safe direction here in a way it is not for a reply, because there is no
///    previous reply to fall back to and there is always a previous bio.
fn write_bio_if_changed(x: Option<&X>, bio: &crate::bio::Bio, spend: &mut Spend, paths: &Paths) {
    let Some(x) = x else {
        return;
    };
    let at = now();
    let path = format!("{}/bio.last", paths.contest_dir);
    let previous = std::fs::read_to_string(&path).ok();
    let marker = previous.as_deref().and_then(BioMarker::parse);

    let Some(record) = radar_contest::records_in(std::path::Path::new(&paths.contest_dir))
        .into_iter()
        .max_by_key(|r| r.week)
    else {
        return;
    };
    let Some(text) = bio_to_write(bio, &record, marker.as_ref(), at) else {
        return;
    };

    // Metered like everything else a stranger's week can trigger. `Cost::Post`
    // rather than a price of its own: X lists a profile write near a post, and
    // a seventh required price would take this instance silent until somebody
    // set it -- `Prices::from_vars` is all-or-nothing, which is rule 8 working
    // and the wrong trade for an optional feature.
    let Ok(reservation) = spend.authorize(Cost::Post, day_of(at)) else {
        eprintln!("radar-analyst: budget spent; the bio is not written");
        return;
    };
    match x.update_profile(&text) {
        Ok(()) => {
            let charged = reservation.reserved();
            spend.settle(reservation, charged);
            // The marker is the record, and it is written *after* the platform
            // accepted -- a marker written first would make a failed write look
            // like a done one and the bio would then never be retried.
            if let Err(e) = std::fs::write(&path, BioMarker::render(at, &text)) {
                eprintln!("radar-analyst: the bio was written but not recorded: {e}");
            }
        }
        Err(e) => {
            spend.release(reservation);
            eprintln!("radar-analyst: the bio was refused by the platform: {e}");
        }
    }
}

/// Posts the claim prompt for any week whose winner has not been told yet.
///
/// Runs on every tick, not only at close. `try_claim` requires a claim to be a
/// reply to this post, so a week with no prompt on its record accepts no claim
/// at all -- and a prompt that failed to post once would otherwise cost the
/// winner the whole seven days. Retrying is bounded by the claim window.
///
/// The prompt goes under the account's own winning reply, so it arrives in the
/// thread the winner is already in. Its id is written back into the record;
/// until that write happens no claim is possible, which is the safe direction.
///
/// In a dry run the post is recorded and not published, no id comes back,
/// `claim_prompt` stays `None`, and no claim can land -- correct, because no
/// winning reply was published for anyone to have seen either.
/// The post the claim prompt replies to.
///
/// **The winner's own summons, not the bot's winning reply.**
///
/// Since 2026-02-23 X accepts an API reply only when the author of the post
/// being replied to mentioned or quoted the bot in that post. A summons did
/// exactly that by definition, so this is the one reply the platform
/// guarantees. Replying under the account's own post relies instead on an
/// exemption reported by a blog and a developer forum and documented nowhere
/// -- and if it is not real, the winner is never told they won and finds out
/// when the pool rolls over unclaimed.
///
/// It also lands where they will see it: the summons is their own post, so the
/// prompt reaches their notifications rather than a thread they left.
///
/// The winning reply is the fallback, for weeks closed before mention ids were
/// recorded. Those are the weeks the exemption has to hold for, and there is
/// exactly one of them.
///
/// Split out of [`prompt_claim_if_due`] because the match on the winning reply
/// is the whole of it and it is one character from being wrong: CI turned the
/// `==` into a `!=`, which posts the prompt under **a losing entrant's**
/// summons, and nothing failed -- the only test that reached this code had one
/// entrant, so the winner and the first non-winner were the same row.
fn claim_target(ranking: &radar_contest::Ranking, winning_reply: &str) -> String {
    ranking
        .ranked
        .iter()
        .find(|r| r.entry.reply_id == winning_reply)
        .and_then(|r| r.entry.mention_id.clone())
        .unwrap_or_else(|| winning_reply.to_owned())
}

fn prompt_claim_if_due(publisher: &dyn Publisher, spend: &mut Spend, paths: &Paths) {
    let at = now();
    let Some(record) = crate::contest::prompt_due(&paths.contest_dir, at) else {
        return;
    };
    let Some(winner) = record.winner.as_ref() else {
        return;
    };
    let Some(post) = crate::weekly::claim_prompt(&record) else {
        return;
    };

    let under = claim_target(&record.ranking, &winner.reply_id);

    let Ok(reservation) = spend.authorize(Cost::Reply, day_of(at)) else {
        eprintln!(
            "radar-analyst: budget spent; week {} claim prompt not posted, retrying next tick",
            record.week.0
        );
        return;
    };
    match crate::weekly::publish_under(
        publisher,
        &paths.posts,
        &format!("claim:{}", record.week.0),
        Some(&under),
        std::slice::from_ref(&post),
        at,
    ) {
        Ok((sent, first)) => {
            settle_if_sent(spend, reservation, sent);
            if let Some(id) = first {
                let mut updated = record;
                updated.claim_prompt = Some(id);
                if let Err(e) = crate::contest::write_record(&paths.contest_dir, &updated) {
                    // The post went out and the record does not know it. The
                    // next tick posts a second prompt, which is noisy and
                    // recoverable; a claim replying to either one is refused
                    // until a write succeeds, which is the safe failure.
                    eprintln!(
                        "radar-analyst: week {} claim prompt posted but not recorded: {e}",
                        updated.week.0
                    );
                }
            }
        }
        Err(e) => {
            spend.release(reservation);
            eprintln!("radar-analyst: cannot write {}: {e}", paths.posts);
        }
    }
}

/// What the process says about itself on start: the commit, and how many
/// accounts the contest excludes.
///
/// A function rather than an `eprintln!` inside `run`, for the reason
/// [`unfunded_notice`] gives: `run` never returns and nothing inside it can be
/// tested.
///
/// The count, never the ids. The ids are the operator's other accounts and the
/// number is the thing worth checking -- "operators: 1" on a box whose
/// `analyst.env` names a second account is the whole of finding Phase 0.3, and
/// it is invisible any other way.
#[must_use]
pub fn build_notice(operators: usize) -> String {
    format!(
        "radar-analyst: build {}; operators: {operators} {}.",
        radar_types::build_sha_or_unknown(),
        if operators == 1 { "id" } else { "ids" }
    )
}

/// Every account the operator controls, for the contest's exclusion rule.
///
/// The bot's own id is always included, so an unset or mistyped
/// `RADAR_CONTEST_OPERATORS` can never make the bot itself eligible -- the
/// failure this ordering exists to prevent. Everything else is additive.
///
/// Ids only: anything that is not a run of digits is dropped, because an X
/// account id is a number and a handle pasted here would silently never match
/// the `summoner` field, which carries an id.
fn operator_ids(x: Option<&X>) -> Vec<String> {
    let own = x.map_or_else(|| "radar".to_owned(), |x| x.user_id().to_owned());
    operator_ids_from(
        &own,
        std::env::var("RADAR_CONTEST_OPERATORS").ok().as_deref(),
    )
}

/// The same, with the listed value supplied rather than read.
///
/// Split out so the rule can be tested without setting a process-wide variable
/// — the pattern `Paths::from_vars` already uses, for the same reason.
#[must_use]
fn operator_ids_from(own: &str, listed: Option<&str>) -> Vec<String> {
    let mut ids = vec![own.to_owned()];
    let Some(listed) = listed else {
        return ids;
    };
    ids.extend(
        listed
            .split(',')
            .map(|id| {
                id.trim()
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect::<String>()
            })
            // An empty id is what a stray comma, a blank entry or a pasted
            // handle collapses to, and an empty string in the set would make
            // `is_operator("")` true. No summoner is empty today, so this is
            // belt and braces — but the belt costs one `!`, and the failure it
            // prevents is an entrant silently excluded from a prize.
            .filter(|id| !id.is_empty()),
    );
    ids
}

/// Sleeps rather than exiting, so a misconfigured unit is visible as a running
/// service that says what is missing rather than as a restart loop.
fn idle_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3_600));
    }
}

/// One poll, and everything it found. Returns how many mentions were answered.
///
/// Public so a test can drive exactly one against a fake platform. The loop in
/// [`run`] is this function and a sleep.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn tick(
    x: Option<&X>,
    publisher: &dyn Publisher,
    gate: &mut Gate,
    spend: &mut Spend,
    client: &radar_onchain::RpcClient,
    rates: Option<&BaseRates>,
    creators: Option<&radar_roast::CreatorIndex>,
    provider: Option<&dyn radar_model::Provider>,
    self_mint: Option<&Address>,
    paths: &Paths,
) -> usize {
    let Some(x) = x else {
        return 0;
    };
    let at = now();
    let today = day_of(at);

    // The read is billable before it happens.
    let Ok(read) = spend.authorize(Cost::MentionRead, today) else {
        eprintln!("radar-analyst: budget spent; not polling");
        return 0;
    };

    let cursor = poll::read_cursor(&paths.cursor);
    let mentions = match x.mentions(cursor.as_deref()) {
        Ok(m) => {
            // Settled at what was reserved. The platform does not report a
            // per-call charge on the response, so the list price is the best
            // available actual -- and settling at anything less would quietly
            // hand the budget back. Settling at zero, which an earlier draft of
            // this did, makes the meter decorative.
            let charged = read.reserved();
            spend.settle(read, charged);
            m
        }
        Err(e) => {
            // Nothing was delivered, so nothing is charged.
            spend.release(read);
            eprintln!("radar-analyst: mentions poll failed: {e}");
            return 0;
        }
    };

    let mut answered = 0;
    // **The cursor advances only over mentions this loop actually finished.**
    //
    // It used to be computed from the whole page, whatever happened inside. So
    // a log write that failed on the second of five mentions broke the loop --
    // correctly, an account that cannot record what it says must stop saying
    // things -- and then advanced the cursor past all five. The remaining three
    // were never polled again. The module doc says the log-before-post rule
    // exists so nothing is published unrecorded; dropping the mention entirely
    // is the other half of the same failure, and it was silent.
    let mut handled: Vec<&str> = Vec::new();
    for mention in &mentions {
        // A winner naming an address inside the claim window is claiming, not
        // summoning. Checked first, and at the cost of a directory listing
        // only: the claim is written into the record and the mention is not
        // answered, because a wallet is not a coin.
        if let Some(week) = crate::contest::try_claim(mention, &paths.contest_dir, at, |a| {
            // One `getAccountInfo`, on its own budget: a claim is one mention
            // and must not spend the dossier allowance of the summons behind
            // it in the queue.
            let mut budget = CallBudget::default();
            client.owner_of(&mut budget, a).map_err(|e| e.to_string())
        }) {
            eprintln!(
                "radar-analyst: {} -> claim recorded for week {}",
                mention.id, week.0
            );
            handled.push(&mention.id);
            continue;
        }

        // The model call is the one thing a stranger can make this account
        // spend that nothing charged for until now. Reserved **before**
        // `answer`, because `answer` makes the call internally: by the time a
        // reply comes back the money is gone, and a ceiling checked after that
        // is not a ceiling.
        //
        // A refusal here does not refuse the mention. The day's model budget
        // being spent means this one reply is the deterministic template --
        // which is exactly what the account ships with no provider configured
        // at all, so it is rule 8 rather than an outage.
        let reserved = provider.and_then(|_| {
            spend
                .authorize(Cost::ModelCall, today)
                .inspect_err(|_| {
                    eprintln!(
                        "radar-analyst: model budget spent; {} answered by the template",
                        mention.id
                    );
                })
                .ok()
        });
        let ctx = Answering {
            client,
            rates,
            creators,
            // Gated on the reservation, so a refused meter means no call was
            // made rather than one that was made unmetered.
            provider: if reserved.is_some() { provider } else { None },
            self_mint,
            now: at,
        };

        let outcome = crate::answer::answer(mention, gate, &ctx);
        // Settled here rather than inside the arms below. The money is already
        // spent by this point, and the reply's own reservation is a separate
        // ceiling that can be refused -- one must not hold the other open.
        if let Some(commitment) = reserved {
            match outcome.billed() {
                Billed::NoCall => spend.release(commitment),
                Billed::Reported(actual) => spend.settle(commitment, actual),
                // Rule 9. What was reserved is the honest charge for a cost
                // nobody reported. Zero is not.
                Billed::Unreported => {
                    let charged = commitment.reserved();
                    spend.settle(commitment, charged);
                }
            }
        }

        match outcome {
            Answered::Reply { entry, .. } => {
                let mint = entry.mint.clone().unwrap_or_default();
                let Ok(reply_cost) = spend.authorize(Cost::Reply, today) else {
                    // `break`, not `continue`. The day's reply budget is spent,
                    // so every mention behind this one would be refused for the
                    // same reason -- and `continue` advanced the cursor over
                    // each of them, which is a mention answered by nobody, ever.
                    // Stopping leaves the cursor where it is, so they are polled
                    // again when the budget rolls.
                    //
                    // The sheet is written down before stopping. It was built
                    // and paid for; discarding it loses the evidence for a
                    // decision the account actually made, which is the one
                    // thing `log` exists to prevent.
                    eprintln!(
                        "radar-analyst: reply budget spent; {} not answered, and \
                         nothing behind it is polled past",
                        mention.id
                    );
                    if let Err(e) = crate::log::append(&paths.log, &entry) {
                        eprintln!("radar-analyst: cannot write {}: {e}", paths.log);
                    }
                    if let Err(e) = crate::contest::append_refusal(
                        &paths.refusals,
                        &crate::contest::RefusalLine {
                            at,
                            summoner: mention.author.clone(),
                            why: "the day's reply budget is spent".to_owned(),
                            // Not a gate refusal at all, and deliberately not
                            // dressed as one: `kind` is `None`, which the
                            // week-close job reads as unknown and excludes.
                            // Nobody should lose a week because the operator's
                            // budget ran out.
                            kind: None,
                        },
                    ) {
                        eprintln!("radar-analyst: cannot write {}: {e}", paths.refusals);
                    }
                    break;
                };
                match crate::publish::publish(publisher, &paths.log, *entry) {
                    Ok(written) => {
                        handled.push(&mention.id);
                        if let Some(id) = &written.reply_id {
                            let charged = reply_cost.reserved();
                            spend.settle(reply_cost, charged);
                            gate.record(&mention.author, &mint, id, at);
                            answered += 1;
                        } else {
                            // Nothing was published, so nothing is charged and
                            // the gate is not told: a broken publisher must not
                            // silence the account by spending an allowance it
                            // never used.
                            spend.release(reply_cost);
                        }
                    }
                    Err(e) => {
                        spend.release(reply_cost);
                        // An account that cannot record what it says must not
                        // carry on saying things.
                        eprintln!("radar-analyst: cannot write {}: {e}", paths.log);
                        break;
                    }
                }
            }
            // **A symbol gets an answer.** `$DOGE` names nothing on chain --
            // symbols are not unique, and guessing which token one meant is how
            // a measurement gets published about the wrong project. Saying
            // exactly that is the honest reply and the best content available.
            //
            // Design 0009 asks for it and nothing called it: the daemon printed
            // the text to its own terminal and the person who asked got
            // silence, which reads as an account that ignores you. `answer` now
            // gates these on the symbol, so answering them does not put one
            // reply shape outside every cap in this module.
            Answered::Ticker { key, text } => {
                let Ok(reply_cost) = spend.authorize(Cost::Reply, today) else {
                    eprintln!(
                        "radar-analyst: reply budget spent; {} not answered",
                        mention.id
                    );
                    break;
                };
                let entry = crate::log::Entry {
                    at,
                    mention_id: mention.id.clone(),
                    summoner: mention.author.clone(),
                    // No mint, and that is the *content* of the reply rather
                    // than a gap in the record: a symbol identifies nothing.
                    mint: None,
                    read_at_slot: None,
                    // Nothing was read, so there is no evidence to carry. An
                    // empty sheet beside a reply that states no fact about a
                    // coin is the honest pairing; inventing one would put a
                    // measurement next to a reply that made none.
                    fact_sheet: String::new(),
                    reply: text,
                    fellback: None,
                    reply_id: None,
                    signals: Some(Vec::new()),
                    pointed_at: None,
                };
                match crate::publish::publish(publisher, &paths.log, entry) {
                    Ok(written) => {
                        handled.push(&mention.id);
                        if let Some(id) = &written.reply_id {
                            let charged = reply_cost.reserved();
                            spend.settle(reply_cost, charged);
                            // Against the same key `answer` admitted on, which
                            // is why the key is carried on the variant rather
                            // than re-derived here: two derivations of one key
                            // is how a dedupe map fills with entries nothing
                            // ever looks up.
                            gate.record(&mention.author, &key, id, at);
                            answered += 1;
                        } else {
                            spend.release(reply_cost);
                        }
                    }
                    Err(e) => {
                        spend.release(reply_cost);
                        eprintln!("radar-analyst: cannot write {}: {e}", paths.log);
                        break;
                    }
                }
            }
            other => {
                // Refusals, symbols and unreadable chains cost nothing and are
                // not published. They are still worth a line: an account that
                // answers nothing should say what it is seeing.
                eprintln!("radar-analyst: {} -> {other:?}", mention.id);
                handled.push(&mention.id);

                // **A duplicate gets pointed at the answer.**
                //
                // `AlreadyAnswered` has carried the existing reply's id since
                // the day it was written and nothing ever read it: the daemon
                // printed the refusal and the asker got nothing back. Two costs,
                // and the second is the one that matters. A real person asking
                // about a coin somebody else asked about ten minutes ago was
                // answered with silence -- and because the *first* asker's
                // reply is their contest entry, a script that asks first about
                // every trending launch owns the entry on the hottest coins. The
                // contest rewarded speed and automation, which is the exact
                // behaviour this account exists to expose.
                //
                // A pointer costs one post: no model call, no chain read. It is
                // recorded in the reply log like every other public statement,
                // carrying `pointed_at` so the restore can tell it from an
                // answer -- see `log::Entry::pointed_at`.
                //
                // A refused reply budget is not a failure here. The pointer is
                // a courtesy; skipping it leaves the refusal recorded and the
                // loop moving, which is why this is an `if let` chain rather
                // than a `break`.
                if let Answered::Refused(Refused::AlreadyAnswered { reply_id }) = &other
                    && let Ok(reply_cost) = spend.authorize(Cost::Reply, today)
                {
                    let entry = crate::log::Entry {
                        at,
                        mention_id: mention.id.clone(),
                        summoner: mention.author.clone(),
                        mint: None,
                        read_at_slot: None,
                        fact_sheet: String::new(),
                        reply: pointer_reply(reply_id),
                        fellback: None,
                        reply_id: None,
                        signals: Some(Vec::new()),
                        pointed_at: Some(reply_id.clone()),
                    };
                    match crate::publish::publish(publisher, &paths.log, entry) {
                        Ok(written) => {
                            if written.reply_id.is_some() {
                                let charged = reply_cost.reserved();
                                spend.settle(reply_cost, charged);
                                // The day's allowance and an hourly token, and
                                // nothing else: not the summoner's, which the
                                // gate refused before charging, and not the
                                // dedupe map, which already holds the answer
                                // this points at.
                                gate.record_post(at);
                            } else {
                                spend.release(reply_cost);
                            }
                        }
                        Err(e) => {
                            spend.release(reply_cost);
                            eprintln!("radar-analyst: cannot write {}: {e}", paths.log);
                            break;
                        }
                    }
                }
                // A gate refusal is also a fact the contest needs: an account
                // refused during the week does not win it. Appended, never
                // fatal -- a refusal that could not be written is a line on
                // the terminal, and the reply loop carries on.
                if let Answered::Refused(why) = &other
                    && let Err(e) = crate::contest::append_refusal(
                        &paths.refusals,
                        &crate::contest::RefusalLine {
                            at,
                            summoner: mention.author.clone(),
                            why: crate::answer::describe(why),
                            kind: Some(crate::contest::RefusalKind::of(why)),
                        },
                    )
                {
                    eprintln!("radar-analyst: cannot write {}: {e}", paths.refusals);
                }
            }
        }
    }

    if let Some(next) = poll::next_cursor(handled.iter().copied(), cursor.as_deref())
        && let Err(e) = poll::write_cursor(&paths.cursor, &next)
    {
        // Not fatal, and loud: the next start re-reads from the old cursor,
        // which the gate's dedupe absorbs.
        eprintln!("radar-analyst: cannot save the cursor: {e}");
    }
    if let Err(e) = spend.save() {
        eprintln!("radar-analyst: cannot save the ledger: {e}");
    }
    answered
}

#[cfg(test)]
mod tests {

    /// A closed week with a winner who has a handle, for the bio planner.
    fn bio_record() -> radar_contest::Record {
        let entry = radar_contest::Entry {
            reply_id: "r1".to_owned(),
            summoner: "9001".to_owned(),
            mention_id: Some("m1".to_owned()),
            handle: Some("somebody".to_owned()),
            mint: "M".to_owned(),
            at: radar_contest::Week(2958).opens_at() + 10,
            metrics: radar_contest::Metrics::default(),
        };
        radar_contest::Record::close(
            radar_contest::Week(2958),
            radar_contest::Ranking {
                ranked: vec![radar_contest::Ranked { entry, score: 12 }],
                excluded: Vec::new(),
            },
            &radar_contest::Rules::published(["op"]),
        )
    }

    #[test]
    fn the_bio_planner_holds_every_reason_not_to_write() {
        // `write_bio_if_changed` needs an X client and a filesystem, so nothing
        // in it can be reached by a test -- and CI proved that three times,
        // killing no mutant of any comparison that lived there. Every decision
        // is here now and the writer has none left.
        let bio = crate::bio::Bio {
            lead: "Automated.".to_owned(),
        };
        let record = bio_record();
        let closed = radar_contest::Week(2958).closes_at();

        // Nothing written yet: write it, and it says what the record says.
        let first = bio_to_write(&bio, &record, None, closed + 60).expect("a bio");
        assert!(first.starts_with("Automated."), "{first}");
        assert!(first.contains("@somebody"), "{first}");
        assert!(first.contains("2026-09-21"), "{first}");

        // The same text an hour later is not a write.
        let marker = BioMarker {
            at: closed + 60,
            text: first.clone(),
        };
        assert_eq!(
            bio_to_write(&bio, &record, Some(&marker), closed + 60 + 7_200),
            None,
            "unchanged"
        );

        // Too soon is not a write either, even with something new to say.
        let stale = BioMarker {
            at: closed + 60,
            text: "something else".to_owned(),
        };
        assert_eq!(
            bio_to_write(&bio, &record, Some(&stale), closed + 60 + 60),
            None,
            "59 minutes"
        );
        assert!(
            bio_to_write(&bio, &record, Some(&stale), closed + 60 + 3_600).is_some(),
            "an hour later, with new text"
        );

        // A record with nothing to say writes nothing, however long it has been.
        let mut voided = bio_record();
        voided.voided = Some(radar_contest::ledger::Voided {
            at: closed,
            reason: "bought".to_owned(),
        });
        assert_eq!(bio_to_write(&bio, &voided, None, closed + 60), None);

        // A lead that leaves no room writes nothing rather than a bio the
        // platform would cut mid-figure.
        let long = crate::bio::Bio {
            lead: "x".repeat(crate::bio::MAX - 10),
        };
        assert_eq!(bio_to_write(&long, &record, None, closed + 60), None);
    }

    #[test]
    fn the_bio_writer_is_off_unless_a_lead_is_written_and_says_which() {
        // A bio write overwrites the only copy of whatever the account says.
        // "It is being written and you did not know" is the state worth being
        // told about on a restart, so both are said.
        let off = bio_notice(None);
        assert!(off.contains("not written"), "{off}");
        assert!(off.contains("RADAR_BIO_LEAD"), "{off}");

        let on = bio_notice(Some(&crate::bio::Bio {
            lead: "Automated.".to_owned(),
        }));
        assert!(on.contains("IS written"), "{on}");
        assert!(on.contains("will be replaced"), "{on}");
        assert!(on.contains("Automated."), "{on}");
    }

    #[test]
    fn the_bio_is_written_at_most_hourly_and_only_when_the_text_changed() {
        // Two guards on a metered endpoint that overwrites the only copy of a
        // public profile, and CI killed neither until they moved out of a
        // function that needs a platform to run.
        //
        // Inverting the freshness guard writes ONLY when it is too soon.
        // Inverting the text comparison writes ONLY when nothing changed --
        // a bio frozen at its first value forever, paid for on every tick.
        let then = 1_000_000;
        let old = BioMarker {
            at: then,
            text: "old".to_owned(),
        };

        assert!(bio_write_due(then, None, "anything"), "never written here");
        assert!(!bio_write_due(then, Some(&old), "new"), "just written");
        assert!(
            !bio_write_due(then + 3_599, Some(&old), "new"),
            "59 minutes"
        );
        assert!(
            bio_write_due(then + 3_600, Some(&old), "new"),
            "on the hour"
        );
        // The whole point of the second guard: an hour has passed and the text
        // is the same, so there is nothing to pay for.
        assert!(
            !bio_write_due(then + 7_200, Some(&old), "old"),
            "unchanged text is not a write"
        );
        // A marker from the future does not unlock it -- a clock that went
        // backwards must not turn this into every tick.
        assert!(!bio_write_due(1_000, Some(&old), "new"));
    }

    #[test]
    fn a_marker_that_will_not_parse_means_write_again() {
        // The safe direction: the alternative is a torn marker freezing the
        // bio at whatever it happened to say, with nothing saying why.
        assert_eq!(BioMarker::parse(""), None);
        assert_eq!(BioMarker::parse("no newline"), None);
        assert_eq!(BioMarker::parse("not-a-number\ntext"), None);

        let parsed = BioMarker::parse("1700000000\nthe bio").expect("parses");
        assert_eq!(parsed.at, 1_700_000_000);
        assert_eq!(parsed.text, "the bio");
        // Round-trips, so what was written is what is compared next time. A
        // render that did not match the parser would make every tick a write.
        assert_eq!(
            BioMarker::parse(&BioMarker::render(parsed.at, &parsed.text)),
            Some(parsed)
        );
    }

    /// A funded meter on its own ledger file, so these tests do not share one.
    fn a_spend() -> Spend {
        let dir = std::env::temp_dir().join(format!(
            "radar-daemon-thread-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let ledger = dir.join("ledger.json").to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&ledger);
        Spend::open(
            Budget {
                per_call_max: MicroUsd(50_000),
                daily_max: MicroUsd(1_000_000),
            },
            Prices {
                mention_read: MicroUsd(1_000),
                post_read: MicroUsd(5_000),
                reply: MicroUsd(10_000),
                post: MicroUsd(15_000),
                model_call: MicroUsd(2_000),
                user_read: MicroUsd(20_000),
            },
            ledger,
            1,
        )
    }

    #[test]
    fn a_thread_is_priced_as_a_post_and_a_reply_for_each_one_after_it() {
        // `announce_week` charged ONE `Cost::Post` for the whole thread until
        // 2026-09-06 -- already short by the teardown, and short by two once
        // the hunters post joined it. A meter that under-reports the one
        // billable thing a stranger can trigger is worse than no meter,
        // because the daily cap is computed from it.
        let mut spend = a_spend();
        let one = reserve_thread(&mut spend, 1, 1);
        let before = spend.spent_today();
        assert_eq!(one.len(), 1);

        // A post, and a reply for each one after it. The exact total, not
        // "more than one": CI turned the `n == 0` into `n != 0`, which prices
        // the *first* post as a reply and every one after it as a post -- a
        // different, larger number that still passes any "more than" check.
        assert_eq!(before, MicroUsd(15_000), "one post is one Post");

        let mut spend = a_spend();
        let three = reserve_thread(&mut spend, 3, 1);
        assert_eq!(three.len(), 3);
        assert_eq!(
            spend.spent_today(),
            MicroUsd(15_000 + 10_000 + 10_000),
            "a post and two replies"
        );
    }

    #[test]
    fn a_thread_the_budget_only_half_covers_says_so_and_says_nothing_otherwise() {
        // `>` is the dangerous mutation of this comparison: it prints a
        // shortfall on every ordinary week and stays silent on the one week
        // that actually lost a post -- an alarm that fires when nothing is
        // wrong and not when something is.
        assert_eq!(
            short_thread_notice(3, 3),
            None,
            "the ordinary week is silent"
        );
        assert_eq!(short_thread_notice(1, 1), None);
        let short = short_thread_notice(2, 3).expect("a shortfall is said");
        assert!(short.contains("2 of the week's 3"), "{short}");
        // A thread whose reservations somehow exceed its posts is not a
        // shortfall, and must not be announced as one.
        assert_eq!(short_thread_notice(4, 3), None);
    }

    #[test]
    fn a_thread_that_lands_in_part_is_charged_for_the_part_that_landed() {
        // `publish_under` stops the thread when the platform refuses a post,
        // so a partial thread is an ordinary state. Charging for the planned
        // posts would spend the day's budget on posts nobody received --
        // which is the failure `settle_if_sent` was written to prevent for
        // one post, applied per post here.
        let mut spend = a_spend();
        let three = reserve_thread(&mut spend, 3, 1);
        let reserved = spend.spent_today();
        settle_thread(&mut spend, three, 1);
        let after_one = spend.spent_today();
        assert!(after_one < reserved, "{after_one:?} vs {reserved:?}");

        // And a thread where nothing landed costs nothing.
        let mut spend = a_spend();
        let three = reserve_thread(&mut spend, 3, 1);
        settle_thread(&mut spend, three, 0);
        assert_eq!(spend.spent_today(), radar_types::MicroUsd::ZERO);
    }

    #[test]
    fn a_contest_directory_that_cannot_be_written_is_said_at_start_not_at_midnight() {
        // The analyst writes the contest's records once a week. A deployment
        // that grants write to the analyst's directory and not the contest's
        // looks perfect for six days and then cannot close the week -- which
        // is what `deploy/radar-analyst.service` did until 2026-09-07, with
        // the only symptom a line in a journal at midnight.
        //
        // Re-apply by returning `None` unconditionally: the second assertion
        // fails, and the box goes back to finding out on a Monday.
        let dir = std::env::temp_dir().join(format!("radar-contest-w{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ok = dir.to_string_lossy().into_owned();
        assert_eq!(
            contest_writable_notice(&ok),
            None,
            "a directory it can create is writable"
        );
        // The probe cleans up after itself: a stray file in the contest
        // directory is a file `records_in` has to skip.
        assert!(!dir.join(".radar-write-probe").exists());

        // A path under a file is the closest thing to a read-only mount that a
        // test can produce without one: the create fails for a different
        // reason, and what is asserted is that a failure to write is *said*.
        let blocked = dir.join("a-file").to_string_lossy().into_owned();
        std::fs::write(&blocked, b"not a directory").expect("write");
        let notice = contest_writable_notice(&format!("{blocked}/contest"))
            .expect("a directory it cannot write is named");
        assert!(notice.contains("CANNOT BE WRITTEN"), "{notice}");
        // The directive, not the symptom: whoever reads this has a unit file
        // open, not an strace.
        assert!(notice.contains("ReadWritePaths"), "{notice}");
        assert!(notice.contains("sibling"), "{notice}");
    }

    /// One entrant, with the summons the claim prompt should reply to.
    fn entrant(reply_id: &str, mention_id: &str) -> radar_contest::Ranked {
        radar_contest::Ranked {
            entry: radar_contest::Entry {
                reply_id: reply_id.to_owned(),
                summoner: format!("s{reply_id}"),
                mention_id: Some(mention_id.to_owned()),
                handle: None,
                mint: "M".to_owned(),
                at: 0,
                metrics: radar_contest::Metrics::default(),
            },
            score: 0,
        }
    }

    #[test]
    fn the_claim_prompt_goes_under_the_winners_summons_and_nobody_elses() {
        // CI turned this `==` into a `!=` and nothing failed, because the only
        // test reaching the code had one entrant -- so the winner and the first
        // non-winner were the same row. With two, the inverted match posts the
        // prompt under a *loser's* summons: the winner is never told, and the
        // pool rolls over unclaimed while somebody who did not win is invited
        // to claim it.
        let ranking = radar_contest::Ranking {
            ranked: vec![
                entrant("won", "winners-summons"),
                entrant("lost", "someone-elses"),
            ],
            excluded: Vec::new(),
        };
        assert_eq!(claim_target(&ranking, "won"), "winners-summons");

        // The order does not decide it either: the winner is matched by reply
        // id, not by being first.
        let reversed = radar_contest::Ranking {
            ranked: vec![
                entrant("lost", "someone-elses"),
                entrant("won", "winners-summons"),
            ],
            excluded: Vec::new(),
        };
        assert_eq!(claim_target(&reversed, "won"), "winners-summons");
    }

    #[test]
    fn a_week_closed_before_mention_ids_falls_back_to_the_winning_reply() {
        // The one week that exists. Its entries have no mention id, so the
        // prompt goes under the bot's own reply and relies on the undocumented
        // exemption -- which is the situation this fallback exists to describe,
        // not to prefer.
        let mut only = entrant("won", "unused");
        only.entry.mention_id = None;
        let ranking = radar_contest::Ranking {
            ranked: vec![only],
            excluded: Vec::new(),
        };
        assert_eq!(claim_target(&ranking, "won"), "won");

        // And a winner who is not in the ranking at all -- which should not
        // happen -- gets the reply id rather than a stranger's summons.
        let others = radar_contest::Ranking {
            ranked: vec![entrant("lost", "someone-elses")],
            excluded: Vec::new(),
        };
        assert_eq!(claim_target(&others, "won"), "won");
    }

    #[test]
    fn the_start_line_carries_the_commit_and_the_operator_count() {
        // Both are states an operator otherwise infers. "operators: 1" on a box
        // whose analyst.env names a second account is the whole of Phase 0.3,
        // and it is invisible any other way.
        let one = build_notice(1);
        assert!(one.contains("operators: 1 id."), "{one}");
        assert!(!one.contains("1 ids"), "{one}");
        assert!(
            build_notice(2).contains("operators: 2 ids"),
            "{}",
            build_notice(2)
        );

        // An ordinary build has no `RADAR_BUILD_SHA` and says so rather than
        // printing a blank, which reads as neither a commit nor an absence.
        // Release CI sets it, so this asserts the shape and not the value.
        assert!(one.contains("build "), "{one}");
        assert!(!one.contains("build ;"), "{one}");
    }
    #[test]
    fn the_operator_set_always_holds_the_bots_own_id_and_drops_empty_ones() {
        use super::operator_ids_from;

        // Unset: the bot is still excluded. This ordering is the point -- a
        // missing variable must never make the bot eligible for its own prize.
        assert_eq!(operator_ids_from("111", None), vec!["111".to_owned()]);

        // Listed ids are additive.
        assert_eq!(
            operator_ids_from("111", Some("222,333")),
            vec!["111".to_owned(), "222".to_owned(), "333".to_owned()]
        );

        // Whitespace and a handle pasted where an id belongs. `summoner` is a
        // numeric id, so a handle would silently never match; it collapses to
        // empty and is dropped rather than sitting in the set as "".
        //
        // Re-apply by deleting the `!` in the filter and the last assertion
        // fails: "" enters the set and `is_operator("")` becomes true.
        assert_eq!(
            operator_ids_from("111", Some(" 222 , , @thecabalhunter , 333 ")),
            vec!["111".to_owned(), "222".to_owned(), "333".to_owned()]
        );
        assert!(
            !operator_ids_from("111", Some(",,")).contains(&String::new()),
            "an empty id must never enter the set"
        );
    }

    use super::*;

    #[test]
    fn a_post_that_left_is_charged_and_one_that_did_not_is_given_back() {
        // Re-applied as CI did: `>` to `==` charges the dry run and refunds the
        // real post, and both assertions below fail.
        let dir = std::env::temp_dir().join(format!("radar-daemon-settle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let ledger = dir.join("ledger.json").to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&ledger);
        let prices = Prices {
            mention_read: MicroUsd(1_000),
            post_read: MicroUsd(5_000),
            reply: MicroUsd(10_000),
            post: MicroUsd(15_000),
            model_call: MicroUsd(2_000),
            user_read: MicroUsd(20_000),
        };
        let mut spend = Spend::open(
            Budget {
                per_call_max: MicroUsd(50_000),
                daily_max: MicroUsd(1_000_000),
            },
            prices,
            ledger,
            1,
        );
        let reservation = spend.authorize(Cost::Post, 1).expect("authorised");
        settle_if_sent(&mut spend, reservation, 0);
        assert_eq!(
            spend.spent_today(),
            MicroUsd::ZERO,
            "nothing left, nothing charged"
        );

        let reservation = spend.authorize(Cost::Post, 1).expect("authorised");
        settle_if_sent(&mut spend, reservation, 2);
        assert_eq!(
            spend.spent_today(),
            MicroUsd(15_000),
            "a thread that left is one post's price"
        );
    }

    #[test]
    fn either_lane_finding_something_keeps_the_loop_busy() {
        // Re-applied as CI did: `+` to `*` makes (0, 1) idle and the second
        // assertion fails; `+` to `-` panics on (0, 1) and the test fails there.
        assert_eq!(next_wait(1, 0, poll::IDLE), poll::BUSY);
        assert_eq!(next_wait(0, 1, poll::IDLE), poll::BUSY);
        assert_eq!(next_wait(2, 3, poll::IDLE), poll::BUSY);
        // Nothing found on either lane: the wait doubles from where it was.
        assert_eq!(next_wait(0, 0, poll::BUSY), poll::BUSY * 2);
    }

    /// A getter over a fixed table, so the rules can be tested without touching
    /// process-wide environment variables.
    fn from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key: &str| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn a_day_is_whole_days_since_the_epoch() {
        // The meter's window and the gate's are the same day, so this arithmetic
        // decides when both reset. Getting it wrong by an operator means a
        // budget that resets hourly or never.
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(86_399), 0, "one second before the boundary");
        assert_eq!(day_of(86_400), 1, "the boundary itself");
        assert_eq!(day_of(86_401), 1);
        assert_eq!(day_of(1_788_000_000), 20_694);
    }

    #[test]
    fn a_budget_needs_both_halves_or_it_is_closed() {
        // Deny by default: a per-call ceiling with no daily cap is not a
        // budget, it is a ceiling on how fast an unbounded bill accumulates.
        let both = from(&[
            ("RADAR_ANALYST_DAILY_USD", "5.00"),
            ("RADAR_ANALYST_PER_CALL_USD", "0.25"),
        ]);
        let budget = budget_from(&both);
        assert_eq!(budget.daily_max, MicroUsd(5_000_000));
        assert_eq!(budget.per_call_max, MicroUsd(250_000));

        for partial in [
            vec![("RADAR_ANALYST_DAILY_USD", "5.00")],
            vec![("RADAR_ANALYST_PER_CALL_USD", "0.25")],
            vec![],
        ] {
            assert_eq!(
                budget_from(&from(&partial)),
                Budget::CLOSED,
                "{partial:?} must not be a budget"
            );
        }
    }

    #[test]
    fn a_ceiling_that_will_not_parse_is_closed_rather_than_ignored() {
        // A typo in a spending ceiling must not read as permission.
        let typo = from(&[
            ("RADAR_ANALYST_DAILY_USD", "five dollars"),
            ("RADAR_ANALYST_PER_CALL_USD", "0.25"),
        ]);
        assert_eq!(budget_from(&typo), Budget::CLOSED);
    }

    #[test]
    fn an_unfunded_instance_is_told_why_it_is_answering_nothing() {
        // An operator who mistypes a ceiling gets a service that answers
        // nothing, and this line is the difference between that and a mystery.
        let notice = unfunded_notice(Budget::CLOSED).expect("a closed budget says so");
        assert!(notice.contains("unfunded"), "{notice}");
        assert!(notice.contains("RADAR_ANALYST_DAILY_USD"), "{notice}");

        // A funded one says nothing: a warning that fires when everything is
        // fine is a warning nobody reads.
        assert_eq!(
            unfunded_notice(Budget {
                per_call_max: MicroUsd(250_000),
                daily_max: MicroUsd(5_000_000),
            }),
            None
        );
    }

    #[test]
    fn absent_limits_refuse_everything() {
        // Zero is the refusing value in `Gate`, so unset means nobody is
        // answered. A default here would be a policy invented by whoever typed
        // it.
        let limits = limits_from(&from(&[]));
        assert_eq!(limits.per_summoner_daily, 0);
        assert_eq!(limits.global_daily, 0);
        // Except the dedupe window, which is not a spending decision: zero
        // would answer the same coin again on the very next poll.
        assert_eq!(limits.dedupe_seconds, 3_600);
    }

    #[test]
    fn limits_are_read_when_they_are_set() {
        let set = from(&[
            ("RADAR_ANALYST_PER_SUMMONER_DAILY", "3"),
            ("RADAR_ANALYST_GLOBAL_DAILY", "50"),
            ("RADAR_ANALYST_DEDUPE_SECONDS", "900"),
        ]);
        let limits = limits_from(&set);
        assert_eq!(limits.per_summoner_daily, 3);
        assert_eq!(limits.global_daily, 50);
        assert_eq!(limits.dedupe_seconds, 900);
    }

    #[test]
    fn the_self_mint_is_none_when_unset_or_blank_and_read_when_it_is_an_address() {
        // Unset and blank both mean no token is special. The token does not
        // exist until the launch gate is met, so for now the correct
        // configuration is none, and it must not be reported as an error.
        assert!(matches!(self_mint_from(&from(&[])), Ok(None)));
        assert!(matches!(
            self_mint_from(&from(&[("RADAR_SELF_MINT", "   ")])),
            Ok(None)
        ));

        // A real address, with the whitespace an env file leaves around it.
        let mint = Address::new([3u8; 32]);
        let padded = format!("  {mint}  ");
        let read = self_mint_from(&from(&[("RADAR_SELF_MINT", padded.as_str())]));
        assert!(
            matches!(read, Ok(Some(m)) if m == mint),
            "the mint must round-trip"
        );
    }

    #[test]
    fn a_self_mint_that_is_not_an_address_is_an_error_and_not_none() {
        // The direction that matters. `None` means the rule is off, so a typo
        // that read as `None` would have the analyst state its own price while
        // every log line said the rule was configured. Re-apply the bug by
        // mapping the parse failure to `Ok(None)` and this fails.
        match self_mint_from(&from(&[("RADAR_SELF_MINT", "not-a-mint")])) {
            Err(e) => {
                assert!(e.contains("RADAR_SELF_MINT"), "{e}");
                assert!(e.contains("nothing is answered"), "{e}");
                // Not echoed: the likeliest wrong value is another variable's
                // secret on the wrong line.
                assert!(!e.contains("not-a-mint"), "{e}");
            }
            Ok(m) => panic!("an unparseable mint must not be accepted as {m:?}"),
        }
    }

    #[test]
    fn the_self_mint_notice_names_the_mint_or_says_there_is_none() {
        // One line in the journal that says whether the rule is armed, and for
        // which token. Each state says something only it could say.
        let none = self_mint_notice(None);
        assert!(none.contains("no RADAR_SELF_MINT"), "{none}");
        assert!(none.contains("same rule"), "{none}");

        let mint = Address::new([3u8; 32]);
        let some = self_mint_notice(Some(&mint));
        assert!(some.contains(&mint.to_string()), "{some}");
        assert!(some.contains("never stated"), "{some}");
        assert!(!some.contains("no RADAR_SELF_MINT"), "{some}");
    }

    #[test]
    fn the_paths_are_all_under_one_directory() {
        // An operator moves one thing, and the unit grants write access to one
        // path. A file that escaped this directory would be a file the service
        // is not permitted to write.
        let paths = Paths::under("/var/lib/radar/analyst");
        for path in [&paths.log, &paths.cursor, &paths.ledger] {
            assert!(
                path.starts_with("/var/lib/radar/analyst/"),
                "{path} escapes the directory"
            );
        }
        // And they are three different files, not one name used three times.
        assert_ne!(paths.log, paths.cursor);
        assert_ne!(paths.cursor, paths.ledger);
        assert_ne!(paths.log, paths.ledger);
    }

    /// A getter over a fixed list, so nothing touches the process environment.
    fn vars<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k: &str| {
            pairs
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn speaking_in_public_needs_its_own_word() {
        // The switch this exists for. A credential makes the account *readable*;
        // it must not make it *audible*, because the launch gate asks for a
        // hundred replies to be read beside their fact sheets first and there
        // was no way to do that without publishing them.
        assert!(may_publish(&vars(&[("RADAR_X_PUBLISH", "on")])));
        assert!(may_publish(&vars(&[("RADAR_X_PUBLISH", "ON")])));
        assert!(may_publish(&vars(&[("RADAR_X_PUBLISH", "  on  ")])));
    }

    #[test]
    fn anything_that_is_not_that_word_is_silence() {
        // Including every value an operator might reasonably expect to work.
        // A ceiling spelled wrongly must not read as permission, and this is a
        // ceiling on speech.
        //
        // `true`, `1` and `yes` are here deliberately: each one is somebody
        // confidently enabling the account and getting silence, which is the
        // safe direction and is reported by `posture` rather than left a
        // mystery.
        for value in ["", " ", "off", "true", "1", "yes", "no", "onn", "n"] {
            assert!(
                !may_publish(&vars(&[("RADAR_X_PUBLISH", value)])),
                "{value:?} must not enable publishing"
            );
        }
        assert!(!may_publish(&vars(&[])), "absent is silence");
    }

    /// A client that is never called — only [`Publisher::name`] is read.
    fn a_client() -> X {
        X::at("https://example.test", "bearer", "u42")
    }

    #[test]
    fn only_a_credential_that_is_switched_on_speaks() {
        // CI found this by deleting the arm that selects the live client and
        // watching every test pass. The daemon that leaves behind holds a valid
        // credential, is switched on, and silently posts nothing.
        //
        // It is the failure direction nobody notices: a daemon that wrongly
        // posts is caught within a minute by anybody looking at the account, and
        // one that wrongly stays silent looks exactly like a quiet week — which
        // `radar brief`'s analyst check is deliberately built not to alarm on.
        assert_eq!(publisher_for(Some(a_client()), true).name(), "x");

        // Every other combination is silence, and each is a real state: no
        // credential yet; a credential being read beside its fact sheets before
        // anybody outside sees a reply; and the switch on with nothing to speak
        // through, which must not be mistaken for the first case.
        assert_eq!(publisher_for(Some(a_client()), false).name(), "dry-run");
        assert_eq!(publisher_for(None, true).name(), "dry-run");
        assert_eq!(publisher_for(None, false).name(), "dry-run");
    }

    #[test]
    fn the_three_states_are_told_apart_in_words() {
        // Reading-but-silent and live look identical from everywhere except the
        // reply log, so the daemon says which it is on every start.
        assert!(posture(false, false, false).contains("no credential"));
        assert!(posture(true, true, false).contains("log ONLY"));
        assert!(posture(true, true, false).contains("RADAR_X_PUBLISH=on"));
        assert!(posture(true, true, true).contains("LIVE"));

        // A bearer is required to speak, so "publishing without one" is not a
        // state that can exist -- and if it ever did, it must not be reported
        // as live.
        assert!(posture(false, true, true).contains("no credential"));

        // The misconfiguration the second credential introduced: switched on,
        // able to read, unable to sign. Answers everything, delivers nothing.
        // It must not read as either of its neighbours.
        let unsigned = posture(true, false, true);
        assert!(unsigned.contains("no signing credential"), "{unsigned}");
        assert!(unsigned.contains("RADAR_X_API_KEY"), "{unsigned}");
        assert!(!unsigned.contains("LIVE"), "{unsigned}");
        assert!(!unsigned.contains("log ONLY"), "{unsigned}");
    }

    #[test]
    fn the_gate_ignores_the_bot_by_its_own_id_and_not_by_a_name() {
        // Research 0029, S21. The list held the literal `radar` until
        // 2026-09-06. A mention carries `author_id`, which is a run of digits,
        // so no account could ever match it -- the one entry the ignore list
        // exists for was the one entry it did not contain.
        //
        // Re-apply by returning `vec!["radar".to_owned()]` and the first
        // assertion fails.
        let x = X::at("http://127.0.0.1:1", "test-token", "1739482910");
        assert_eq!(ignored(Some(&x)), vec!["1739482910".to_owned()]);

        // No credential is nothing to poll, so there is nobody to ignore. A
        // placeholder here would be a list that matches nobody, which is what
        // the bug was.
        assert!(ignored(None).is_empty());
    }

    #[test]
    fn a_misconfigured_provider_says_so_and_an_absent_one_does_not_alarm() {
        // `radar_model::from_vars(&env).ok()` threw the reason away, so a key
        // set with a price missing produced an account that answered exactly
        // as it had the day before -- and nothing said the key had been read
        // and rejected. `Selection` names every missing variable; this is what
        // lets it reach anybody.
        //
        // Re-apply by collapsing the match back to one arm: the first
        // assertion passes and every other one fails.
        let resting = provider_notice(&radar_model::Selection::None);
        assert!(resting.contains("template"), "{resting}");
        assert!(
            !resting.to_lowercase().contains("unusable"),
            "the resting state must not read as a fault: {resting}"
        );

        // The one an operator setting up a key actually hits, and it has to
        // carry the variable name through: "incomplete" on its own tells them
        // nothing they did not already know.
        let half = provider_notice(&radar_model::Selection::Incomplete(
            "RADAR_MODEL_OPENAI_KEY is set but RADAR_MODEL_PRICE_IN is missing".to_owned(),
        ));
        assert!(half.contains("UNUSABLE"), "{half}");
        assert!(half.contains("RADAR_MODEL_PRICE_IN"), "{half}");

        // And the one that fires when a vendor is switched without unsetting
        // the old key -- the case paying two vendors at once looks like.
        let both = provider_notice(&radar_model::Selection::Ambiguous(
            "RADAR_MODEL_API_KEY and RADAR_MODEL_OPENAI_KEY".to_owned(),
        ));
        assert!(both.contains("UNUSABLE"), "{both}");
        assert!(both.contains("RADAR_MODEL_OPENAI_KEY"), "{both}");
    }
}
