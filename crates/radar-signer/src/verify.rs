// SPDX-License-Identifier: Apache-2.0
//! What the signer checks before it signs.
//!
//! The caller sends an [`Authorization`] and some transaction bytes and says
//! they correspond. This module assumes that claim is false and looks.
//!
//! The threat model is not "a bug in the executor". It is a fully compromised
//! executor — prompt-injected, or replaced outright — that can construct any
//! transaction it likes and describe it any way it likes. The only things it
//! cannot do are forge an [`Authorization`] the kernel did not issue, and change
//! the bytes after this module has read them.
//!
//! So every check here is against the *decoded bytes*, never against anything
//! the caller said about them.

use radar_risk::{Authorization, Autonomy, Policy};
use radar_types::{Address, Slot};

use crate::tx::{DecodeError, Message, decode};

/// Programs the signer will sign an instruction for.
///
/// An allowlist rather than a denylist, because the set of programs that can
/// take a token away from you is not enumerable and the set that can trade one
/// is.
#[derive(Debug, Clone)]
pub struct Allowlist {
    /// Programs that may appear in a signed transaction.
    pub programs: Vec<[u8; 32]>,
}

/// The system program, needed for compute budget and wSOL handling.
pub const SYSTEM_PROGRAM: [u8; 32] = [0u8; 32];

/// Why the signer refused.
///
/// Every applicable reason is returned. A caller that fixed one and resubmitted
/// only to hit the next would learn nothing about whether the transaction was
/// ever going to be signable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, thiserror::Error)]
pub enum Rejection {
    /// The bytes did not decode.
    #[error("undecodable: {0}")]
    Undecodable(String),
    /// This signer's own policy authorises nothing.
    ///
    /// `Observe`, `Alert` and `Approve` cannot self-authorise, and this process
    /// signs without a human present. Checked against the policy **the signer
    /// loaded**, not the one the caller says it judged against — which is the
    /// whole point of [ADR 0008](https://github.com/hey-vera/radar/blob/main/docs/adr/0008-the-signer-holds-its-own-policy.md).
    ///
    /// This is what makes `Policy::CLOSED` closed *at the key*, rather than
    /// closed only in the process that decides.
    #[error("this signer's policy ({level}) does not authorise unattended signing")]
    AutonomyInsufficient {
        /// The level the signer is configured with.
        level: String,
    },
    /// The authorisation asks for more than this signer's policy permits.
    ///
    /// Refused rather than quietly clamped to the smaller number. A caller
    /// asking for more than the operator allowed is either a bug or an attack,
    /// and silently serving a reduced version hides both.
    #[error("authorization allows {asked} micro-USD; this signer's policy allows {allowed}")]
    AboveSignerPolicy {
        /// What the authorisation asked for.
        asked: u64,
        /// What the signer's own policy permits.
        allowed: u64,
    },
    /// The authorisation's window is longer than this signer's policy permits.
    ///
    /// Expiry is the only thing making a grant temporary, and the caller
    /// currently chooses it. An authorisation valid for a year is a standing
    /// authorisation wearing a short one's clothes.
    #[error("authorization runs {slots} slots past now; this signer's policy allows {allowed}")]
    LongerThanSignerPolicy {
        /// How far ahead the authorisation expires.
        slots: u64,
        /// The longest window the signer accepts.
        allowed: u64,
    },
    /// The authorization has expired.
    ///
    /// Checked against a slot the caller supplies, which the caller could lie
    /// about — but lying makes an expired authorization usable, not an
    /// unauthorised trade possible, and the bounds still hold. The kernel's
    /// short lifetime is the real defence.
    #[error("authorization expired at slot {expires_after}, now {now}")]
    Expired {
        /// When the authorization stopped being valid.
        expires_after: Slot,
        /// The slot the caller reported.
        now: Slot,
    },
    /// The authorization still needs an operator's signature.
    #[error("operator signature required and not present")]
    NeedsOperator,
    /// The transaction invokes a program that is not allowed.
    #[error("program not allowed: {0}")]
    ProgramNotAllowed(String),
    /// The authorised mint does not appear anywhere in the transaction.
    ///
    /// The check that catches the substitution attack: an executor that swapped
    /// the mint for another would otherwise hold a valid authorization for a
    /// trade in a different token.
    #[error("authorised mint {0} is not in the transaction")]
    MintAbsent(String),
    /// The transaction moves more lamports than the authorization permits.
    #[error("transfers {found} lamports, authorised for at most {allowed}")]
    OverSpend {
        /// What the transaction moves.
        found: u64,
        /// What the authorization permits.
        allowed: u64,
    },
    /// The fee payer is not the wallet the signer holds a key for.
    ///
    /// Signing for a fee payer we are not means signing something we cannot
    /// reason about.
    #[error("fee payer is not the signing wallet")]
    ForeignFeePayer,
    /// The transaction contains no instruction at all.
    #[error("no instructions")]
    Empty,
    /// The transaction closes or reassigns an account it should not.
    #[error("contains an account-ownership change, which no trade needs")]
    OwnershipChange,
    /// An instruction on a venue this signer knows carried data it could not
    /// read.
    ///
    /// Rule 9. An instruction whose size the signer cannot establish is not an
    /// instruction that spends nothing — it is one whose spend is unknown, and
    /// unknown is not safe. pump.fun ships new instructions (the decoder's own
    /// table went from fourteen discriminators to twenty-one), so this will fire
    /// on a program upgrade before it fires on an attack. Refusing is still
    /// right: the fix is a decoder update, and until it lands the alternative is
    /// signing a payload nobody has read.
    #[error("pump.fun instruction {0} could not be read, so its size is unknown")]
    UnreadableVenueInstruction(String),
    /// The message is a versioned (v0) transaction.
    ///
    /// [ADR 0003](https://github.com/hey-vera/radar/blob/main/docs/adr/0003-legacy-transactions-because-the-signer-must-be-able-to-read-them.md)
    /// says the signer takes legacy transactions only. Until now that held only
    /// as far as *lookup tables*: `decode` refuses a versioned message that
    /// carries them, and accepted one that did not. The ADR's reason is broader
    /// than its enforcement was — a versioned message is a format whose account
    /// resolution this process does not fully own — so the refusal belongs here,
    /// where the decision to sign is made, rather than in the decoder that other
    /// tests read versioned bytes with.
    #[error("versioned (v0) message; this signer signs legacy transactions only")]
    Versioned,
}

/// A transaction that passed every check.
///
/// Constructed only by [`check`], and holding the message so a caller cannot
/// sign different bytes from the ones that were verified. There is no way to
/// build one from an unverified message, which is what makes "verified" a fact
/// about the value rather than a claim about the control flow.
#[derive(Debug, Clone)]
pub struct Checked {
    message: Message,
    bytes: Vec<u8>,
}

impl Checked {
    /// The verified message.
    #[must_use]
    pub const fn message(&self) -> &Message {
        &self.message
    }

    /// The exact bytes that were verified.
    ///
    /// The only bytes a caller should sign. Signing anything else discards
    /// everything this module established.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The bytes to sign: the message, without the signature array.
    #[must_use]
    pub fn signable(&self) -> &[u8] {
        // The message was decoded from exactly these bytes, so the offset is in
        // range by construction; the fallback keeps the unreachable case from
        // being a panic in the process that holds the key.
        self.message.signable(&self.bytes).unwrap_or(&[])
    }
}

/// The system program's `Transfer` discriminator.
const SYSTEM_TRANSFER: u32 = 2;

/// The bounds the caller asserts about a transaction it built.
///
/// Both are the caller's word and neither is trusted as a *permission*: they can
/// only narrow what the authorization and the signer's own policy already allow.
/// A compromised caller setting `max_lamports` to `u64::MAX` gains nothing,
/// because the authorization's own ceiling still applies.
///
/// `max_lamports` exists because the authorization is denominated in micro-USD
/// and a transaction in lamports, and this process has no price feed — see
/// [`lamport_ceiling`]. The executor has one, so it converts and states the
/// result here. That makes the executor's *intent* checkable against the
/// executor's *output*: a router that inflated the buy is caught by the process
/// that holds the key rather than by the process that asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallerBounds {
    /// The caller's view of the chain head.
    pub now: Slot,
    /// The most lamports the caller intended this transaction to spend.
    pub max_lamports: u64,
}

/// Verifies transaction bytes against an authorization.
///
/// `signing_wallet` is the public key this process holds the secret for;
/// `bounds` is what the caller asserts about the chain head and the size it
/// intended, neither of which can widen anything.
///
/// # Errors
///
/// Returns every applicable [`Rejection`], sorted, so a caller sees the whole
/// picture rather than one reason at a time.
pub fn check(
    authorization: &Authorization,
    bytes: &[u8],
    signing_wallet: &Address,
    allowlist: &Allowlist,
    policy: &Policy,
    bounds: CallerBounds,
) -> Result<Checked, Vec<Rejection>> {
    let now = bounds.now;
    let message = match decode(bytes) {
        Ok(m) => m,
        // Undecodable is terminal: there is nothing further to check against.
        Err(e) => return Err(vec![Rejection::Undecodable(DecodeError::to_string(&e))]),
    };

    let mut rejections = Vec::new();

    // The signer's own policy, applied unconditionally, before anything the
    // caller asserted is used as a bound.
    //
    // Unconditional is the operative word (ADR 0008). A clamp does not depend on
    // state the caller supplies, so unlike re-running the kernel here it cannot
    // be defeated by lying about the portfolio. It answers a narrower question
    // than the kernel does, and it answers it without trusting the questioner.
    if !policy.autonomy.can_self_authorise() {
        rejections.push(Rejection::AutonomyInsufficient {
            level: format!("{:?}", policy.autonomy),
        });
    }

    // Under `Canary` the dust bound applies instead. That level exists to permit
    // exactly one thing, and inheriting the larger ceiling would make it the
    // same as `Capped`.
    let policy_ceiling = if policy.autonomy == Autonomy::Canary {
        policy.max_canary
    } else {
        policy.max_position
    };
    if authorization.max_notional > policy_ceiling {
        rejections.push(Rejection::AboveSignerPolicy {
            asked: authorization.max_notional.get(),
            allowed: policy_ceiling.get(),
        });
    }

    let window = authorization.expires_after.get().saturating_sub(now.get());
    if window > policy.max_input_staleness.get() {
        rejections.push(Rejection::LongerThanSignerPolicy {
            slots: window,
            allowed: policy.max_input_staleness.get(),
        });
    }

    if now.get() > authorization.expires_after.get() {
        rejections.push(Rejection::Expired {
            expires_after: authorization.expires_after,
            now,
        });
    }
    if authorization.needs_operator_signature {
        rejections.push(Rejection::NeedsOperator);
    }
    if message.instructions.is_empty() {
        rejections.push(Rejection::Empty);
    }
    if message.versioned {
        rejections.push(Rejection::Versioned);
    }

    for program in message.programs_outside(&allowlist.programs) {
        rejections.push(Rejection::ProgramNotAllowed(
            Address::new(program).to_string(),
        ));
    }

    if !mentions(&message, &authorization.mint) {
        rejections.push(Rejection::MintAbsent(authorization.mint.to_string()));
    }

    if message.fee_payer() != Some(*signing_wallet.as_bytes()) {
        rejections.push(Rejection::ForeignFeePayer);
    }

    // Every action gets a ceiling on outgoing lamports, and until 2026-08-31
    // only `Buy` did.
    //
    // # The hole that was here, because it is instructive
    //
    // The exemption read: "refusing to sign a sale because it is large is how a
    // position gets trapped in exactly the situation the limits exist to
    // prevent." The concern is real. The reasoning conflated two different
    // things, and the gap between them was a way to empty a wallet.
    //
    // A large *sale* is tokens leaving through a DEX and lamports arriving.
    // [`lamports_transferred`] counts neither of those: it sums **system-program
    // transfers out**. A legitimate exit moves almost none -- rent for an
    // account, a fee -- so bounding it does not trap anything.
    //
    // What the exemption did allow: given any `Exit` authorisation, a
    // transaction that lists the authorised mint as an inert account, uses only
    // allowlisted programs (the system program is necessarily one), pays its fee
    // from the right wallet, and transfers **the entire balance to any address**
    // passed every check. Demonstrated before this was changed: a 100 SOL
    // transfer to an unrelated address, authorised against an `Exit` whose
    // notional was one micro-dollar.
    //
    // The ceiling used is the authorisation's own, not a tighter rent-sized
    // constant. A constant would be better and this is not the moment to invent
    // one: no real exit has ever been signed, so any number here would be a
    // guess, and a guess that is too small traps the position the old comment
    // was rightly worried about. This bound is finite, which is the property
    // that was missing.
    //
    // # The second hole that was here, and it is the one that mattered
    //
    // Until 2026-09-06 `lamports_transferred` was the *whole* of the size check,
    // and it counts system-program transfers. **A swap makes none.** A pump.fun
    // buy carries its lamports in the instruction's own data, which this process
    // never decoded, so every buy scored zero against every ceiling.
    //
    // The composition test in `radar-pumpfun` documented it as passing:
    // a 20,000,000-lamport buy was signed under an authorisation whose ceiling
    // was 5,000,000. Rule 1 says the signer re-derives the transaction and checks
    // it against the authorisation's bounds; for the only kind of transaction
    // Radar would ever sign, it did not. LEARNINGS 33.
    let bought = match lamports_bought(&message) {
        Ok(v) => v,
        Err(unreadable) => {
            rejections.extend(unreadable);
            // Not a value that can be compared, so the size check is skipped
            // rather than run against a total that is missing an instruction.
            // The rejection above already fails the transaction.
            rejections.sort();
            rejections.dedup();
            return Err(rejections);
        }
    };
    let moved = lamports_transferred(&message).saturating_add(bought);
    // The tightest of the three. The authorisation may narrow the signer's
    // policy and the caller may narrow the authorisation; neither may widen, and
    // a caller that tried is already refused above.
    let ceiling = lamport_ceiling(authorization)
        .min(policy_ceiling.get())
        .min(bounds.max_lamports);
    if moved > ceiling {
        rejections.push(Rejection::OverSpend {
            found: moved,
            allowed: ceiling,
        });
    }

    if changes_ownership(&message) {
        rejections.push(Rejection::OwnershipChange);
    }

    if rejections.is_empty() {
        Ok(Checked {
            message,
            bytes: bytes.to_vec(),
        })
    } else {
        rejections.sort();
        rejections.dedup();
        Err(rejections)
    }
}

/// Whether an address appears anywhere in the message's account list.
fn mentions(message: &Message, address: &Address) -> bool {
    message.accounts.iter().any(|a| a == address.as_bytes())
}

/// Total lamports moved by system-program transfers.
///
/// Deliberately sums *every* transfer rather than looking for one. A transaction
/// that splits an overspend across three instructions is the obvious way around
/// a check that only inspects the first.
fn lamports_transferred(message: &Message) -> u64 {
    message
        .instructions
        .iter()
        .filter(|i| i.program_id == SYSTEM_PROGRAM)
        .filter_map(|i| {
            let discriminator = u32::from_le_bytes(i.data.get(0..4)?.try_into().ok()?);
            if discriminator != SYSTEM_TRANSFER {
                return None;
            }
            Some(u64::from_le_bytes(i.data.get(4..12)?.try_into().ok()?))
        })
        .fold(0u64, u64::saturating_add)
}

/// Lamports leaving the wallet through a swap on a venue this signer can read.
///
/// # Why buys and not sells
///
/// A buy sends lamports out and the amount is in the instruction: either the
/// exact SOL in (`buy_exact_sol_in`, `buy_exact_quote_in_v2`) or the maximum SOL
/// cost accepted (`buy`, `buy_v2`). Both are upper bounds on what leaves, which
/// is exactly what a ceiling needs.
///
/// A **sell** sends tokens out and brings lamports in. Its lamport field is a
/// *minimum acceptable output*, not a spend, and adding it to the total would
/// refuse a large exit for being large — the trap the comment above
/// [`lamports_transferred`]'s call site describes. So sells contribute nothing
/// here. That leaves the size of a sale unbounded by this process, and it is
/// deliberately unbounded: the bound a sale needs is on **tokens**, and the
/// authorization carries no token quantity to check one against. Recorded as a
/// known gap rather than closed with a number nobody has measured.
///
/// # Errors
///
/// Returns [`Rejection::UnreadableVenueInstruction`] for every pump.fun
/// instruction whose discriminator is not in the decoder's table or whose
/// arguments are truncated. Rule 9: an unreadable size is unknown, not zero.
fn lamports_bought(message: &Message) -> Result<u64, Vec<Rejection>> {
    let program = radar_decode::Program::PumpFun;
    let mut total = 0u64;
    let mut unreadable = Vec::new();
    // The venue is named once, here, and the decoder is given it. It is
    // deliberately `PumpFun` and nothing else: this process signs the bonding
    // curve and only the bonding curve, and PumpSwap's `buy` carries the same
    // eight bytes, so a filter that widened by accident would have read an AMM
    // trade as a curve trade and bounded a spend against the wrong pool.
    for instruction in message
        .instructions
        .iter()
        .filter(|i| radar_decode::Program::at(&Address::new(i.program_id)) == Some(program))
    {
        let Some(known) = radar_decode::decode(program, &instruction.data)
            .known()
            .copied()
            .and_then(radar_decode::Instruction::pumpfun)
        else {
            unreadable.push(Rejection::UnreadableVenueInstruction(
                radar_decode::Discriminator::from_data(&instruction.data)
                    .map_or_else(|| "«under eight bytes»".to_owned(), |d| d.to_string()),
            ));
            continue;
        };
        if !known.is_buy() {
            continue;
        }
        // A buy that decodes to a known variant but whose arguments are
        // truncated is refused rather than skipped: the discriminator said this
        // instruction spends, and the payload would not say how much.
        let Some(Ok(trade)) = radar_decode::pumpfun::trade_args(known, &instruction.data) else {
            unreadable.push(Rejection::UnreadableVenueInstruction(
                known.anchor_name().to_owned(),
            ));
            continue;
        };
        // Exactly one of the two fields is lamports, and which one depends on
        // the variant — `radar_decode::args` carries the unit in the type so
        // this cannot read a token amount as money. `u64::MAX` as a max cost
        // means the trader accepted any price; saturating here turns that into a
        // refusal, which is the right answer for an unbounded buy.
        let lamports = trade
            .exact
            .lamports()
            .or_else(|| trade.limit.lamports())
            .unwrap_or(u64::MAX);
        total = total.saturating_add(lamports);
    }
    if unreadable.is_empty() {
        Ok(total)
    } else {
        Err(unreadable)
    }
}

/// The lamport ceiling implied by an authorization's notional.
///
/// The authorization is denominated in micro-USD and the transaction in
/// lamports, and this process has no price feed — deliberately, since a signer
/// with a price feed has one more input to be lied to by.
///
/// So the conversion is left to the caller, who states it in
/// [`CallerBounds::max_lamports`], and this stays as the floor under it: the notional
/// read as lamports, a ceiling far tighter than any real trade at any real SOL
/// price, so it fails closed rather than open.
///
/// **The residual, stated plainly.** Because this is the tighter of the two in
/// every realistic case, a genuine trade sized from a real price will be refused
/// by it, and the caller's honest conversion cannot lift it. So today the pair
/// bounds *safety* correctly and cannot yet size a real trade. The fix is a
/// lamport-denominated `Policy` — the signer holding its own ceiling in the unit
/// the chain uses — which is a decision about what the operator's limit *means*
/// and belongs in an ADR rather than in a patch. Nothing trades
/// (`Policy::CLOSED`), so the order is right: hold the bound, then argue the
/// unit.
const fn lamport_ceiling(authorization: &Authorization) -> u64 {
    authorization.max_notional.get()
}

/// Whether the message contains a system-program `Assign` or `CloseAccount`.
///
/// No trade needs to change who owns an account. One that does is either a bug
/// or an attempt to take the wallet.
fn changes_ownership(message: &Message) -> bool {
    /// The system program's `Assign` discriminator.
    const SYSTEM_ASSIGN: u32 = 1;
    /// `AssignWithSeed`.
    const SYSTEM_ASSIGN_WITH_SEED: u32 = 10;

    message
        .instructions
        .iter()
        .filter(|i| i.program_id == SYSTEM_PROGRAM)
        .any(|i| {
            i.data.get(0..4).is_some_and(|d| {
                let discriminator = u32::from_le_bytes(d.try_into().unwrap_or([0; 4]));
                discriminator == SYSTEM_ASSIGN || discriminator == SYSTEM_ASSIGN_WITH_SEED
            })
        })
}

/// A verified transaction for other modules' tests.
///
/// Exists so the key module can prove it signs the bytes that were checked,
/// without a second path that constructs a [`Checked`] from unverified input.
/// That path is the one that would eventually get called from production.
#[cfg(test)]
pub mod tests_support {
    use radar_risk::{Action, Authorization, Autonomy, Policy};
    use radar_types::{Address, MicroUsd, Slot};

    use super::{Allowlist, Checked, SYSTEM_PROGRAM, check};

    /// A transaction that passes every check.
    ///
    /// # Panics
    ///
    /// Panics if the fixture stops verifying, which means a check changed and
    /// this fixture no longer describes a signable transaction.
    #[must_use]
    pub fn checked_fixture() -> Checked {
        const DEX: [u8; 32] = [0x11; 32];
        const MINT: [u8; 32] = [0x22; 32];
        const WALLET: [u8; 32] = [0x33; 32];

        let mut bytes = vec![0u8, 1, 0, 0, 4];
        for a in [WALLET, MINT, DEX, SYSTEM_PROGRAM] {
            bytes.extend_from_slice(&a);
        }
        bytes.extend_from_slice(&[0xAA; 32]);
        bytes.extend_from_slice(&[1, 2, 2, 0, 1, 2, 0xAB, 0xCD]);

        let authorization = Authorization {
            nonce: "fixture".to_owned(),
            mint: Address::new(MINT),
            action: Action::Buy,
            max_notional: MicroUsd(50_000_000),
            expires_after: Slot(1_150),
            needs_operator_signature: false,
        };
        check(
            &authorization,
            &bytes,
            &Address::new(WALLET),
            &Allowlist {
                programs: vec![DEX, SYSTEM_PROGRAM],
            },
            // Wide on purpose. This fixture exists so other crates can get a
            // `Checked` without reconstructing a transaction, and a policy that
            // refused it would make every one of those tests fail for a reason
            // that has nothing to do with what they are testing.
            &Policy {
                autonomy: Autonomy::Capped,
                max_position: MicroUsd(1_000_000_000),
                max_canary: MicroUsd(1_000_000_000),
                max_input_staleness: radar_types::SlotDelta(100_000),
                ..Policy::CLOSED
            },
            super::CallerBounds {
                now: Slot(1_000),
                max_lamports: u64::MAX,
            },
        )
        .expect("the fixture must verify")
    }
}

#[cfg(test)]
mod tests {
    use radar_risk::Action;
    use radar_types::MicroUsd;

    use super::*;

    const DEX: [u8; 32] = [0x11; 32];
    const MINT: [u8; 32] = [0x22; 32];
    const WALLET: [u8; 32] = [0x33; 32];
    const NOW: Slot = Slot(1_000);

    /// A policy wide enough not to interfere with tests about other things.
    ///
    /// The clamp gets its own tests below. Everywhere else it must be out of the
    /// way, or a refusal could come from the policy rather than from the
    /// property under test.
    fn policy() -> Policy {
        Policy {
            autonomy: Autonomy::Capped,
            max_position: MicroUsd(1_000_000_000),
            max_canary: MicroUsd(1_000_000_000),
            max_input_staleness: radar_types::SlotDelta(100_000),
            ..Policy::CLOSED
        }
    }

    fn allowlist() -> Allowlist {
        Allowlist {
            programs: vec![DEX, SYSTEM_PROGRAM],
        }
    }

    fn authorization() -> Authorization {
        Authorization {
            nonce: "test".to_owned(),
            mint: Address::new(MINT),
            action: Action::Buy,
            max_notional: MicroUsd(50_000_000),
            expires_after: Slot(1_150),
            needs_operator_signature: false,
        }
    }

    /// Builds a transaction over the given account set.
    ///
    /// `accounts[0]` is the fee payer. Instructions are `(program_index,
    /// account_indices, data)`.
    fn build(accounts: &[[u8; 32]], instructions: &[(u8, Vec<u8>, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0u8];
        out.push(1);
        out.push(0);
        out.push(0);
        out.push(u8::try_from(accounts.len()).expect("small"));
        for a in accounts {
            out.extend_from_slice(a);
        }
        out.extend_from_slice(&[0xAA; 32]);
        out.push(u8::try_from(instructions.len()).expect("small"));
        for (program, indices, data) in instructions {
            out.push(*program);
            out.push(u8::try_from(indices.len()).expect("small"));
            out.extend_from_slice(indices);
            out.push(u8::try_from(data.len()).expect("small"));
            out.extend_from_slice(data);
        }
        out
    }

    /// pump.fun's program id, as the message carries it.
    const PUMP: [u8; 32] = *radar_decode::pumpfun::PROGRAM_ID.as_bytes();

    /// An allowlist that also permits the venue, so a refusal in a size test is
    /// about the size rather than about the program.
    fn venue_allowlist() -> Allowlist {
        Allowlist {
            programs: vec![DEX, SYSTEM_PROGRAM, PUMP],
        }
    }

    /// A pump.fun trade instruction's data: discriminator, then two `u64`s.
    fn venue_trade(ix: radar_decode::pumpfun::Instruction, first: u64, second: u64) -> Vec<u8> {
        let mut data = ix.discriminator().as_bytes().to_vec();
        data.extend_from_slice(&first.to_le_bytes());
        data.extend_from_slice(&second.to_le_bytes());
        data
    }

    /// A transaction carrying one pump.fun instruction over the usual accounts.
    fn venue_tx(data: Vec<u8>) -> Vec<u8> {
        build(
            &[WALLET, MINT, PUMP, SYSTEM_PROGRAM],
            &[(2, vec![0, 1], data)],
        )
    }

    /// `check` against the venue allowlist, with the caller's own ceiling given.
    fn check_venue(bytes: &[u8], max_lamports: u64) -> Result<Checked, Vec<Rejection>> {
        check(
            &authorization(),
            bytes,
            &Address::new(WALLET),
            &venue_allowlist(),
            &policy(),
            CallerBounds {
                now: NOW,
                max_lamports,
            },
        )
    }

    /// A system transfer instruction's data.
    fn transfer(lamports: u64) -> Vec<u8> {
        let mut data = SYSTEM_TRANSFER.to_le_bytes().to_vec();
        data.extend_from_slice(&lamports.to_le_bytes());
        data
    }

    /// The honest transaction every test then damages one field of.
    fn honest() -> Vec<u8> {
        build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[(2, vec![0, 1], vec![0xAB, 0xCD])],
        )
    }

    #[test]
    fn an_honest_transaction_is_signed() {
        let checked = check(
            &authorization(),
            &honest(),
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect("should verify");
        assert_eq!(checked.bytes(), honest());
    }

    #[test]
    fn a_substituted_mint_is_refused() {
        // The attack the whole process exists for: the executor holds a valid
        // authorization for one token and builds a transaction for another.
        let other = build(
            &[WALLET, [0x99; 32], DEX, SYSTEM_PROGRAM],
            &[(2, vec![0, 1], vec![0xAB])],
        );
        let rejections = check(
            &authorization(),
            &other,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert!(
            rejections
                .iter()
                .any(|r| matches!(r, Rejection::MintAbsent(_)))
        );
    }

    #[test]
    fn an_unlisted_program_is_refused() {
        let evil = build(
            &[WALLET, MINT, [0xEE; 32], SYSTEM_PROGRAM],
            &[(2, vec![0, 1], vec![0xAB])],
        );
        let rejections = check(
            &authorization(),
            &evil,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert!(
            rejections
                .iter()
                .any(|r| matches!(r, Rejection::ProgramNotAllowed(_)))
        );
    }

    #[test]
    fn an_oversized_spend_is_refused() {
        let big = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[
                (2, vec![0, 1], vec![0xAB]),
                (3, vec![0, 1], transfer(60_000_000)),
            ],
        );
        let rejections = check(
            &authorization(),
            &big,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert!(
            rejections
                .iter()
                .any(|r| matches!(r, Rejection::OverSpend { .. }))
        );
    }

    #[test]
    fn a_spend_split_across_instructions_is_still_caught() {
        // The obvious way around a check that inspects only the first transfer.
        let split = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[
                (2, vec![0, 1], vec![0xAB]),
                (3, vec![0, 1], transfer(30_000_000)),
                (3, vec![0, 1], transfer(30_000_000)),
            ],
        );
        let rejections = check(
            &authorization(),
            &split,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert_eq!(
            rejections
                .iter()
                .filter(|r| matches!(
                    r,
                    Rejection::OverSpend {
                        found: 60_000_000,
                        ..
                    }
                ))
                .count(),
            1
        );
    }

    #[test]
    fn a_spend_within_the_ceiling_is_signed() {
        let ok = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[
                (2, vec![0, 1], vec![0xAB]),
                (3, vec![0, 1], transfer(10_000_000)),
            ],
        );
        assert!(
            check(
                &authorization(),
                &ok,
                &Address::new(WALLET),
                &allowlist(),
                &policy(),
                unbounded(NOW)
            )
            .is_ok()
        );
    }

    #[test]
    fn a_large_sale_is_still_signed() {
        // The concern the old exemption was protecting, stated correctly this
        // time. Refusing to sign a sale because it is large is how a position
        // gets trapped, so a big exit must still go through.
        //
        // What makes it big is the DEX instruction, not lamports leaving the
        // wallet -- a sale sends tokens out and brings lamports in. That is why
        // bounding outgoing system transfers does not trap anything, and why the
        // old exemption was solving this problem with the wrong tool.
        let mut auth = authorization();
        auth.action = Action::Exit;
        let sale = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            // Opaque, and deliberately so: whatever amount this sale is for is
            // inside the DEX instruction, where the signer cannot read it. A
            // hundred bytes rather than more, because the fixture writes a
            // single-byte shortvec length and anything past 127 is not a valid
            // transaction -- a malformed fixture would fail this test for a
            // reason that has nothing to do with the property.
            &[(2, vec![0, 1], vec![0xFF; 100])],
        );
        let outcome = check(
            &auth,
            &sale,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        );
        assert!(
            outcome.is_ok(),
            "a large sale must still be signable: {outcome:?}"
        );
    }

    #[test]
    fn an_exit_authorisation_cannot_move_the_balance_to_a_stranger() {
        // The hole this replaced, kept as the test that would have caught it.
        //
        // Until 2026-08-31 only `Buy` was capped. So any `Exit` authorisation --
        // one micro-dollar of notional was enough -- could be spent on a
        // transaction that listed the mint as an inert account, used only
        // allowlisted programs, paid its fee from the right wallet, and
        // transferred the entire balance somewhere nobody authorised. Every
        // check passed.
        //
        // A hundred SOL, against a notional of one micro-dollar.
        const STRANGER: [u8; 32] = [0x66; 32];
        let mut auth = authorization();
        auth.action = Action::Exit;
        auth.max_notional = MicroUsd(1);

        let drain = build(
            &[WALLET, MINT, STRANGER, SYSTEM_PROGRAM],
            &[(3, vec![0, 2], transfer(100_000_000_000))],
        );
        let rejections = check(
            &auth,
            &drain,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("draining the wallet must be refused");
        assert!(
            rejections
                .iter()
                .any(|r| matches!(r, Rejection::OverSpend { .. })),
            "expected an overspend rejection, got {rejections:?}"
        );
    }

    #[test]
    fn the_ceiling_is_inclusive_and_one_lamport_past_it_is_not() {
        // The boundary itself, swept. `just mutants` found `>` could become
        // `>=` with every test still passing, which means nothing exercised a
        // transfer of exactly the ceiling.
        //
        // Both directions are wrong in a way that matters. Exclusive refuses a
        // transaction that is precisely within the operator's limit, which reads
        // as an unexplained failure at round numbers. Off by one the other way
        // is an overspend, small but of exactly the kind these bounds exist to
        // make impossible.
        let mut auth = authorization();
        auth.action = Action::Buy;
        auth.max_notional = MicroUsd(1_000_000);

        let at_the_ceiling = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[(3, vec![0, 2], transfer(1_000_000))],
        );
        assert!(
            check(
                &auth,
                &at_the_ceiling,
                &Address::new(WALLET),
                &allowlist(),
                &policy(),
                unbounded(NOW)
            )
            .is_ok(),
            "exactly the ceiling is inside the limit"
        );

        let one_over = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[(3, vec![0, 2], transfer(1_000_001))],
        );
        assert!(
            check(
                &auth,
                &one_over,
                &Address::new(WALLET),
                &allowlist(),
                &policy(),
                unbounded(NOW)
            )
            .is_err(),
            "one lamport past it is not"
        );
    }

    #[test]
    fn a_reduce_is_capped_too() {
        // The third action, and it was exempt for the same reason. Swept rather
        // than sampled, because "Buy is capped" was true before this change as
        // well and asserting only that would not have caught anything.
        let mut auth = authorization();
        auth.action = Action::Reduce;
        auth.max_notional = MicroUsd(1);

        let drain = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[(3, vec![0, 2], transfer(100_000_000_000))],
        );
        assert!(
            check(
                &auth,
                &drain,
                &Address::new(WALLET),
                &allowlist(),
                &policy(),
                unbounded(NOW)
            )
            .is_err(),
            "a reduce must be capped as well"
        );
    }

    /// [`check`] with the fixtures that are the same in every clamp test.
    ///
    /// Only the transaction, the authorisation, the policy and the slot vary
    /// here, and naming just those keeps each test about the one thing it is
    /// varying.
    fn check_under(
        bytes: &[u8],
        authorization: &Authorization,
        policy: &Policy,
        now: Slot,
    ) -> Result<Checked, Vec<Rejection>> {
        check(
            authorization,
            bytes,
            &Address::new(WALLET),
            &allowlist(),
            policy,
            unbounded(now),
        )
    }

    /// Bounds that assert nothing beyond the slot.
    ///
    /// `u64::MAX` so the caller's own ceiling never binds, leaving the
    /// authorisation's as the one under test. Every test about the caller's
    /// ceiling states it explicitly.
    const fn unbounded(now: Slot) -> CallerBounds {
        CallerBounds {
            now,
            max_lamports: u64::MAX,
        }
    }

    #[test]
    fn an_authorisation_wider_than_the_signers_policy_is_refused() {
        // The only case this change is about.
        //
        // The signer does not verify that an `Authorization` came from the
        // kernel -- no MAC, and the nonce is checked against nothing -- so its
        // bounds are the caller's claim about what was approved. Before ADR
        // 0008 that claim was the only ceiling there was.
        //
        // Refused rather than quietly clamped to the smaller number: a caller
        // asking for more than the operator allowed is either a bug or an
        // attack, and silently serving a reduced version hides both.
        let mut auth = authorization();
        auth.max_notional = MicroUsd(500_000_000);

        let tight = Policy {
            max_position: MicroUsd(10_000_000),
            ..policy()
        };
        let rejections = check_under(&honest(), &auth, &tight, Slot(1_000))
            .expect_err("an authorisation above the signer's policy must be refused");

        assert!(
            rejections.iter().any(|r| matches!(
                r,
                Rejection::AboveSignerPolicy {
                    asked: 500_000_000,
                    allowed: 10_000_000
                }
            )),
            "expected an AboveSignerPolicy naming both numbers, got {rejections:?}"
        );
    }

    #[test]
    fn an_authorisation_exactly_at_the_policy_ceiling_is_allowed() {
        // The boundary, swept. `just mutants` turned `>` into `>=` here and
        // every test still passed, meaning nothing exercised an authorisation
        // equal to the policy's ceiling.
        //
        // Exclusive would refuse an authorisation precisely at the operator's
        // limit, which reads as an unexplained failure at exactly the round
        // number an operator is most likely to configure.
        let at_the_limit = Policy {
            max_position: MicroUsd(50_000_000),
            ..policy()
        };
        let mut auth = authorization();
        auth.max_notional = MicroUsd(50_000_000);
        assert!(
            check_under(&honest(), &auth, &at_the_limit, Slot(1_000)).is_ok(),
            "exactly the policy ceiling is inside the policy"
        );

        auth.max_notional = MicroUsd(50_000_001);
        assert!(
            check_under(&honest(), &auth, &at_the_limit, Slot(1_000)).is_err(),
            "one micro-dollar past it is not"
        );
    }

    #[test]
    fn a_closed_policy_signs_nothing_whatever_the_caller_claims() {
        // `Policy::CLOSED` enforced *at the key*, rather than only in the
        // process that decides.
        //
        // The authorisation here is perfectly formed and entirely within its own
        // bounds. Before this change the signer would have signed it, because
        // nothing in this process had an opinion about whether Radar was
        // trading at all.
        let rejections = check_under(&honest(), &authorization(), &Policy::CLOSED, Slot(1_000))
            .expect_err("a closed policy must sign nothing");

        assert!(
            rejections
                .iter()
                .any(|r| matches!(r, Rejection::AutonomyInsufficient { .. })),
            "expected AutonomyInsufficient, got {rejections:?}"
        );
    }

    #[test]
    fn every_level_that_cannot_self_authorise_refuses() {
        // Swept rather than sampled. `Approve` is the interesting one: the
        // kernel sets `needs_operator_signature` for it, but that flag is on the
        // authorisation -- which the caller writes. The signer's own policy is
        // what makes the level stick.
        for level in [Autonomy::Observe, Autonomy::Alert, Autonomy::Approve] {
            let closed = Policy {
                autonomy: level,
                ..policy()
            };
            assert!(
                check_under(&honest(), &authorization(), &closed, Slot(1_000)).is_err(),
                "{level:?} must not authorise unattended signing"
            );
        }

        for level in [Autonomy::Capped, Autonomy::Auto] {
            let open = Policy {
                autonomy: level,
                ..policy()
            };
            assert!(
                check_under(&honest(), &authorization(), &open, Slot(1_000)).is_ok(),
                "{level:?} must still sign, or this test proves nothing"
            );
        }
    }

    #[test]
    fn canary_is_bounded_by_the_dust_limit_and_not_the_position_limit() {
        // The level exists to permit exactly one thing: a dust round trip.
        // Inheriting `max_position` would make it indistinguishable from
        // `Capped`, which is the whole distinction it carries.
        let canary = Policy {
            autonomy: Autonomy::Canary,
            max_position: MicroUsd(1_000_000_000),
            max_canary: MicroUsd(2_000_000),
            ..policy()
        };

        let mut small = authorization();
        small.max_notional = MicroUsd(1_000_000);
        assert!(
            check_under(&honest(), &small, &canary, Slot(1_000)).is_ok(),
            "a dust authorisation is what Canary is for"
        );

        let mut large = authorization();
        large.max_notional = MicroUsd(500_000_000);
        assert!(
            check_under(&honest(), &large, &canary, Slot(1_000)).is_err(),
            "Canary must not inherit the position limit"
        );
    }

    #[test]
    fn an_authorisation_valid_for_far_too_long_is_refused() {
        // Expiry is the only thing making a grant temporary, and the caller
        // chooses it. An authorisation good for a year is a standing
        // authorisation wearing a short one's clothes.
        let short = Policy {
            max_input_staleness: radar_types::SlotDelta(150),
            ..policy()
        };

        let mut auth = authorization();
        auth.expires_after = Slot(1_150);
        assert!(
            check_under(&honest(), &auth, &short, Slot(1_000)).is_ok(),
            "exactly the permitted window is inside it"
        );

        auth.expires_after = Slot(1_151);
        assert!(
            check_under(&honest(), &auth, &short, Slot(1_000)).is_err(),
            "one slot past the permitted window is not"
        );
    }

    #[test]
    fn the_transfer_ceiling_is_the_tighter_of_the_two() {
        // An authorisation may narrow the signer's policy; it may never widen
        // it. A caller that tried to widen is already refused, so what this
        // covers is the other order: a *narrow* authorisation under a wide
        // policy must still be the bound that applies.
        let mut auth = authorization();
        auth.action = Action::Buy;
        auth.max_notional = MicroUsd(1_000);

        let wide = Policy {
            max_position: MicroUsd(1_000_000_000),
            ..policy()
        };
        let over_the_authorisation = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[(3, vec![0, 2], transfer(500_000))],
        );
        assert!(
            check_under(&over_the_authorisation, &auth, &wide, Slot(1_000)).is_err(),
            "the narrower of the two bounds is the one that applies"
        );
    }

    #[test]
    fn an_expired_authorization_is_refused() {
        let rejections = check(
            &authorization(),
            &honest(),
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(Slot(2_000)),
        )
        .expect_err("must refuse");
        assert!(
            rejections
                .iter()
                .any(|r| matches!(r, Rejection::Expired { .. }))
        );
    }

    #[test]
    fn an_authorization_awaiting_an_operator_is_refused() {
        let mut auth = authorization();
        auth.needs_operator_signature = true;
        let rejections = check(
            &auth,
            &honest(),
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert!(rejections.contains(&Rejection::NeedsOperator));
    }

    #[test]
    fn a_foreign_fee_payer_is_refused() {
        // Signing for a wallet we are not is signing something we cannot reason
        // about.
        let rejections = check(
            &authorization(),
            &honest(),
            &Address::new([0x77; 32]),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert!(rejections.contains(&Rejection::ForeignFeePayer));
    }

    #[test]
    fn an_ownership_change_is_refused() {
        // No trade needs to reassign an account. One that does is either a bug
        // or an attempt to take the wallet.
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend_from_slice(&[0xEE; 32]);
        let evil = build(
            &[WALLET, MINT, DEX, SYSTEM_PROGRAM],
            &[(2, vec![0, 1], vec![0xAB]), (3, vec![0], data)],
        );
        let rejections = check(
            &authorization(),
            &evil,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert!(rejections.contains(&Rejection::OwnershipChange));
    }

    #[test]
    fn an_empty_transaction_is_refused() {
        let empty = build(&[WALLET, MINT, DEX, SYSTEM_PROGRAM], &[]);
        let rejections = check(
            &authorization(),
            &empty,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert!(rejections.contains(&Rejection::Empty));
    }

    #[test]
    fn every_reason_is_reported_not_just_the_first() {
        // A caller fixing one problem and resubmitting only to hit the next has
        // learned nothing about whether this was ever going to be signable.
        let mut auth = authorization();
        auth.needs_operator_signature = true;
        let bad = build(
            &[[0x77; 32], [0x99; 32], [0xEE; 32], SYSTEM_PROGRAM],
            &[(2, vec![0, 1], vec![0xAB])],
        );
        let rejections = check(
            &auth,
            &bad,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(Slot(9_999)),
        )
        .expect_err("must refuse");
        assert!(rejections.len() >= 5, "got {rejections:?}");
    }

    #[test]
    fn undecodable_bytes_refuse_without_pretending_to_check_anything() {
        // Reporting five rejections about a transaction that does not exist
        // would be five statements with no evidence behind them.
        let rejections = check(
            &authorization(),
            &[0xFF; 8],
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("must refuse");
        assert_eq!(rejections.len(), 1);
        assert!(matches!(rejections[0], Rejection::Undecodable(_)));
    }

    #[test]
    fn the_checked_bytes_are_the_verified_bytes() {
        // There is no path from an unverified message to a Checked, so a caller
        // cannot sign bytes other than the ones this module read.
        let bytes = honest();
        let checked = check(
            &authorization(),
            &bytes,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect("verifies");
        assert_eq!(checked.bytes(), bytes.as_slice());
        assert_eq!(checked.message().instructions.len(), 1);
    }

    // --- the size of a swap ---------------------------------------------------
    //
    // Every test below fails against the code as it stood on 2026-09-06, when
    // the only thing counted toward a ceiling was a system-program transfer and
    // a swap makes none of those.

    #[test]
    fn a_buy_that_spends_more_than_the_authorisation_is_refused() {
        // The hole. `authorization()` carries 50,000,000 micro-USD, read as
        // lamports; this buy pins 60,000,000 lamports exactly. Before the fix
        // the transaction scored zero against every ceiling and was signed.
        use radar_decode::pumpfun::Instruction;
        let rejections = check_venue(
            &venue_tx(venue_trade(Instruction::BuyExactSolIn, 60_000_000, 0)),
            u64::MAX,
        )
        .expect_err("a buy above the ceiling must be refused");
        assert!(
            rejections.iter().any(|r| matches!(
                r,
                Rejection::OverSpend {
                    found: 60_000_000,
                    allowed: 50_000_000
                }
            )),
            "expected the size check to fire on the swap, got {rejections:?}"
        );
    }

    #[test]
    fn a_buy_inside_the_authorisation_is_signed() {
        // The other half. A check that refused everything would pass the test
        // above and be worthless.
        use radar_decode::pumpfun::Instruction;
        assert!(
            check_venue(
                &venue_tx(venue_trade(Instruction::BuyExactSolIn, 50_000_000, 0)),
                u64::MAX,
            )
            .is_ok(),
            "exactly the ceiling is inside it"
        );
        assert!(
            check_venue(
                &venue_tx(venue_trade(Instruction::BuyExactSolIn, 50_000_001, 0)),
                u64::MAX,
            )
            .is_err(),
            "one lamport past it is not"
        );
    }

    #[test]
    fn a_token_exact_buy_is_bounded_by_the_sol_it_accepts() {
        // `buy` and `buy_v2` put the token amount first and the maximum SOL cost
        // second, so the field that is money is the *second* one. Reading the
        // first as lamports is the six-orders-of-magnitude error
        // `radar_decode::args` exists to make impossible; this asserts the
        // signer reads the right one.
        use radar_decode::pumpfun::Instruction;
        // A huge token amount with a small SOL bound: signable.
        assert!(
            check_venue(
                &venue_tx(venue_trade(Instruction::Buy, 3_614_520_997_424, 1_000_000)),
                u64::MAX,
            )
            .is_ok(),
            "the token quantity is not a spend"
        );
        // A small token amount with a huge SOL bound: refused.
        assert!(
            check_venue(
                &venue_tx(venue_trade(Instruction::Buy, 1_000, 60_000_000)),
                u64::MAX,
            )
            .is_err(),
            "the SOL bound is the spend"
        );
    }

    #[test]
    fn a_buy_that_accepted_any_price_is_refused() {
        // `u64::MAX` as a maximum cost means the trader accepted any price. That
        // is a real and common thing to see on chain, and it is not something
        // this signer may agree to: an unbounded buy has no ceiling to check.
        use radar_decode::pumpfun::Instruction;
        assert!(
            check_venue(
                &venue_tx(venue_trade(Instruction::Buy, 1_000, u64::MAX)),
                u64::MAX,
            )
            .is_err()
        );
    }

    #[test]
    fn buys_split_across_instructions_are_summed() {
        // The obvious way around a check that inspects one instruction: two buys
        // of 30,000,000 under a ceiling of 50,000,000.
        use radar_decode::pumpfun::Instruction;
        let bytes = build(
            &[WALLET, MINT, PUMP, SYSTEM_PROGRAM],
            &[
                (
                    2,
                    vec![0, 1],
                    venue_trade(Instruction::BuyExactSolIn, 30_000_000, 0),
                ),
                (
                    2,
                    vec![0, 1],
                    venue_trade(Instruction::BuyExactSolIn, 30_000_000, 0),
                ),
            ],
        );
        assert!(check_venue(&bytes, u64::MAX).is_err());
    }

    #[test]
    fn a_swap_and_a_transfer_are_counted_together() {
        // Neither alone is over; together they are. A check that took the larger
        // rather than the sum would pass this.
        use radar_decode::pumpfun::Instruction;
        let bytes = build(
            &[WALLET, MINT, PUMP, SYSTEM_PROGRAM],
            &[
                (
                    2,
                    vec![0, 1],
                    venue_trade(Instruction::BuyExactSolIn, 30_000_000, 0),
                ),
                (3, vec![0, 1], transfer(30_000_000)),
            ],
        );
        assert!(check_venue(&bytes, u64::MAX).is_err());
    }

    #[test]
    fn a_large_sell_is_not_refused_for_being_large() {
        // The trap the old exemption was right to worry about, kept shut. A sell
        // sends tokens out and lamports *in*; its lamport field is a minimum
        // acceptable output, not a spend. Counting it would refuse an exit for
        // being profitable, which traps a position in exactly the situation the
        // limits exist to prevent.
        use radar_decode::pumpfun::Instruction;
        assert!(
            check_venue(
                &venue_tx(venue_trade(Instruction::Sell, 500_000_000_000, 900_000_000)),
                u64::MAX,
            )
            .is_ok(),
            "a sell's minimum output is not a spend"
        );
    }

    #[test]
    fn an_unknown_instruction_on_the_venue_is_refused_rather_than_scored_zero() {
        // Rule 9 in the process that holds the key. A discriminator the decoder
        // does not have is an instruction whose size is *unknown*, and the
        // convenient default -- treat it as spending nothing -- is the one that
        // empties a wallet on the next program upgrade.
        let mut data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33];
        data.extend_from_slice(&u64::MAX.to_le_bytes());
        let rejections =
            check_venue(&venue_tx(data), u64::MAX).expect_err("an unreadable instruction");
        assert!(
            rejections
                .iter()
                .any(|r| matches!(r, Rejection::UnreadableVenueInstruction(_))),
            "got {rejections:?}"
        );
    }

    #[test]
    fn a_truncated_buy_is_refused_rather_than_scored_zero() {
        // The discriminator says this instruction spends; the payload does not
        // say how much. Skipping it would score a spend of zero.
        use radar_decode::pumpfun::Instruction;
        let data = Instruction::BuyExactSolIn
            .discriminator()
            .as_bytes()
            .to_vec();
        let rejections =
            check_venue(&venue_tx(data), u64::MAX).expect_err("a truncated buy is unreadable");
        assert!(
            rejections
                .iter()
                .any(|r| matches!(r, Rejection::UnreadableVenueInstruction(_))),
            "got {rejections:?}"
        );
    }

    #[test]
    fn a_known_non_trade_on_the_venue_contributes_nothing() {
        // Fee collection and accumulator bookkeeping are numerous and spend
        // nothing. Refusing them would make the check fire on a change a
        // reasonable person would make, which is worse than no check at all.
        use radar_decode::pumpfun::Instruction;
        let data = Instruction::CollectCreatorFee
            .discriminator()
            .as_bytes()
            .to_vec();
        assert!(check_venue(&venue_tx(data), u64::MAX).is_ok());
    }

    #[test]
    fn the_callers_own_ceiling_narrows_but_cannot_widen() {
        use radar_decode::pumpfun::Instruction;
        let inside = venue_tx(venue_trade(Instruction::BuyExactSolIn, 40_000_000, 0));
        // Inside the authorisation and inside the caller's own bound: signed.
        assert!(check_venue(&inside, 40_000_000).is_ok());
        // Inside the authorisation but past what the caller said it intended:
        // refused. This is the executor's *output* checked against the
        // executor's *intent*, which is what catches a router that inflated the
        // size after the gate had judged it.
        assert!(check_venue(&inside, 39_999_999).is_err());
        // And a caller asserting no bound at all cannot lift the
        // authorisation's: 60,000,000 is still refused.
        let outside = venue_tx(venue_trade(Instruction::BuyExactSolIn, 60_000_000, 0));
        assert!(check_venue(&outside, u64::MAX).is_err());
    }

    #[test]
    fn a_versioned_message_is_refused_even_with_no_lookup_tables() {
        // ADR 0003 says legacy only. Until now that held only as far as lookup
        // tables: `tx::decode` refuses a versioned message carrying them and
        // accepts one that does not. The ADR's reason is the format, not the
        // tables, so the refusal belongs at the decision to sign.
        let mut bytes = honest();
        // A versioned message *inserts* a version byte before the header rather
        // than setting a bit in it, so this is an insertion and not an OR. The
        // message starts after the one-byte signature count and the signatures
        // it declares, which `build` makes none of.
        let signatures = usize::from(bytes[0]);
        bytes.insert(1 + signatures * 64, 0x80);
        // A versioned message carries a trailing lookup-table count, and the
        // decoder rejects trailing bytes, so append the empty one.
        bytes.push(0);
        let rejections = check(
            &authorization(),
            &bytes,
            &Address::new(WALLET),
            &allowlist(),
            &policy(),
            unbounded(NOW),
        )
        .expect_err("a versioned message must be refused");
        assert!(
            rejections.iter().any(|r| matches!(r, Rejection::Versioned)),
            "got {rejections:?}"
        );
    }
}
