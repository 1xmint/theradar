// SPDX-License-Identifier: Apache-2.0
//! Who gets answered, and how often.
//!
//! # Why this exists at all
//!
//! Every reply costs money — an X reply, a model call and a handful of RPC
//! reads — and **every one of them is triggered by a stranger**. The bill is
//! therefore unbounded by default, and it is unbounded by people who can see
//! that it is.
//!
//! There is **no rate limiter anywhere in `radar-serve` today**. This is the
//! first one in the repository, which is why it is a small pure type with its
//! own tests rather than a few counters inside a loop.
//!
//! # Rule 8: no configuration means nothing is answered
//!
//! A gate with no limits loaded refuses everything. That is the same shape as a
//! spend meter with no budget and a signer with no allowlist, and it is chosen
//! for the same reason: **spending nothing is always recoverable.** The failure
//! this prevents is a deploy that silently drops its config and answers the
//! world for free.
//!
//! # Pure, so the refusals are testable
//!
//! No clock and no I/O. The caller passes the current time in, exactly as
//! `radar-risk` takes the slot as an argument, so every refusal here can be
//! reproduced from a recording rather than by waiting a day.

use std::collections::HashMap;

/// What the gate decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admitted {
    /// Answer it.
    Yes,
    /// Refused, with a reason worth telling the asker.
    No(Refused),
}

/// Why a mention was not answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// No limits were configured, so nothing is answered.
    Unconfigured,
    /// This asker has had their allowance today.
    SummonerDaily {
        /// What the cap is.
        cap: u32,
    },
    /// The account's whole daily allowance is spent.
    GlobalDaily {
        /// What the cap is.
        cap: u32,
    },
    /// The account is answering as fast as it is allowed to right now.
    ///
    /// Distinct from [`Self::GlobalDaily`] because the two mean opposite things
    /// to the person refused: the day's allowance is gone until midnight, and
    /// this one comes back in minutes.
    GlobalRate {
        /// Replies the account will send per hour once the burst is spent.
        per_hour: u32,
    },
    /// This mint was answered recently; point at that answer instead.
    AlreadyAnswered {
        /// The reply that already exists.
        reply_id: String,
    },
    /// Radar itself, or an account it should not argue with.
    SelfOrIgnored,
}

/// The limits.
///
/// Deliberately has no `Default`. A caller that wants limits has to state them,
/// because a default here would be a policy invented by whoever typed it and
/// applied to real money.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Replies one summoner may get per day.
    pub per_summoner_daily: u32,
    /// Replies the account will send per day, in total.
    ///
    /// The backstop. X allows 10,000 posts per 24 hours per app, so this is a
    /// **cost** ceiling rather than an API one, and it should be set from what
    /// the operator is willing to spend.
    ///
    /// **It is a ceiling and not a schedule**, which is the hole
    /// [`Gate::burst`] closes. See there.
    pub global_daily: u32,
    /// How long a mint's answer is reused rather than recomputed, in seconds.
    pub dedupe_seconds: u64,
}

/// The gate.
#[derive(Debug)]
pub struct Gate {
    limits: Option<Limits>,
    day: u64,
    per_summoner: HashMap<String, u32>,
    global: u32,
    /// Tokens left in the hourly bucket. See [`Gate::burst`].
    tokens: u32,
    /// When the bucket was last topped up, in seconds since the epoch.
    ///
    /// Advanced by exactly the time the refill consumed rather than to `now`,
    /// so a partial hour is not thrown away on every call.
    refilled_at: u64,
    answered: HashMap<String, (u64, String)>,
    ignored: Vec<String>,
}

impl Gate {
    /// A gate with limits.
    #[must_use]
    pub fn new(limits: Limits, ignored: Vec<String>) -> Self {
        Self {
            limits: Some(limits),
            day: 0,
            per_summoner: HashMap::new(),
            global: 0,
            tokens: Self::burst(limits),
            refilled_at: 0,
            answered: HashMap::new(),
            ignored,
        }
    }

    /// The most replies the account will send back to back.
    ///
    /// # The hole this closes
    ///
    /// `global_daily` was spent purely in arrival order, with nothing spreading
    /// it out. On the box it is 50. So **seventeen throwaway accounts asking
    /// about three real mints each at 00:01 UTC silenced the bot for everyone
    /// until midnight**, every day, for the price of seventeen sign-ups. Every
    /// one of those mentions is individually legitimate: three a day is inside
    /// the per-summoner cap, and the mints are real. No rule was broken and the
    /// account went quiet for twenty-three hours.
    ///
    /// A **token bucket** rather than a fixed hourly window, and the difference
    /// is the whole point: a window resets on a boundary, so fifty replies at
    /// 10:59 and fifty more at 11:00 is a hundred in two minutes and the
    /// schedule is decorative. A bucket has no boundary to sit either side of.
    ///
    /// Ten, or the daily cap when that is smaller — a burst larger than the day
    /// is not a burst. Ten is chosen so a genuine flurry (a coin going around,
    /// several people asking at once) is answered immediately, which is the
    /// behaviour that makes the account worth mentioning, and so the refusal
    /// arrives only when somebody is clearly working at it.
    const fn burst(limits: Limits) -> u32 {
        if limits.global_daily < 10 {
            limits.global_daily
        } else {
            10
        }
    }

    /// Replies added back per hour: the day's allowance, spread over the day.
    ///
    /// At least one, so a very small daily cap still recovers rather than
    /// stopping for good after the first burst.
    const fn per_hour(limits: Limits) -> u32 {
        if limits.global_daily < 24 {
            1
        } else {
            limits.global_daily / 24
        }
    }

    /// A gate with **no** limits, which answers nothing.
    ///
    /// Rule 8. This is what an instance with missing configuration gets, and it
    /// is not an error state — it is the safe one.
    #[must_use]
    pub fn unconfigured() -> Self {
        Self {
            limits: None,
            day: 0,
            per_summoner: HashMap::new(),
            global: 0,
            tokens: 0,
            refilled_at: 0,
            answered: HashMap::new(),
            ignored: Vec::new(),
        }
    }

    /// Moves the day forward if `now` is in a later one, clearing allowances.
    ///
    /// **Called by both [`Gate::admit`] and [`Gate::record`], and it has to
    /// be.** An earlier version rolled the day only in `admit`, so a `record`
    /// that arrived first was counted against the *old* day and then wiped by
    /// the next `admit` — every allowance silently reset to zero. The test that
    /// caught it is `allowances_reset_on_a_new_day_and_not_on_a_restart`.
    ///
    /// The day comes from the clock the caller passed rather than from how long
    /// this process has been alive. That distinction is `Agent::restore`'s
    /// defect exactly: the startup path began the day again at zero, so a crash
    /// loop under `Restart=always` handed out a fresh allowance per crash.
    fn roll_to(&mut self, now: u64) {
        let day = now / 86_400;
        if day != self.day {
            self.day = day;
            self.per_summoner.clear();
            self.global = 0;
        }
        self.refill_to(now);
    }

    /// Tops the hourly bucket up for the time that has passed.
    ///
    /// `refilled_at` moves forward by the seconds the refill actually consumed,
    /// never to `now`. Snapping it to `now` would discard whatever fraction of
    /// an hour had not yet earned a whole token, so a gate consulted every five
    /// minutes would refill at zero for ever — the bucket would look like a
    /// bucket and behave like a cap of `burst` per lifetime.
    fn refill_to(&mut self, now: u64) {
        let Some(limits) = self.limits else {
            return;
        };
        if self.refilled_at == 0 {
            self.refilled_at = now;
            return;
        }
        // A clock that went backwards is not a refill. Re-based rather than
        // ignored, so the bucket is not frozen until the clock catches up.
        if now < self.refilled_at {
            self.refilled_at = now;
            return;
        }
        let per_hour = u64::from(Self::per_hour(limits));
        let elapsed = now - self.refilled_at;
        let earned = elapsed.saturating_mul(per_hour) / 3_600;
        if earned == 0 {
            return;
        }
        let burst = Self::burst(limits);
        self.tokens = burst.min(
            self.tokens
                .saturating_add(u32::try_from(earned).unwrap_or(u32::MAX)),
        );
        // Only the whole tokens are paid for. `per_hour` is at least 1, so this
        // cannot divide by zero.
        self.refilled_at += earned.saturating_mul(3_600) / per_hour;
    }

    /// Decides one mention.
    ///
    /// `now` is seconds since the epoch, passed in rather than read, so a
    /// refusal is reproducible.
    pub fn admit(&mut self, summoner: &str, mint: &str, now: u64) -> Admitted {
        let Some(limits) = self.limits else {
            return Admitted::No(Refused::Unconfigured);
        };

        self.roll_to(now);

        if self.ignored.iter().any(|i| i == summoner) {
            // Answering Radar's own posts is a loop that costs money on every
            // pass, and arguing with another bot is the same thing more slowly.
            return Admitted::No(Refused::SelfOrIgnored);
        }

        // Dedupe first, and deliberately: pointing at an existing answer costs
        // no model call and no RPC, so it must not be refused by a cap it does
        // not spend against.
        if let Some((at, reply_id)) = self.answered.get(mint)
            && now.saturating_sub(*at) < limits.dedupe_seconds
        {
            return Admitted::No(Refused::AlreadyAnswered {
                reply_id: reply_id.clone(),
            });
        }

        if self.global >= limits.global_daily {
            return Admitted::No(Refused::GlobalDaily {
                cap: limits.global_daily,
            });
        }
        // Checked here and spent in `record`, matching how `global` itself
        // works: a mention that was admitted and then failed to post has cost
        // the account no reply, and charging the rate for it would let a broken
        // publisher throttle a healthy one.
        if self.tokens == 0 {
            return Admitted::No(Refused::GlobalRate {
                per_hour: Self::per_hour(limits),
            });
        }
        let used = self.per_summoner.entry(summoner.to_owned()).or_insert(0);
        if *used >= limits.per_summoner_daily {
            return Admitted::No(Refused::SummonerDaily {
                cap: limits.per_summoner_daily,
            });
        }
        // **The summoner's allowance is spent here, on admission, and the
        // account's is spent on sending.** They are two different costs. An
        // admitted mention reads the chain before anything is posted -- sixty
        // RPC calls under the dossier's own budget -- so until 2026-09-05 one
        // account posting thirty mentions naming thirty unreadable mints, or
        // thirty anything while the publisher was down, was thirty dossiers
        // and no refusal: the cap counted replies, and none of those was one.
        // The per-summoner cap is what stops one account making the bot
        // expensive, so it has to count the expensive thing.
        *used += 1;
        Admitted::Yes
    }

    /// Records that a reply was actually sent.
    ///
    /// Separate from [`Gate::admit`] on purpose, for the **account's** cap. A
    /// mention that was admitted and then failed to post has cost nothing on
    /// X, and charging the global allowance for it would let a broken publisher
    /// silence the account by spending a budget it never used. The summoner's
    /// own allowance was already charged on admission, and is not charged
    /// again here.
    pub fn record(&mut self, summoner: &str, mint: &str, reply_id: &str, now: u64) {
        self.roll_to(now);
        // Kept as a parameter so a caller cannot record a reply without saying
        // who it was for; the count it used to move is spent in `admit`.
        let _ = summoner;
        self.global += 1;
        self.tokens = self.tokens.saturating_sub(1);
        self.answered
            .insert(mint.to_owned(), (now, reply_id.to_owned()));
    }

    /// How many replies have gone out today.
    #[must_use]
    pub const fn sent_today(&self) -> u32 {
        self.global
    }

    /// Replies the account may send back to back right now.
    #[must_use]
    pub const fn tokens_left(&self) -> u32 {
        self.tokens
    }

    /// Rebuilds today's counts and the dedupe map from what is on disk.
    ///
    /// # Why a gate has to be restorable
    ///
    /// Every count in here lived only in memory. `radar-analyst.service` runs
    /// under `Restart=always`, and the daemon restarted three times on the
    /// night of 2026-09-06 — each restart handed every summoner a fresh
    /// allowance, emptied the day's total, and forgot every mint answered in
    /// the last hour. The dedupe map is the worst of the three: forgetting it
    /// means the next asker about a mint answered ten minutes ago gets a second
    /// model call, a second dossier and a second public reply saying the same
    /// thing. A crash loop is then a spending loop.
    ///
    /// `Agent::restore`'s recorded defect was the same shape and the day
    /// counter already learned from it: the day comes from the caller's clock
    /// rather than from how long the process has been alive.
    ///
    /// # What the two logs can and cannot say
    ///
    /// `replies.jsonl` holds **two lines per reply**: `publish` appends before
    /// it posts and again after, which is the log-before-post rule. So lines
    /// are folded by `mention_id`, and a mention counts once however many times
    /// it appears.
    ///
    /// | recovered | from | exact? |
    /// |---|---|---|
    /// | the day's sent total | reply lines carrying a `reply_id` | yes |
    /// | each summoner's usage | distinct mentions in the log | a **lower bound** |
    /// | the dedupe map | mint to the id that answered it | yes |
    /// | the hourly bucket | not recovered | see below |
    ///
    /// The per-summoner figure is a lower bound because the allowance is spent
    /// on *admission* and a mention admitted but never published — an
    /// unreadable chain, a mint that is not an address — never reaches the log.
    /// A restart therefore hands back the few reads that got that far. That is
    /// the direction to be wrong in, and it is a great deal narrower than
    /// handing back everything, which is what happened before.
    ///
    /// A **pointer** line — `Entry::pointed_at` — counts against the day and
    /// against nothing else: it cost a post, it spent no summoner allowance
    /// (the gate refused before it charged), and it must never enter the dedupe
    /// map, or later askers are pointed at the pointer.
    ///
    /// `refusals.jsonl` is deliberately **not** read. A refusal spent nothing
    /// by definition, so there is nothing in it to restore — and reading it
    /// would be the one way to count a reply twice.
    ///
    /// The bucket is deliberately **not** restored: it starts full. Its job is
    /// to stop a burst, and a process that has just restarted has sent nothing
    /// in the seconds since — refusing on a rate it did not spend would turn a
    /// restart into an outage, which is the failure this whole function is
    /// about.
    pub fn restore(&mut self, replies: &[crate::log::Entry], now: u64) {
        let Some(limits) = self.limits else {
            return;
        };
        self.roll_to(now);
        let today = now / 86_400;

        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for entry in replies {
            if entry.at / 86_400 != today {
                continue;
            }
            // A pointer spent no summoner allowance, so it is not counted as a
            // mention that did.
            if seen.insert(entry.mention_id.as_str()) && entry.pointed_at.is_none() {
                *self.per_summoner.entry(entry.summoner.clone()).or_insert(0) += 1;
            }
            let Some(reply_id) = &entry.reply_id else {
                continue;
            };
            // The post happened, so the account's allowance was spent. The
            // pre-post line for the same mention carries no id and is skipped
            // here, which is what keeps one reply from counting twice.
            self.global += 1;
            if entry.pointed_at.is_some() {
                continue;
            }
            if let Some(mint) = &entry.mint {
                // Latest wins. The log is append-ordered, so a later line for
                // the same mint is the more recent answer.
                let newer = self
                    .answered
                    .get(mint)
                    .is_none_or(|(at, _)| *at <= entry.at);
                if newer && now.saturating_sub(entry.at) < limits.dedupe_seconds {
                    self.answered
                        .insert(mint.clone(), (entry.at, reply_id.clone()));
                }
            }
        }
    }

    /// How many mints are inside the dedupe window.
    ///
    /// For the restart line an operator reads. A restored gate that says zero
    /// here after a busy hour is the failure `restore` exists to prevent,
    /// showing itself at the moment somebody is looking.
    #[must_use]
    pub fn answered_recently(&self) -> usize {
        self.answered.len()
    }

    /// Records a post that answered nothing of its own — a pointer.
    ///
    /// Spends the account's daily allowance and an hourly token, because it
    /// cost a post. Touches neither the summoner's allowance nor the dedupe
    /// map: see [`crate::log::Entry::pointed_at`] for why each of those would
    /// be wrong.
    pub fn record_post(&mut self, now: u64) {
        self.roll_to(now);
        self.global += 1;
        self.tokens = self.tokens.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;

    fn limits() -> Limits {
        Limits {
            per_summoner_daily: 2,
            global_daily: 5,
            dedupe_seconds: 3600,
        }
    }

    #[test]
    fn the_day_boundary_is_a_day_and_the_allowance_survives_inside_it() {
        // `roll_to` divides the clock into days. Mutated to multiply, every
        // distinct instant becomes a distinct "day" and the per-summoner
        // allowance is cleared on every single call -- so one summoner could
        // ask without limit, which is the whole thing this gate exists to stop.
        let mut gate = Gate::new(limits(), Vec::new());

        assert_eq!(gate.admit("alice", "MintOne", DAY), Admitted::Yes);
        gate.record("alice", "MintOne", "r1", DAY);
        assert_eq!(gate.admit("alice", "MintTwo", DAY + 1), Admitted::Yes);
        gate.record("alice", "MintTwo", "r2", DAY + 1);

        // Two used, two allowed, still the same day: the third is refused.
        assert!(
            matches!(
                gate.admit("alice", "MintThree", DAY + 2),
                Admitted::No(Refused::SummonerDaily { .. })
            ),
            "a second passing instant is not a new day"
        );

        // And a real new day does restore it.
        assert_eq!(gate.admit("alice", "MintThree", 2 * DAY), Admitted::Yes);
    }

    #[test]
    fn the_dedupe_window_is_exclusive_at_its_edge() {
        // `now - at < dedupe_seconds`. One character from `<=`, and the two
        // disagree only at the boundary -- so without this the window is a
        // second longer than it says, for ever, and nothing notices.
        let mut gate = Gate::new(limits(), Vec::new());
        let l = limits();

        assert_eq!(gate.admit("alice", "MintOne", DAY), Admitted::Yes);
        gate.record("alice", "MintOne", "r1", DAY);

        // A second inside the window: pointed at the existing answer.
        assert!(matches!(
            gate.admit("alice", "MintOne", DAY + l.dedupe_seconds - 1),
            Admitted::No(Refused::AlreadyAnswered { .. })
        ));

        // Exactly the window: outside it, and answerable again.
        assert!(
            !matches!(
                gate.admit("alice", "MintOne", DAY + l.dedupe_seconds),
                Admitted::No(Refused::AlreadyAnswered { .. })
            ),
            "the window is exclusive at its edge"
        );
    }

    #[test]
    fn the_reply_count_is_the_count_and_not_a_constant() {
        // Every other test here asserts `sent_today() == 0`, which a function
        // returning zero satisfies for ever. That is LEARNINGS 5 in a counter:
        // a meter reading zero because nothing happened and one reading zero
        // because it is broken are the same number.
        let mut gate = Gate::new(limits(), Vec::new());
        assert_eq!(gate.sent_today(), 0);

        gate.record("alice", "MintOne", "r1", DAY);
        assert_eq!(gate.sent_today(), 1, "one reply is one reply");

        gate.record("bob", "MintTwo", "r2", DAY);
        assert_eq!(
            gate.sent_today(),
            2,
            "the count is global, not per summoner"
        );
    }

    #[test]
    fn an_unconfigured_gate_answers_nothing() {
        // Rule 8, and the failure it prevents is a deploy that dropped its
        // config and started answering the world for free.
        let mut gate = Gate::unconfigured();
        assert_eq!(
            gate.admit("alice", "MintOne", DAY),
            Admitted::No(Refused::Unconfigured)
        );
    }

    #[test]
    fn one_summoner_cannot_spend_the_whole_budget() {
        let mut gate = Gate::new(limits(), Vec::new());
        for i in 0..2 {
            assert_eq!(gate.admit("alice", &format!("Mint{i}"), DAY), Admitted::Yes);
            gate.record("alice", &format!("Mint{i}"), "r", DAY);
        }
        assert_eq!(
            gate.admit("alice", "MintThree", DAY),
            Admitted::No(Refused::SummonerDaily { cap: 2 })
        );
        // And somebody else is unaffected, or one loud account could silence
        // the service for everyone.
        assert_eq!(gate.admit("bob", "MintThree", DAY), Admitted::Yes);
    }

    #[test]
    fn the_global_cap_stops_a_crowd() {
        // The per-summoner cap does nothing against many accounts, which is the
        // cheap attack: a hundred throwaway accounts asking once each.
        let mut gate = Gate::new(limits(), Vec::new());
        for i in 0..5 {
            let who = format!("user{i}");
            assert_eq!(gate.admit(&who, &format!("Mint{i}"), DAY), Admitted::Yes);
            gate.record(&who, &format!("Mint{i}"), "r", DAY);
        }
        assert_eq!(
            gate.admit("fresh", "MintSix", DAY),
            Admitted::No(Refused::GlobalDaily { cap: 5 })
        );
    }

    #[test]
    fn a_mint_answered_recently_points_at_the_answer() {
        let mut gate = Gate::new(limits(), Vec::new());
        gate.record("alice", "MintOne", "reply-1", DAY);
        assert_eq!(
            gate.admit("bob", "MintOne", DAY + 60),
            Admitted::No(Refused::AlreadyAnswered {
                reply_id: "reply-1".to_owned()
            })
        );
        // And it expires, so a token that moved is answerable again.
        assert_eq!(gate.admit("bob", "MintOne", DAY + 3601), Admitted::Yes);
    }

    #[test]
    fn dedupe_is_checked_before_the_caps_it_does_not_spend() {
        // Pointing at an existing answer costs no model call and no RPC. If a
        // spent global cap refused it, the account would go silent rather than
        // giving the cheapest useful reply it has.
        let mut gate = Gate::new(limits(), Vec::new());
        gate.record("alice", "MintOne", "reply-1", DAY);
        for i in 0..5 {
            gate.record(&format!("u{i}"), &format!("Other{i}"), "r", DAY);
        }
        assert!(matches!(
            gate.admit("bob", "MintOne", DAY + 60),
            Admitted::No(Refused::AlreadyAnswered { .. })
        ));
    }

    #[test]
    fn the_account_does_not_answer_itself() {
        // A reply to its own post is a loop that costs money on every pass.
        let mut gate = Gate::new(limits(), vec!["radar".to_owned()]);
        assert_eq!(
            gate.admit("radar", "MintOne", DAY),
            Admitted::No(Refused::SelfOrIgnored)
        );
    }

    #[test]
    fn allowances_reset_on_a_new_day_and_not_on_a_restart() {
        // The day comes from the clock the caller passed, not from how long the
        // process has been alive. `Agent::restore` had exactly this defect: the
        // startup path began the day again at zero, so a crash loop handed out
        // a fresh allowance per crash.
        let mut gate = Gate::new(limits(), Vec::new());
        for i in 0..5 {
            let who = format!("user{i}");
            gate.record(&who, &format!("Mint{i}"), "r", DAY);
        }
        assert!(matches!(
            gate.admit("fresh", "MintSix", DAY),
            Admitted::No(Refused::GlobalDaily { .. })
        ));
        assert_eq!(gate.admit("fresh", "MintSix", 2 * DAY), Admitted::Yes);
        assert_eq!(gate.sent_today(), 0);
    }

    #[test]
    fn admitting_without_sending_spends_the_summoners_allowance_and_not_the_accounts() {
        // Two caps, two costs. A publisher that is failing must not be able to
        // silence the account by burning the global allowance it never used --
        // so `sent_today` stays at zero however many are admitted. But each
        // admission reads the chain, so the *summoner's* allowance is spent on
        // admission: until 2026-09-05 this loop admitted ten and the cap of two
        // never fired, which is the 30-mention burst from design 0007's gate
        // costing thirty dossiers. Re-applied by moving the increment back to
        // `record`: the third admission below comes back `Yes` and this fails.
        let mut gate = Gate::new(limits(), Vec::new());
        for i in 0..2 {
            assert_eq!(gate.admit("alice", &format!("Mint{i}"), DAY), Admitted::Yes);
        }
        for i in 2..10 {
            assert_eq!(
                gate.admit("alice", &format!("Mint{i}"), DAY),
                Admitted::No(Refused::SummonerDaily { cap: 2 }),
                "admission {i}"
            );
        }
        assert_eq!(gate.sent_today(), 0);
        // And nobody else pays for alice's burst.
        assert_eq!(gate.admit("bob", "MintX", DAY), Admitted::Yes);
    }

    #[test]
    fn a_reply_recorded_after_admission_is_not_charged_to_the_summoner_twice() {
        // The other half of the split. If `record` also moved the summoner's
        // count, every published reply would cost two of the daily allowance
        // and the cap on the page would be half the cap in force.
        let mut gate = Gate::new(limits(), Vec::new());
        assert_eq!(gate.admit("alice", "MintOne", DAY), Admitted::Yes);
        gate.record("alice", "MintOne", "r1", DAY);
        assert_eq!(gate.admit("alice", "MintTwo", DAY), Admitted::Yes);
        gate.record("alice", "MintTwo", "r2", DAY);
        assert_eq!(gate.sent_today(), 2);
        assert_eq!(
            gate.admit("alice", "MintThree", DAY),
            Admitted::No(Refused::SummonerDaily { cap: 2 })
        );
    }

    // --- the hourly bucket ----------------------------------------------------

    /// Limits for the tests about the **account's** rate.
    ///
    /// `per_summoner_daily` is deliberately out of the way: a refusal in one of
    /// these must be the bucket, and a per-summoner cap firing first would make
    /// the test pass for the wrong reason. `global_daily: 48` makes the refill
    /// exactly two an hour, so the arithmetic is readable.
    fn wide() -> Limits {
        Limits {
            per_summoner_daily: 100,
            global_daily: 48,
            dedupe_seconds: 3_600,
        }
    }

    /// One admitted-and-sent reply, which is the pair the loop always makes.
    fn answer(gate: &mut Gate, who: &str, mint: &str, now: u64) -> Admitted {
        let verdict = gate.admit(who, mint, now);
        if verdict == Admitted::Yes {
            gate.record(who, mint, "r", now);
        }
        verdict
    }

    #[test]
    fn a_crowd_at_one_instant_cannot_spend_the_whole_day() {
        // The denial of service, at the size that made it free.
        //
        // `global_daily` was spent in arrival order with nothing spreading it
        // out. On the box it is 50. So seventeen throwaway accounts asking about
        // three real mints each at 00:01 UTC silenced the account for everyone
        // until midnight — every mention individually legitimate, no rule
        // broken, twenty-three hours of silence for the price of seventeen
        // sign-ups.
        let mut gate = Gate::new(wide(), Vec::new());
        let at = DAY;

        // Ten go out back to back: a real flurry is what makes this account
        // worth mentioning, and answering it immediately is the point.
        for n in 0..10 {
            assert_eq!(
                answer(&mut gate, &format!("a{n}"), &format!("Mint{n}"), at),
                Admitted::Yes,
                "reply {n} is inside the burst"
            );
        }
        assert_eq!(gate.tokens_left(), 0);

        // The eleventh at the same instant is not, and the day is nowhere near
        // spent — which is the difference the asker is told about.
        assert!(
            matches!(
                gate.admit("a11", "Mint11", at),
                Admitted::No(Refused::GlobalRate { per_hour: 2 })
            ),
            "the eleventh in one instant must be refused on the rate"
        );
        assert_eq!(gate.sent_today(), 10, "and the day is barely touched");

        // An hour later the account is answering again, without anybody
        // restarting anything.
        assert_eq!(
            answer(&mut gate, "a11", "Mint11", at + 3_600),
            Admitted::Yes
        );
    }

    #[test]
    fn the_bucket_has_no_boundary_to_burst_either_side_of() {
        // Why a token bucket and not an hourly window. A window resets on a
        // boundary, so `global_daily / 24` at 10:59 and the same again at 11:00
        // is twice the rate in two minutes and the schedule is decorative.
        //
        // Here the burst is spent at the top of an hour and the whole of the
        // next hour is attempted one second later: what comes back is the
        // refill and nothing more.
        let mut gate = Gate::new(wide(), Vec::new());
        let hour = DAY + 3_600;
        for n in 0..10 {
            assert_eq!(
                answer(&mut gate, "alice", &format!("M{n}"), hour),
                Admitted::Yes
            );
        }
        // One second into the next hour: `48 / 24` is two an hour, so one second
        // has earned nothing.
        assert!(matches!(
            gate.admit("bob", "Later", hour + 1),
            Admitted::No(Refused::GlobalRate { .. })
        ));
        // A full hour has earned exactly two.
        let later = hour + 3_600;
        assert_eq!(answer(&mut gate, "bob", "L1", later), Admitted::Yes);
        assert_eq!(answer(&mut gate, "bob", "L2", later), Admitted::Yes);
        assert!(matches!(
            gate.admit("bob", "L3", later),
            Admitted::No(Refused::GlobalRate { .. })
        ));
    }

    #[test]
    fn a_gate_consulted_constantly_still_refills() {
        // The bug a naive refill has: snapping `refilled_at` to `now` throws
        // away every fraction of an hour that has not yet earned a whole token,
        // so a gate asked every five minutes refills at zero for ever and the
        // bucket becomes a cap of `burst` per process lifetime.
        //
        // Re-apply by setting `self.refilled_at = now` in `refill_to`: this
        // fails at the first admission after the burst.
        let mut gate = Gate::new(wide(), Vec::new());
        let at = DAY;
        for n in 0..10 {
            assert_eq!(
                answer(&mut gate, "alice", &format!("M{n}"), at),
                Admitted::Yes
            );
        }
        // Polled every five minutes for an hour, the way the daemon does.
        for tick in 1..=12 {
            let _ = gate.admit("bob", "Nope", at + tick * 300);
        }
        assert_eq!(
            answer(&mut gate, "bob", "Yes", at + 3_600),
            Admitted::Yes,
            "an hour of five-minute polls must still have earned a token"
        );
    }

    #[test]
    fn the_daily_cap_is_still_the_ceiling_above_the_rate() {
        // The bucket spreads the day out; it does not replace the day. A cap
        // small enough to be spent before the bucket empties must still bind.
        let limits = Limits {
            per_summoner_daily: 100,
            global_daily: 3,
            dedupe_seconds: 3_600,
        };
        let mut gate = Gate::new(limits, Vec::new());
        let at = DAY;
        for n in 0..3 {
            assert_eq!(
                answer(&mut gate, "alice", &format!("M{n}"), at),
                Admitted::Yes
            );
        }
        assert!(
            matches!(
                gate.admit("alice", "M4", at + 86_000),
                Admitted::No(Refused::GlobalDaily { cap: 3 })
            ),
            "the day's cap outranks a refilled bucket"
        );
    }

    // --- restoring from disk --------------------------------------------------

    fn entry(
        mention: &str,
        who: &str,
        mint: &str,
        at: u64,
        reply: Option<&str>,
    ) -> crate::log::Entry {
        crate::log::Entry {
            at,
            mention_id: mention.to_owned(),
            summoner: who.to_owned(),
            mint: Some(mint.to_owned()),
            read_at_slot: None,
            fact_sheet: String::new(),
            reply: "said something".to_owned(),
            fellback: None,
            reply_id: reply.map(ToOwned::to_owned),
            signals: Some(Vec::new()),
            pointed_at: None,
        }
    }

    #[test]
    fn a_restart_does_not_hand_back_the_days_allowance() {
        // `radar-analyst.service` runs under `Restart=always` and the daemon
        // restarted three times on the night of 2026-09-06. Every count in this
        // type lived only in memory, so each restart gave every summoner a fresh
        // allowance and emptied the day's total: a crash loop was a spending
        // loop.
        let at = DAY + 100;
        // What `publish` writes: one line before the post, one after.
        let log = vec![
            entry("m1", "alice", "MintOne", at, None),
            entry("m1", "alice", "MintOne", at, Some("r1")),
            entry("m2", "alice", "MintTwo", at, None),
            entry("m2", "alice", "MintTwo", at, Some("r2")),
        ];

        let mut gate = Gate::new(limits(), Vec::new());
        gate.restore(&log, at + 10);

        assert_eq!(gate.sent_today(), 2, "two replies, not four lines");
        // `limits()` allows two per summoner, and alice has had both.
        assert!(matches!(
            gate.admit("alice", "MintThree", at + 10),
            Admitted::No(Refused::SummonerDaily { cap: 2 })
        ));
        // And the mints answered are still answered.
        assert!(matches!(
            gate.admit("bob", "MintOne", at + 10),
            Admitted::No(Refused::AlreadyAnswered { .. })
        ));
    }

    #[test]
    fn a_restored_dedupe_points_at_the_reply_that_exists() {
        // Not merely "refused" — the refusal has to carry the id, because the
        // loop answers a duplicate by pointing at it. A restore that forgot the
        // id would produce a refusal nothing could act on.
        let at = DAY + 100;
        let log = vec![entry("m1", "alice", "MintOne", at, Some("r99"))];
        let mut gate = Gate::new(limits(), Vec::new());
        gate.restore(&log, at + 10);

        match gate.admit("bob", "MintOne", at + 10) {
            Admitted::No(Refused::AlreadyAnswered { reply_id }) => {
                assert_eq!(reply_id, "r99");
            }
            other => panic!("expected a pointer to r99, got {other:?}"),
        }
    }

    #[test]
    fn a_reply_from_outside_the_dedupe_window_is_not_restored_as_current() {
        // The window is what makes the dedupe honest: an answer from this
        // morning is not the answer to a question asked this evening, because
        // the chain has moved.
        let at = DAY;
        let log = vec![entry("m1", "alice", "MintOne", at, Some("r1"))];
        let mut gate = Gate::new(limits(), Vec::new());
        // `limits()` dedupes for an hour; two hours have passed.
        gate.restore(&log, at + 7_200);
        assert_eq!(gate.admit("bob", "MintOne", at + 7_200), Admitted::Yes);
    }

    #[test]
    fn yesterdays_replies_do_not_count_against_today() {
        // The day boundary, on the restore path as well as the live one. A
        // restart just after midnight must not start with yesterday's total.
        let yesterday = DAY;
        let log = vec![
            entry("m1", "alice", "MintOne", yesterday, Some("r1")),
            entry("m2", "alice", "MintTwo", yesterday, Some("r2")),
        ];
        let mut gate = Gate::new(limits(), Vec::new());
        gate.restore(&log, 2 * DAY + 60);
        assert_eq!(gate.sent_today(), 0);
        assert_eq!(
            gate.admit("alice", "MintThree", 2 * DAY + 60),
            Admitted::Yes
        );
    }

    #[test]
    fn a_pointer_costs_the_day_and_nothing_else() {
        // The three rules `Entry::pointed_at` carries, all in one place, because
        // getting any one of them wrong is invisible until it matters:
        //
        // 1. it counts against the day — it cost a post;
        // 2. it does not spend the summoner's allowance — the gate refused
        //    before it charged, which is why a pointer exists at all;
        // 3. it does not enter the dedupe map — a pointer that became the
        //    canonical answer would point the next asker at the pointer.
        let at = DAY + 100;
        let mut pointer = entry("m2", "bob", "MintOne", at, Some("p1"));
        pointer.mint = None;
        pointer.pointed_at = Some("r1".to_owned());
        let log = vec![entry("m1", "alice", "MintOne", at, Some("r1")), pointer];

        let mut gate = Gate::new(limits(), Vec::new());
        gate.restore(&log, at + 10);

        assert_eq!(gate.sent_today(), 2, "the pointer was a post");
        // Bob was refused, so bob's allowance is untouched: `limits()` allows
        // two and both are available.
        assert_eq!(gate.admit("bob", "MintTwo", at + 10), Admitted::Yes);
        gate.record("bob", "MintTwo", "r2", at + 10);
        assert_eq!(gate.admit("bob", "MintThree", at + 10), Admitted::Yes);
        // And the third asker is pointed at the answer, not at the pointer.
        match gate.admit("carol", "MintOne", at + 10) {
            Admitted::No(Refused::AlreadyAnswered { reply_id }) => {
                assert_eq!(reply_id, "r1", "never the pointer's own id");
            }
            other => panic!("expected the original answer, got {other:?}"),
        }
    }

    #[test]
    fn an_unpublished_line_counts_against_the_summoner_and_not_the_day() {
        // The dry run, and the publisher that was down. `publish` logs before it
        // posts, so a line with no `reply_id` is a mention that was admitted —
        // it spent the summoner's allowance and read the chain — and never
        // became a public statement.
        let at = DAY + 100;
        let log = vec![entry("m1", "alice", "MintOne", at, None)];
        let mut gate = Gate::new(limits(), Vec::new());
        gate.restore(&log, at + 10);

        assert_eq!(gate.sent_today(), 0, "nothing was said");
        assert_eq!(gate.admit("alice", "MintTwo", at + 10), Admitted::Yes);
        gate.record("alice", "MintTwo", "r2", at + 10);
        assert!(
            matches!(
                gate.admit("alice", "MintThree", at + 10),
                Admitted::No(Refused::SummonerDaily { .. })
            ),
            "the admission was still an admission"
        );
        // And nothing was answered, so nothing is deduped.
        assert_eq!(gate.admit("bob", "MintOne", at + 10), Admitted::Yes);
    }

    #[test]
    fn a_restored_gate_can_still_answer_immediately() {
        // The bucket is deliberately not restored. Its job is to stop a burst,
        // and a process that has just started has sent nothing in the seconds
        // since — refusing on a rate it did not spend would turn a restart into
        // an outage, which is the failure `restore` exists to prevent.
        let at = DAY + 100;
        let log: Vec<crate::log::Entry> = (0..20)
            .map(|n| {
                entry(
                    &format!("m{n}"),
                    &format!("s{n}"),
                    &format!("M{n}"),
                    at,
                    Some("r"),
                )
            })
            .collect();
        let mut gate = Gate::new(wide(), Vec::new());
        gate.restore(&log, at + 10);
        assert_eq!(gate.sent_today(), 20);
        assert_eq!(gate.admit("fresh", "NewMint", at + 10), Admitted::Yes);
    }

    #[test]
    fn an_unconfigured_gate_restores_nothing_and_answers_nothing() {
        // Rule 8 outranks the restore. A gate with no limits refuses everything,
        // and reading a log must not be the thing that gives it an opinion.
        let at = DAY + 100;
        let log = vec![entry("m1", "alice", "MintOne", at, Some("r1"))];
        let mut gate = Gate::unconfigured();
        gate.restore(&log, at + 10);
        assert_eq!(gate.sent_today(), 0);
        assert_eq!(
            gate.admit("alice", "MintTwo", at + 10),
            Admitted::No(Refused::Unconfigured)
        );
    }

    // --- the bucket's arithmetic, at its boundaries -------------------------
    //
    // CI reported eight survivors across `burst`, `per_hour`, `refill_to` and
    // `tokens_left`. Every one of them is a comparison or an operator whose
    // wrong version still produces a bucket that fills and empties -- just at
    // the wrong rate, which is exactly the failure nobody notices.

    fn with_daily(global_daily: u32) -> Limits {
        Limits {
            per_summoner_daily: 100,
            global_daily,
            dedupe_seconds: 3_600,
        }
    }

    #[test]
    fn the_burst_is_the_smaller_of_ten_and_the_day() {
        // A burst larger than the day is not a burst. `<` mutated to `<=` or
        // `==` changes the answer only at exactly ten, which is why the
        // boundary is walked rather than sampled.
        assert_eq!(Gate::burst(with_daily(0)), 0, "no day, no burst");
        assert_eq!(Gate::burst(with_daily(3)), 3);
        assert_eq!(Gate::burst(with_daily(9)), 9);
        assert_eq!(Gate::burst(with_daily(10)), 10, "ten is not under ten");
        assert_eq!(Gate::burst(with_daily(11)), 10);
        assert_eq!(Gate::burst(with_daily(50)), 10, "the box's setting");
    }

    #[test]
    fn the_refill_is_the_day_spread_over_a_day_and_never_zero() {
        // A cap under twenty-four would refill at zero an hour by integer
        // division, so the bucket would be a cap of `burst` per process
        // lifetime -- which is the bug this whole mechanism replaces, one level
        // down.
        assert_eq!(Gate::per_hour(with_daily(0)), 1, "never zero");
        assert_eq!(Gate::per_hour(with_daily(1)), 1);
        assert_eq!(Gate::per_hour(with_daily(23)), 1);
        assert_eq!(Gate::per_hour(with_daily(24)), 1, "exactly one an hour");
        assert_eq!(Gate::per_hour(with_daily(25)), 1);
        assert_eq!(Gate::per_hour(with_daily(48)), 2);
        assert_eq!(Gate::per_hour(with_daily(50)), 2, "the box's setting");
    }

    #[test]
    fn tokens_left_reports_the_bucket_rather_than_a_constant() {
        // Mutated to return `0`, every test that asserts an empty bucket still
        // passes. So this asserts it is *full* at the start and falls one at a
        // time, which no constant satisfies.
        let mut gate = Gate::new(wide(), Vec::new());
        assert_eq!(gate.tokens_left(), 10, "a fresh gate can answer a flurry");
        assert_eq!(answer(&mut gate, "a", "M1", DAY), Admitted::Yes);
        assert_eq!(gate.tokens_left(), 9);
        assert_eq!(answer(&mut gate, "b", "M2", DAY), Admitted::Yes);
        assert_eq!(gate.tokens_left(), 8);
    }

    #[test]
    fn a_clock_that_went_backwards_re_bases_rather_than_freezing() {
        // NTP steps, a container clock, a host resuming from sleep. Without the
        // `now < refilled_at` branch the subtraction below it underflows to a
        // saturating zero and the bucket never refills again -- the account goes
        // quiet until somebody restarts it, which is the outage `restore` exists
        // to prevent arriving by a different door.
        //
        // Mutated to `==` or `<=` the branch stops covering the backwards case,
        // and this fails.
        let mut gate = Gate::new(wide(), Vec::new());
        let at = 10 * DAY;
        for n in 0..10 {
            assert_eq!(
                answer(&mut gate, &format!("s{n}"), &format!("M{n}"), at),
                Admitted::Yes
            );
        }
        // The clock jumps back an hour, then forward again.
        assert!(matches!(
            gate.admit("late", "Nope", at - 3_600),
            Admitted::No(Refused::GlobalRate { .. })
        ));
        assert_eq!(
            answer(&mut gate, "late", "Yes", at + 3_600),
            Admitted::Yes,
            "an hour after the step, the bucket has earned again"
        );
    }

    #[test]
    fn the_refill_clock_advances_by_what_it_paid_for_and_no_further() {
        // `refilled_at += earned * 3600 / per_hour` -- mutated to `*=`, or with
        // the division flipped to a multiplication, the marker lands somewhere
        // in the far future or the far past and the refill rate stops being a
        // rate at all.
        //
        // Asserted as a *rate* over four hours rather than as a field: at two an
        // hour, four hours earns exactly eight and not seven or nine.
        let mut gate = Gate::new(wide(), Vec::new());
        let at = 20 * DAY;
        for n in 0..10 {
            assert_eq!(
                answer(&mut gate, &format!("s{n}"), &format!("M{n}"), at),
                Admitted::Yes
            );
        }
        assert_eq!(gate.tokens_left(), 0);

        let later = at + 4 * 3_600;
        let mut earned = 0;
        for n in 0..12 {
            if answer(&mut gate, "asker", &format!("L{n}"), later) == Admitted::Yes {
                earned += 1;
            }
        }
        assert_eq!(
            earned, 8,
            "four hours at two an hour is eight, and the burst is ten so nothing is clipped"
        );
    }
}
