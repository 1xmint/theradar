// SPDX-License-Identifier: Apache-2.0
//! What is held, in what, and how sure the account is of it.
//!
//! # The failure this type exists to make unrepresentable
//!
//! Before this, the whole inventory record was seven fields on a position row:
//! a mint, a creator, two slots, a notional in micro-dollars and two optional
//! prices. It could not say **whose** wallet, **how many** tokens, **in what
//! unit**, how much cash was left, what a pending order had already claimed, or
//! when a dollar figure was last true.
//!
//! Every one of those gaps has the same shape, and it is AGENTS.md rule 9's:
//! the missing fact has a convenient default, and the convenient default reads
//! as *nothing is wrong*. An unread balance defaults to zero and an account with
//! nothing in it looks safe. An unpriced holding defaults to zero dollars and a
//! limit measured against it never binds.
//!
//! So the two states are separate types here, not two readings of one number:
//!
//! - A balance is [`Balance::Counted`] with a [`TokenQuantity`], or
//!   [`Balance::Uncounted`] with the [`Refusal`] that stopped the count. There
//!   is no third state and no zero to fall back to.
//! - A dollar figure is [`Valuation::Known`] **with the slot it was true at**,
//!   or [`Valuation::Unknown`] with an [`Unvaluable`] reason. A price with no
//!   slot is not a valuation; it is a number that used to be one.
//!
//! # Why a valuation can be honestly absent
//!
//! `docs/research/0033` captured ten PumpSwap pools from mainnet: four quote in
//! wrapped SOL, one in USDC, and **five in other SPL mints, one of them another
//! `…pump` token**. So a holding's quote asset may itself have no dollar price
//! without a chain of quotes that does not exist here yet.
//!
//! [`Unvaluable::QuoteUnpriced`] is that fact, written down. The alternative is
//! an estimate, and an estimate in this field is a number a limit will be
//! measured against as though somebody had measured it.
//!
//! # Reservations, and why nothing but an outcome releases one
//!
//! [`Portfolio::reserve`] takes `&mut self`, computes what is free, and inserts
//! in the same borrow. Two requests cannot interleave inside it, so two that
//! together exceed the balance cannot both succeed — the exclusive borrow is
//! the mutual exclusion, at level 1 of AGENTS.md §5's ladder rather than at
//! level 3.
//!
//! [`Portfolio::settle`] is the only way a reservation is released, and it takes
//! **no clock**. There is no timeout, no expiry sweep and no `now` parameter to
//! build one from. A reservation released on a timer frees capital while the
//! transaction it was reserved for may still land, which is how the same
//! dollars get committed twice.
//!
//! A [`Settlement::PartiallyFilled`] leaves the remainder **reserved**. It is
//! not free: the operation is not over, and a second proposal spending it would
//! be spending against a claim that still exists.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Address, Asset, Decimals, MicroUsd, SignedMicroUsd, Slot, TokenQuantity};

/// Whose balances these are.
///
/// Not an `Option<Address>` field, because the two states differ in what they
/// permit rather than only in what they contain: a portfolio with no wallet
/// **cannot hold anything and cannot reserve anything**, which is enforced in
/// [`Portfolio::hold`] and [`Portfolio::reserve`] rather than described here.
///
/// Rule 8, applied to inventory: absent custody configuration denies, it does
/// not fall back to some default address.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Custody {
    /// Every balance in this portfolio was read from this wallet.
    Wallet(Address),
    /// No wallet. Nothing is held, and nothing may be.
    Unattributed,
}

/// Why a balance could not be counted, or an asset could not be accounted for.
///
/// Each of these is a fact about the *instrument*, not about the market. That
/// is the distinction LEARNINGS 10 is about, and it is why none of them is
/// allowed to become a zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// A mint or token-account extension that is not on the accepted list.
    ///
    /// Carries the program's numeric `ExtensionType`, which is the vocabulary
    /// `radar_pumpfun::token` already refuses in — `TransferFeeConfig` (1),
    /// `MintCloseAuthority` (3), `ConfidentialTransferMint` (4),
    /// `PermanentDelegate` (12), `TransferHook` (14) and
    /// `ConfidentialTransferFeeConfig` (16) among them. Each can change what a
    /// balance is worth or whether it can move at all, so a balance under one
    /// is not a balance this account may report.
    ///
    /// The code rather than a name, because the name table lives with the
    /// parser that reads the account and duplicating it here would give the
    /// workspace two lists to keep in step.
    Extension(u16),
    /// The token account is frozen. The units exist and cannot move.
    Frozen,
    /// The mint's `decimals` could not be read, or is beyond
    /// [`Decimals::MAX`]. Without it the raw units mean nothing.
    DecimalsUnread,
    /// Which token program the mint lives under was not established.
    ///
    /// The same address under classic SPL and under Token-2022 is two assets
    /// with different transfer semantics, so guessing picks one at random.
    TokenProgramUnknown,
    /// The record that would say what is held does not record it.
    ///
    /// What a stored position row leaves out: it carries dollars committed, not
    /// units received. Distinct from every other variant here because the fix
    /// is a richer record rather than a different chain read.
    NotRecorded,
}

/// Why a dollar figure is absent.
///
/// `None` would say the same thing and say nothing about which of these it was,
/// and they are not interchangeable: one is fixed by a price feed, one by a
/// chain read, and one cannot be fixed at all until a quote path exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unvaluable {
    /// No dollar price for this asset was available.
    NoPrice,
    /// The asset this holding is quoted against has no dollar price itself.
    ///
    /// `docs/research/0033`: five of ten captured PumpSwap pools quote in
    /// arbitrary SPL mints, one of them another `…pump` token. Pricing through
    /// one needs a chain of quotes that does not exist here.
    QuoteUnpriced,
    /// There is no counted quantity to price.
    BalanceUncounted,
    /// The asset's own semantics are refused, so a price would describe a
    /// transfer that may not happen.
    AssetRefused(Refusal),
    /// A price exists but not one that was ever attributed to a slot.
    NotDated,
}

/// A dollar figure and the slot it was true at, or the reason there is none.
///
/// # Why the slot is inside the value
///
/// A valuation without its slot is indistinguishable from a fresh one, and the
/// gap between "worth $40 at the watermark" and "was worth $40 some time last
/// week" is the whole difference between a limit and a decoration. Keeping the
/// slot in a sibling field would make `Known` with no slot representable, which
/// is the state that reads as fresh.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Valuation {
    /// Worth this much, as of this slot.
    Known {
        /// The dollars.
        value: MicroUsd,
        /// When that was true. Not the slot the holding was opened at, and not
        /// the slot it was closed at: the slot the *price* came from.
        as_of: Slot,
    },
    /// Not worth anything that can be stated, and here is why.
    Unknown(Unvaluable),
}

impl Valuation {
    /// The dollars, or `None`.
    ///
    /// `None` is "cannot value", never `$0`. A caller that needs a number has
    /// to decide what an absent one means for it, which is the point.
    #[must_use]
    pub const fn micro_usd(self) -> Option<MicroUsd> {
        match self {
            Self::Known { value, .. } => Some(value),
            Self::Unknown(_) => None,
        }
    }

    /// The slot the figure was true at, or `None` when there is no figure.
    #[must_use]
    pub const fn as_of(self) -> Option<Slot> {
        match self {
            Self::Known { as_of, .. } => Some(as_of),
            Self::Unknown(_) => None,
        }
    }

    /// Why there is no figure, or `None` when there is one.
    #[must_use]
    pub const fn why_unknown(self) -> Option<Unvaluable> {
        match self {
            Self::Known { .. } => None,
            Self::Unknown(why) => Some(why),
        }
    }
}

/// What a holding is *for*.
///
/// Cash and a position are both balances and they answer different questions:
/// cash is what can be committed, a position is what has been. Summing them
/// would double-count a trade at the moment it settles.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetRole {
    /// Settlement balance — native SOL, wrapped SOL, USDC or whatever else a
    /// venue quotes in. What new risk is sized against.
    Cash,
    /// A token held as exposure.
    Position,
}

/// How many units are held, or why nobody could say.
///
/// The two are separate variants rather than an `Option<TokenQuantity>` so that
/// the absent case has to carry its reason. `None` and `Some(zero)` are one
/// keystroke apart and mean opposite things.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Balance {
    /// Counted. Zero here is a measurement: the account was read and is empty.
    Counted(TokenQuantity),
    /// Not counted, and here is why. Never a zero.
    Uncounted(Refusal),
}

impl Balance {
    /// The counted quantity, or `None`.
    #[must_use]
    pub const fn counted(self) -> Option<TokenQuantity> {
        match self {
            Self::Counted(quantity) => Some(quantity),
            Self::Uncounted(_) => None,
        }
    }

    /// Why it was not counted, or `None` when it was.
    #[must_use]
    pub const fn why_uncounted(self) -> Option<Refusal> {
        match self {
            Self::Counted(_) => None,
            Self::Uncounted(why) => Some(why),
        }
    }
}

/// One asset held in one wallet.
///
/// Built through [`Holding::new`], which downgrades a valuation attached to an
/// uncounted balance. A price times an unknown quantity is not a value, and
/// letting the pair exist would put a plausible dollar figure on a holding
/// nobody could measure.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Holding {
    role: AssetRole,
    balance: Balance,
    value: Valuation,
    cost: Valuation,
}

impl Holding {
    /// A holding of `balance`, worth `value`, acquired for `cost`.
    ///
    /// `cost` is itself a [`Valuation`] and not a plain amount, because the
    /// dollars committed were true at the slot the position opened and at no
    /// other — the same reason `value` carries a slot.
    #[must_use]
    pub const fn new(role: AssetRole, balance: Balance, value: Valuation, cost: Valuation) -> Self {
        let value = match balance {
            // A dollar figure on a quantity nobody counted is a figure about
            // nothing. Refused at the constructor rather than at every reader.
            Balance::Uncounted(_) => Valuation::Unknown(Unvaluable::BalanceUncounted),
            Balance::Counted(_) => value,
        };
        Self {
            role,
            balance,
            value,
            cost,
        }
    }

    /// What this holding is for.
    #[must_use]
    pub const fn role(self) -> AssetRole {
        self.role
    }

    /// How many units, or why nobody could say.
    #[must_use]
    pub const fn balance(self) -> Balance {
        self.balance
    }

    /// What it is worth, dated.
    #[must_use]
    pub const fn value(self) -> Valuation {
        self.value
    }

    /// What it cost, dated.
    #[must_use]
    pub const fn cost(self) -> Valuation {
        self.cost
    }

    /// Value minus cost, or the reason it cannot be computed.
    ///
    /// Dated at the **older** of the two slots, because a difference is only as
    /// current as its stalest half. Under-claiming freshness is recoverable;
    /// over-claiming it is how a stale number gets acted on.
    #[must_use]
    pub fn unrealised(self) -> Unrealised {
        let (
            Valuation::Known { value, as_of },
            Valuation::Known {
                value: cost,
                as_of: at,
            },
        ) = (self.value, self.cost)
        else {
            let why = self
                .value
                .why_unknown()
                .or_else(|| self.cost.why_unknown())
                .unwrap_or(Unvaluable::NoPrice);
            return Unrealised::Unknown(why);
        };
        let amount = i64::try_from(value.get())
            .ok()
            .zip(i64::try_from(cost.get()).ok())
            .map(|(v, c)| SignedMicroUsd(v.saturating_sub(c)));
        match amount {
            Some(amount) => Unrealised::Known {
                amount,
                as_of: as_of.min(at),
            },
            // A figure too large for a signed 64-bit difference is not a
            // profit-and-loss number; saying so beats saturating into one.
            None => Unrealised::Unknown(Unvaluable::NoPrice),
        }
    }
}

/// A paper result: value not yet turned into cash.
///
/// Signed, dated, and able to be absent — the three properties a realised
/// figure needs and that a bare `i64` has none of.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unrealised {
    /// This much, as of this slot.
    Known {
        /// The signed difference between value and cost.
        amount: SignedMicroUsd,
        /// The stalest slot contributing to it.
        as_of: Slot,
    },
    /// Not computable, and here is why.
    Unknown(Unvaluable),
}

impl Unrealised {
    /// The amount, or `None`. Never zero as a stand-in.
    #[must_use]
    pub const fn amount(self) -> Option<SignedMicroUsd> {
        match self {
            Self::Known { amount, .. } => Some(amount),
            Self::Unknown(_) => None,
        }
    }
}

/// Which kind of cost was paid.
///
/// Three fields rather than one total, because they are not the same money.
/// Rent is recoverable when an account is closed, a priority fee is not, and a
/// tip is discretionary — collapsing them makes the recoverable part invisible
/// and every fee-efficiency question unanswerable.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostKind {
    /// Base and priority fees paid to validators.
    NetworkFee,
    /// Lamports locked in account rent — for a token account that has to exist
    /// before it can receive anything.
    Rent,
    /// A tip to a block-building service.
    Tip,
}

/// Lamports paid out, by kind.
///
/// Always SOL: every one of these is denominated in lamports on Solana, and
/// [`Portfolio::charge`] refuses any other unit rather than storing a USDC
/// figure in a field the reader will treat as lamports.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Costs {
    fees: TokenQuantity,
    rent: TokenQuantity,
    tips: TokenQuantity,
}

impl Costs {
    /// Nothing paid yet. A counted zero, in lamports.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            fees: TokenQuantity::lamports(0),
            rent: TokenQuantity::lamports(0),
            tips: TokenQuantity::lamports(0),
        }
    }

    /// Fees paid to validators.
    #[must_use]
    pub const fn network_fees(self) -> TokenQuantity {
        self.fees
    }

    /// Lamports locked as rent.
    #[must_use]
    pub const fn rent(self) -> TokenQuantity {
        self.rent
    }

    /// Tips paid to block builders.
    #[must_use]
    pub const fn tips(self) -> TokenQuantity {
        self.tips
    }

    /// All three together, or `None` on overflow.
    ///
    /// Not folded into the dollar results: these are lamports and a realised
    /// result is dollars, and converting needs a SOL price that this type does
    /// not have. Adding them anyway would need an assumed rate, which is the
    /// error `Asset` exists to refuse.
    #[must_use]
    pub fn total(self) -> Option<TokenQuantity> {
        self.fees.checked_add(self.rent)?.checked_add(self.tips)
    }
}

/// Identifies one reservation within one portfolio.
///
/// Opaque, and issued only by [`Portfolio::reserve`]. A caller cannot mint one
/// and settle a claim it never made.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct ReservationId(u64);

/// Capital claimed by an operation that has not finished.
///
/// It is not spent, and it is not free. Treating it as either is how the same
/// dollars get committed twice — as free, by a second proposal sized against a
/// balance the first has a claim on; as spent, by an account that reports a
/// loss for a transaction that never landed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Reservation {
    id: ReservationId,
    asset: Asset,
    outstanding: TokenQuantity,
    filled: TokenQuantity,
    opened_at: Slot,
}

impl Reservation {
    /// Which reservation this is.
    #[must_use]
    pub const fn id(self) -> ReservationId {
        self.id
    }

    /// What it is a claim on.
    #[must_use]
    pub const fn asset(self) -> Asset {
        self.asset
    }

    /// Units still claimed and not yet spent.
    #[must_use]
    pub const fn outstanding(self) -> TokenQuantity {
        self.outstanding
    }

    /// Units already spent against this reservation.
    #[must_use]
    pub const fn filled(self) -> TokenQuantity {
        self.filled
    }

    /// The slot the claim was made at.
    ///
    /// Recorded, and deliberately never read by [`Portfolio::settle`]: a
    /// reservation is released by an outcome, and an age is not an outcome.
    #[must_use]
    pub const fn opened_at(self) -> Slot {
        self.opened_at
    }
}

/// How an operation ended.
///
/// There is no `Expired`. See the module documentation: a claim released on a
/// timer frees capital while the transaction may still land.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Settlement {
    /// Everything outstanding was spent. The claim closes.
    Filled,
    /// This much was spent and the rest is **still claimed**.
    ///
    /// The reservation stays open with the remainder outstanding, so the
    /// unfilled part is not free for a second proposal. It closes only if the
    /// amount given equals what was outstanding, which is a fill by another
    /// name.
    PartiallyFilled(TokenQuantity),
    /// Nothing further was spent. The claim closes and its outstanding units
    /// become free again.
    Abandoned,
}

/// Why an inventory operation could not be carried out.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Error)]
pub enum PortfolioError {
    /// No wallet is attributed, so nothing can be held or claimed.
    #[error("no wallet is attributed to this portfolio; nothing can be held or reserved")]
    NoWallet,
    /// There is no holding in this asset at all.
    #[error("nothing is held in {0:?}")]
    NotHeld(Asset),
    /// There is a holding and its balance was never counted.
    #[error("the balance of {asset:?} was not counted: {why:?}")]
    Uncounted {
        /// The asset.
        asset: Asset,
        /// Why the count failed.
        why: Refusal,
    },
    /// Two quantities in the same operation were in different units.
    #[error("{asset:?} was given in a unit the holding is not measured in")]
    UnitMismatch {
        /// The asset.
        asset: Asset,
    },
    /// Less is free than was asked for.
    #[error("{requested} requested against {free} free")]
    Insufficient {
        /// What is free right now.
        free: TokenQuantity,
        /// What was asked for.
        requested: TokenQuantity,
    },
    /// Claims against this asset exceed its balance.
    #[error("claims against {0:?} exceed the balance held")]
    Overdrawn(Asset),
    /// No such claim.
    #[error("no reservation {0:?}")]
    NoSuchReservation(ReservationId),
    /// A fill larger than what was outstanding.
    #[error("a fill of {filled} against {outstanding} outstanding")]
    OverFilled {
        /// What was still claimed.
        outstanding: TokenQuantity,
        /// What the fill claimed to have spent.
        filled: TokenQuantity,
    },
}

/// Recorded exposure the account cannot fully describe.
///
/// The point of naming it is that a caller can refuse on it. An incomplete
/// portfolio is not a portfolio with a small error in it; it is one whose totals
/// are lower bounds, and sizing new risk against a lower bound is sizing against
/// a number chosen for being convenient.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Incomplete {
    /// Exposure to a mint that could not be placed as a holding at all.
    Unaccounted {
        /// The mint.
        mint: Address,
        /// Why it could not be placed.
        why: Refusal,
    },
    /// A holding whose balance was never counted.
    Uncounted {
        /// The asset.
        asset: Asset,
        /// Why the count failed.
        why: Refusal,
    },
}

/// What the account made and what it is still holding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Results {
    /// Round trips that closed, netted, signed.
    pub realised: SignedMicroUsd,
    /// Open holdings, valued — or the reason they are not.
    pub unrealised: Unrealised,
    /// Lamports paid out. Kept separate from the dollar figures because
    /// converting them needs a SOL price this type does not hold.
    pub costs: Costs,
}

/// What is held, in what, and how sure the account is of it.
///
/// The caller is `radar consider`, which reads it before the risk kernel judges
/// anything and refuses the pass when it cannot be read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Portfolio {
    custody: Custody,
    as_of: Slot,
    holdings: BTreeMap<Asset, Holding>,
    unaccounted: BTreeMap<Address, Refusal>,
    reservations: BTreeMap<ReservationId, Reservation>,
    costs: Costs,
    realised: SignedMicroUsd,
    next_id: u64,
}

impl Portfolio {
    /// An empty portfolio for `wallet`, read as of `as_of`.
    #[must_use]
    pub fn at(wallet: Address, as_of: Slot) -> Self {
        Self::new(Custody::Wallet(wallet), as_of)
    }

    /// A portfolio with no wallet attributed to it.
    ///
    /// It holds nothing and can be made to hold nothing, which is the honest
    /// shape for an instance with no custody configured — rule 8. Not the same
    /// as a wallet that was read and found empty: that one is
    /// [`Portfolio::at`] with counted, zero balances.
    #[must_use]
    pub fn unattributed(as_of: Slot) -> Self {
        Self::new(Custody::Unattributed, as_of)
    }

    fn new(custody: Custody, as_of: Slot) -> Self {
        Self {
            custody,
            as_of,
            holdings: BTreeMap::new(),
            unaccounted: BTreeMap::new(),
            reservations: BTreeMap::new(),
            costs: Costs::none(),
            realised: SignedMicroUsd::ZERO,
            next_id: 0,
        }
    }

    /// Whose balances these are.
    #[must_use]
    pub const fn custody(&self) -> Custody {
        self.custody
    }

    /// The wallet, or `None` when none is attributed.
    #[must_use]
    pub const fn wallet(&self) -> Option<Address> {
        match self.custody {
            Custody::Wallet(address) => Some(address),
            Custody::Unattributed => None,
        }
    }

    /// The watermark this account was assembled at.
    #[must_use]
    pub const fn as_of(&self) -> Slot {
        self.as_of
    }

    /// Records a holding, replacing any previous one in the same asset.
    ///
    /// # Errors
    ///
    /// [`PortfolioError::NoWallet`] when no wallet is attributed. Balances have
    /// to come from somewhere, and an unattributed portfolio has nowhere for
    /// them to have come from.
    pub fn hold(&mut self, asset: Asset, holding: Holding) -> Result<(), PortfolioError> {
        if self.wallet().is_none() {
            return Err(PortfolioError::NoWallet);
        }
        self.holdings.insert(asset, holding);
        Ok(())
    }

    /// Records exposure to a mint that could not be placed as a holding.
    ///
    /// Written down rather than dropped. A dropped one is indistinguishable
    /// from no exposure, which is the reading that authorises more.
    pub fn note_unaccounted(&mut self, mint: Address, why: Refusal) {
        self.unaccounted.insert(mint, why);
    }

    /// The holding in this asset, if any.
    #[must_use]
    pub fn holding(&self, asset: Asset) -> Option<Holding> {
        self.holdings.get(&asset).copied()
    }

    /// Every holding, in asset order.
    pub fn holdings(&self) -> impl Iterator<Item = (Asset, Holding)> + '_ {
        self.holdings.iter().map(|(asset, held)| (*asset, *held))
    }

    /// Mints with recorded exposure that could not be placed, and why.
    pub fn unaccounted(&self) -> impl Iterator<Item = (Address, Refusal)> + '_ {
        self.unaccounted.iter().map(|(mint, why)| (*mint, *why))
    }

    /// Every open claim, in the order they were made.
    pub fn reservations(&self) -> impl Iterator<Item = Reservation> + '_ {
        self.reservations.values().copied()
    }

    /// Units claimed and not yet spent, in this asset.
    ///
    /// # Errors
    ///
    /// [`PortfolioError::UnitMismatch`] when two claims against one asset were
    /// recorded in different units, which cannot happen through
    /// [`Portfolio::reserve`] but is checked rather than assumed.
    pub fn reserved(
        &self,
        asset: Asset,
        decimals: Decimals,
    ) -> Result<TokenQuantity, PortfolioError> {
        let mut total = TokenQuantity::zero(decimals);
        for claim in self.reservations.values().filter(|r| r.asset == asset) {
            total = total
                .checked_add(claim.outstanding)
                .ok_or(PortfolioError::UnitMismatch { asset })?;
        }
        Ok(total)
    }

    /// Counted balance minus outstanding claims.
    ///
    /// # Errors
    ///
    /// Returns [`PortfolioError::NotHeld`] when the asset is absent and
    /// [`PortfolioError::Uncounted`] when its balance was never read. Neither
    /// is reported as zero free: an unread balance that read as zero would
    /// refuse a trade that should happen, and an unread balance that read as
    /// *anything* would authorise one that should not.
    pub fn free(&self, asset: Asset) -> Result<TokenQuantity, PortfolioError> {
        let holding = self
            .holdings
            .get(&asset)
            .ok_or(PortfolioError::NotHeld(asset))?;
        let counted = match holding.balance {
            Balance::Counted(quantity) => quantity,
            Balance::Uncounted(why) => return Err(PortfolioError::Uncounted { asset, why }),
        };
        let claimed = self.reserved(asset, counted.decimals())?;
        counted
            .checked_sub(claimed)
            .ok_or(PortfolioError::Overdrawn(asset))
    }

    /// Claims `amount` of `asset` for an operation about to start.
    ///
    /// # Atomic against the free balance
    ///
    /// The check and the insert happen inside one `&mut self` borrow, so no
    /// second reservation can be taken between them. Two requests that together
    /// exceed the balance cannot both succeed, whichever order they arrive in
    /// and whichever threads they arrive from — a shared portfolio needs a lock
    /// to be *reached*, and the lock is what serialises the callers; this is
    /// what makes serialising them sufficient.
    ///
    /// # Errors
    ///
    /// [`PortfolioError::NoWallet`], anything [`Portfolio::free`] returns, and
    /// [`PortfolioError::Insufficient`] when less is free than was asked for.
    pub fn reserve(
        &mut self,
        asset: Asset,
        amount: TokenQuantity,
        at: Slot,
    ) -> Result<ReservationId, PortfolioError> {
        if self.wallet().is_none() {
            return Err(PortfolioError::NoWallet);
        }
        let free = self.free(asset)?;
        match free.checked_cmp(amount) {
            None => return Err(PortfolioError::UnitMismatch { asset }),
            Some(core::cmp::Ordering::Less) => {
                return Err(PortfolioError::Insufficient {
                    free,
                    requested: amount,
                });
            }
            Some(_) => {}
        }
        let id = ReservationId(self.next_id);
        self.next_id += 1;
        self.reservations.insert(
            id,
            Reservation {
                id,
                asset,
                outstanding: amount,
                filled: TokenQuantity::zero(amount.decimals()),
                opened_at: at,
            },
        );
        Ok(id)
    }

    /// Closes out a claim against the outcome of the operation that made it.
    ///
    /// Takes no clock, on purpose. See the module documentation.
    ///
    /// # Errors
    ///
    /// [`PortfolioError::NoSuchReservation`] for an unknown id,
    /// [`PortfolioError::OverFilled`] for a fill larger than what was
    /// outstanding, [`PortfolioError::UnitMismatch`] for a fill in the wrong
    /// unit, and [`PortfolioError::Uncounted`] when the balance a fill would be
    /// debited from was never counted.
    pub fn settle(&mut self, id: ReservationId, outcome: Settlement) -> Result<(), PortfolioError> {
        let claim = *self
            .reservations
            .get(&id)
            .ok_or(PortfolioError::NoSuchReservation(id))?;

        let spent = match outcome {
            Settlement::Filled => claim.outstanding,
            Settlement::PartiallyFilled(filled) => {
                match claim.outstanding.checked_cmp(filled) {
                    None => return Err(PortfolioError::UnitMismatch { asset: claim.asset }),
                    Some(core::cmp::Ordering::Less) => {
                        return Err(PortfolioError::OverFilled {
                            outstanding: claim.outstanding,
                            filled,
                        });
                    }
                    Some(_) => {}
                }
                filled
            }
            // Nothing more was spent, so nothing is debited and the whole
            // outstanding claim goes back to being free.
            Settlement::Abandoned => {
                self.reservations.remove(&id);
                return Ok(());
            }
        };

        self.debit(claim.asset, spent)?;

        let remaining = claim
            .outstanding
            .checked_sub(spent)
            .ok_or(PortfolioError::OverFilled {
                outstanding: claim.outstanding,
                filled: spent,
            })?;
        if remaining.is_zero() {
            self.reservations.remove(&id);
            return Ok(());
        }
        // The remainder stays claimed. Freeing it here would let a second
        // proposal spend capital the first operation still has a claim on.
        let entry = self
            .reservations
            .get_mut(&id)
            .ok_or(PortfolioError::NoSuchReservation(id))?;
        entry.outstanding = remaining;
        entry.filled = claim
            .filled
            .checked_add(spent)
            .ok_or(PortfolioError::UnitMismatch { asset: claim.asset })?;
        Ok(())
    }

    /// Takes `spent` units out of the counted balance of `asset`.
    fn debit(&mut self, asset: Asset, spent: TokenQuantity) -> Result<(), PortfolioError> {
        let holding = self
            .holdings
            .get_mut(&asset)
            .ok_or(PortfolioError::NotHeld(asset))?;
        let counted = match holding.balance {
            Balance::Counted(quantity) => quantity,
            Balance::Uncounted(why) => return Err(PortfolioError::Uncounted { asset, why }),
        };
        let left = counted
            .checked_sub(spent)
            .ok_or(PortfolioError::Overdrawn(asset))?;
        holding.balance = Balance::Counted(left);
        Ok(())
    }

    /// Records lamports paid out.
    ///
    /// # Errors
    ///
    /// [`PortfolioError::UnitMismatch`] for anything not denominated in
    /// lamports, and again on overflow of the running total.
    pub fn charge(
        &mut self,
        kind: CostKind,
        lamports: TokenQuantity,
    ) -> Result<(), PortfolioError> {
        if lamports.decimals() != Decimals::NATIVE_SOL {
            return Err(PortfolioError::UnitMismatch { asset: Asset::Sol });
        }
        let field = match kind {
            CostKind::NetworkFee => &mut self.costs.fees,
            CostKind::Rent => &mut self.costs.rent,
            CostKind::Tip => &mut self.costs.tips,
        };
        *field = field
            .checked_add(lamports)
            .ok_or(PortfolioError::UnitMismatch { asset: Asset::Sol })?;
        Ok(())
    }

    /// Adds a closed round trip's result to the realised total.
    pub fn record_realised(&mut self, delta: SignedMicroUsd) {
        self.realised = self.realised.saturating_add(delta);
    }

    /// The first thing this account cannot fully describe, or `None`.
    ///
    /// `None` on an empty portfolio, and that is correct: nothing recorded and
    /// nothing missing is a complete description of holding nothing. The state
    /// this rejects is *recorded exposure that could not be measured*, which is
    /// the one that makes every total a lower bound.
    #[must_use]
    pub fn incompleteness(&self) -> Option<Incomplete> {
        if let Some((mint, why)) = self.unaccounted.iter().next() {
            return Some(Incomplete::Unaccounted {
                mint: *mint,
                why: *why,
            });
        }
        self.holdings.iter().find_map(|(asset, held)| {
            held.balance
                .why_uncounted()
                .map(|why| Incomplete::Uncounted { asset: *asset, why })
        })
    }

    /// Cash across every settlement asset, valued.
    ///
    /// A map rather than a total: SOL and USDC are not one number, and 0017 §3
    /// is explicit that they *"must not share an untyped integer or fixed $1
    /// assumption"*. Anything that wants a single figure has to bring a price
    /// and say what slot it was true at.
    pub fn cash(&self) -> impl Iterator<Item = (Asset, Holding)> + '_ {
        self.holdings
            .iter()
            .filter(|(_, held)| held.role == AssetRole::Cash)
            .map(|(asset, held)| (*asset, *held))
    }

    /// What the account made, and what it is still holding.
    ///
    /// The unrealised half is `Unknown` as soon as **any** open position is,
    /// rather than summing the ones that can be valued. A partial sum reads as
    /// a total, and a total that quietly omits the holdings nobody could price
    /// is the smaller number — which is the one that gets permission.
    #[must_use]
    pub fn results(&self) -> Results {
        let mut total = SignedMicroUsd::ZERO;
        let mut oldest: Option<Slot> = None;
        for held in self
            .holdings
            .values()
            .filter(|h| h.role == AssetRole::Position)
        {
            match held.unrealised() {
                Unrealised::Known { amount, as_of } => {
                    total = total.saturating_add(amount);
                    oldest = Some(oldest.map_or(as_of, |o: Slot| o.min(as_of)));
                }
                unknown @ Unrealised::Unknown(_) => {
                    return Results {
                        realised: self.realised,
                        unrealised: unknown,
                        costs: self.costs,
                    };
                }
            }
        }
        Results {
            realised: self.realised,
            unrealised: match oldest {
                Some(as_of) => Unrealised::Known {
                    amount: total,
                    as_of,
                },
                // No open positions at all. That is a *measured* zero -- there
                // is nothing to value -- and it is dated at the watermark the
                // account was assembled at, which is the slot at which the
                // statement "nothing is open" was checked.
                None => Unrealised::Known {
                    amount: SignedMicroUsd::ZERO,
                    as_of: self.as_of,
                },
            },
            costs: self.costs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AssetRole, Balance, CostKind, Costs, Custody, Holding, Incomplete, Portfolio,
        PortfolioError, Refusal, Settlement, Unrealised, Unvaluable, Valuation,
    };
    use crate::{Address, Asset, Decimals, MicroUsd, SignedMicroUsd, Slot, TokenQuantity};
    use std::sync::{Arc, Mutex};

    const NOW: Slot = Slot(10_000);

    fn wallet() -> Address {
        Address::new([7u8; 32])
    }

    fn usdc(units: u64) -> TokenQuantity {
        TokenQuantity::new(units, Decimals::from_mint_account(6).expect("six places"))
    }

    fn priced(value: u64, as_of: Slot) -> Valuation {
        Valuation::Known {
            value: MicroUsd(value),
            as_of,
        }
    }

    /// A portfolio holding `units` of USDC as cash, counted and valued.
    fn with_cash(units: u64) -> Portfolio {
        let mut portfolio = Portfolio::at(wallet(), NOW);
        portfolio
            .hold(
                Asset::Usdc,
                Holding::new(
                    AssetRole::Cash,
                    Balance::Counted(usdc(units)),
                    priced(units, NOW),
                    priced(units, NOW),
                ),
            )
            .expect("a wallet is attributed");
        portfolio
    }

    #[test]
    fn two_requests_that_together_exceed_the_balance_cannot_both_succeed() {
        // The named test. Two threads race for the same $100, each asking for
        // $60. The exclusive borrow inside `reserve` is what makes this
        // impossible rather than unlikely: the free balance is computed and the
        // claim inserted without releasing it.
        //
        // Re-apply the bug by splitting `reserve` into a read and a write --
        // take the free balance, drop the lock, then insert -- and both threads
        // succeed against $100 with $120 claimed.
        let shared = Arc::new(Mutex::new(with_cash(100_000_000)));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let portfolio = Arc::clone(&shared);
            handles.push(std::thread::spawn(move || {
                portfolio
                    .lock()
                    .expect("no thread panicked while holding it")
                    .reserve(Asset::Usdc, usdc(60_000_000), NOW)
            }));
        }
        let outcomes: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("the thread finished"))
            .collect();

        assert_eq!(
            outcomes.iter().filter(|r| r.is_ok()).count(),
            1,
            "exactly one claim on $100 can be for $60: {outcomes:?}"
        );
        let refused = outcomes
            .iter()
            .find_map(|r| r.as_ref().err())
            .expect("the other was refused");
        assert!(
            matches!(refused, PortfolioError::Insufficient { .. }),
            "refused for the right reason: {refused:?}"
        );

        let held = shared.lock().expect("not poisoned");
        assert_eq!(
            held.free(Asset::Usdc).expect("still readable"),
            usdc(40_000_000),
            "the balance is $100 with $60 claimed, not $100 with $120"
        );
    }

    #[test]
    fn a_partial_fill_leaves_the_remainder_reserved_rather_than_free() {
        // The named test. $100 held, $60 claimed, $25 of it spent. The
        // remaining $35 is still claimed by an operation that has not finished,
        // so what is free is $40 -- not $75.
        //
        // Re-apply the bug by removing the reservation on a partial fill
        // instead of reducing it, and `free` returns $75: the same $35 becomes
        // available to a second proposal while the first still has a claim.
        let mut portfolio = with_cash(100_000_000);
        let claim = portfolio
            .reserve(Asset::Usdc, usdc(60_000_000), NOW)
            .expect("enough is free");

        portfolio
            .settle(claim, Settlement::PartiallyFilled(usdc(25_000_000)))
            .expect("a fill inside the claim");

        assert_eq!(
            portfolio.free(Asset::Usdc).expect("readable"),
            usdc(40_000_000),
            "$75 remains, $35 of it still claimed"
        );
        let remaining = portfolio
            .reservations()
            .next()
            .expect("the claim is still open");
        assert_eq!(remaining.outstanding(), usdc(35_000_000));
        assert_eq!(remaining.filled(), usdc(25_000_000));

        // And a second proposal for the unfilled remainder is refused.
        assert!(matches!(
            portfolio.reserve(Asset::Usdc, usdc(41_000_000), NOW),
            Err(PortfolioError::Insufficient { .. })
        ));
    }

    #[test]
    fn an_unknown_valuation_does_not_read_as_zero() {
        // The named test. A holding whose quote asset has no dollar price --
        // five of ten captured PumpSwap pools quote in arbitrary SPL mints --
        // reports no value, and `results` reports the whole account as unvalued
        // rather than summing the priced half into a smaller total.
        //
        // Re-apply the bug by defaulting `micro_usd` to `MicroUsd::ZERO`, or by
        // skipping unknown holdings in `results`, and both assertions below
        // turn into a dollar figure nobody measured.
        let unpriced = Valuation::Unknown(Unvaluable::QuoteUnpriced);
        assert_eq!(unpriced.micro_usd(), None, "cannot value, not $0");
        assert_eq!(unpriced.as_of(), None, "and it is not dated either");
        assert_ne!(unpriced, priced(0, NOW), "nor equal to a measured zero");

        let mut portfolio = with_cash(100_000_000);
        portfolio
            .hold(
                Asset::token_2022(Address::new([3u8; 32])),
                Holding::new(
                    AssetRole::Position,
                    Balance::Counted(TokenQuantity::new(
                        5_000_000,
                        Decimals::from_mint_account(6).expect("six"),
                    )),
                    unpriced,
                    priced(40_000_000, NOW),
                ),
            )
            .expect("a wallet is attributed");

        let results = portfolio.results();
        assert_eq!(
            results.unrealised,
            Unrealised::Unknown(Unvaluable::QuoteUnpriced),
            "an unvaluable holding makes the total unvaluable, not smaller"
        );
        assert_eq!(results.unrealised.amount(), None);
    }

    #[test]
    fn an_uncounted_balance_is_not_a_free_zero() {
        // The same rule one level down. A token account under a refused
        // extension holds units nobody may report, and `free` says so rather
        // than answering zero -- which would read as "held, and empty".
        let mut portfolio = Portfolio::at(wallet(), NOW);
        let mint = Asset::token_2022(Address::new([4u8; 32]));
        portfolio
            .hold(
                mint,
                Holding::new(
                    AssetRole::Position,
                    // TransferHook: a transfer can be refused outright.
                    Balance::Uncounted(Refusal::Extension(14)),
                    priced(40_000_000, NOW),
                    priced(40_000_000, NOW),
                ),
            )
            .expect("a wallet is attributed");

        assert!(matches!(
            portfolio.free(mint),
            Err(PortfolioError::Uncounted {
                why: Refusal::Extension(14),
                ..
            })
        ));
        // And the valuation handed in was dropped: a price on a quantity nobody
        // counted is a figure about nothing.
        let held = portfolio.holding(mint).expect("held");
        assert_eq!(
            held.value(),
            Valuation::Unknown(Unvaluable::BalanceUncounted),
            "the constructor refuses to keep it"
        );
        assert_eq!(
            portfolio.incompleteness(),
            Some(Incomplete::Uncounted {
                asset: mint,
                why: Refusal::Extension(14)
            })
        );
    }

    #[test]
    fn nothing_held_and_nothing_missing_is_a_complete_account() {
        // The distinction `coverage::ObservedSlots` draws, at the balance
        // level: an account that was read and holds nothing is complete, and
        // one with exposure it could not measure is not. Collapsing the two
        // would make every empty instance refuse, which is a check that fires
        // on the normal case.
        assert_eq!(Portfolio::at(wallet(), NOW).incompleteness(), None);
        assert_eq!(Portfolio::unattributed(NOW).incompleteness(), None);

        let mut with_gap = Portfolio::at(wallet(), NOW);
        let mint = Address::new([5u8; 32]);
        with_gap.note_unaccounted(mint, Refusal::NotRecorded);
        assert_eq!(
            with_gap.incompleteness(),
            Some(Incomplete::Unaccounted {
                mint,
                why: Refusal::NotRecorded
            })
        );
    }

    #[test]
    fn a_portfolio_with_no_wallet_can_hold_nothing_and_claim_nothing() {
        // Rule 8 at the inventory level. An instance with no custody configured
        // must not be able to acquire a balance out of nowhere and then size
        // against it.
        let mut nowhere = Portfolio::unattributed(NOW);
        assert_eq!(nowhere.custody(), Custody::Unattributed);
        assert_eq!(nowhere.wallet(), None);
        assert_eq!(
            nowhere.hold(
                Asset::Usdc,
                Holding::new(
                    AssetRole::Cash,
                    Balance::Counted(usdc(1)),
                    Valuation::Unknown(Unvaluable::NoPrice),
                    Valuation::Unknown(Unvaluable::NoPrice),
                )
            ),
            Err(PortfolioError::NoWallet)
        );
        assert_eq!(
            nowhere.reserve(Asset::Usdc, usdc(1), NOW),
            Err(PortfolioError::NoWallet)
        );
    }

    #[test]
    fn an_abandoned_operation_frees_its_claim_and_a_fill_spends_it() {
        // The two ways a claim ends. Neither is a clock: `settle` takes no
        // `now` and there is no expiry sweep, because a claim released on a
        // timer frees capital while the transaction may still land.
        let mut portfolio = with_cash(100_000_000);
        let abandoned = portfolio
            .reserve(Asset::Usdc, usdc(60_000_000), NOW)
            .expect("free");
        portfolio
            .settle(abandoned, Settlement::Abandoned)
            .expect("a known claim");
        assert_eq!(
            portfolio.free(Asset::Usdc).expect("readable"),
            usdc(100_000_000),
            "nothing was spent, so the whole balance is free again"
        );
        assert_eq!(portfolio.reservations().count(), 0);

        let filled = portfolio
            .reserve(Asset::Usdc, usdc(60_000_000), NOW)
            .expect("free");
        portfolio.settle(filled, Settlement::Filled).expect("known");
        assert_eq!(
            portfolio.free(Asset::Usdc).expect("readable"),
            usdc(40_000_000),
            "the claim was spent, so it leaves the balance"
        );
        assert_eq!(portfolio.reservations().count(), 0);
        assert!(matches!(
            portfolio.settle(filled, Settlement::Abandoned),
            Err(PortfolioError::NoSuchReservation(_)),
        ));
    }

    #[test]
    fn a_fill_larger_than_the_claim_is_refused() {
        // Otherwise the debit runs against a balance the claim never covered,
        // and the account reports capital it never had.
        let mut portfolio = with_cash(100_000_000);
        let claim = portfolio
            .reserve(Asset::Usdc, usdc(10_000_000), NOW)
            .expect("free");
        assert!(matches!(
            portfolio.settle(claim, Settlement::PartiallyFilled(usdc(11_000_000))),
            Err(PortfolioError::OverFilled { .. })
        ));
        assert_eq!(
            portfolio.free(Asset::Usdc).expect("readable"),
            usdc(90_000_000),
            "and nothing moved"
        );
    }

    #[test]
    fn a_claim_in_the_wrong_unit_is_refused_rather_than_compared_on_raw_units() {
        // 1_000_000 lamports against 1_000_000 base units of USDC would compare
        // equal on the integers and differ by a factor of a thousand in value.
        let mut portfolio = with_cash(100_000_000);
        assert_eq!(
            portfolio.reserve(Asset::Usdc, TokenQuantity::lamports(1), NOW),
            Err(PortfolioError::UnitMismatch { asset: Asset::Usdc })
        );
    }

    #[test]
    fn fees_rent_and_tips_stay_apart_and_only_take_lamports() {
        // Rent is recoverable when an account closes and a priority fee is not,
        // so one total cannot answer either question. And all three are SOL:
        // recording a USDC figure in a field the reader treats as lamports is
        // wrong by a factor of a thousand.
        let mut portfolio = with_cash(100_000_000);
        portfolio
            .charge(CostKind::NetworkFee, TokenQuantity::lamports(5_000))
            .expect("lamports");
        portfolio
            .charge(CostKind::Rent, TokenQuantity::lamports(2_039_280))
            .expect("lamports");
        portfolio
            .charge(CostKind::Tip, TokenQuantity::lamports(100_000))
            .expect("lamports");
        assert_eq!(
            portfolio.charge(CostKind::NetworkFee, usdc(1)),
            Err(PortfolioError::UnitMismatch { asset: Asset::Sol })
        );

        let costs = portfolio.results().costs;
        assert_eq!(costs.network_fees(), TokenQuantity::lamports(5_000));
        assert_eq!(costs.rent(), TokenQuantity::lamports(2_039_280));
        assert_eq!(costs.tips(), TokenQuantity::lamports(100_000));
        assert_eq!(costs.total(), Some(TokenQuantity::lamports(2_144_280)));
        assert_eq!(Costs::none().total(), Some(TokenQuantity::lamports(0)));
    }

    #[test]
    fn realised_and_unrealised_are_separate_answers() {
        // A closed round trip and an open one are different facts. Netting them
        // into one figure hides which half is still exposed to the market.
        let mut portfolio = with_cash(100_000_000);
        portfolio.record_realised(SignedMicroUsd(-650_000));

        let six = Decimals::from_mint_account(6).expect("six");
        portfolio
            .hold(
                Asset::token_2022(Address::new([9u8; 32])),
                Holding::new(
                    AssetRole::Position,
                    Balance::Counted(TokenQuantity::new(5_000_000, six)),
                    priced(48_000_000, Slot(9_500)),
                    priced(40_000_000, Slot(9_000)),
                ),
            )
            .expect("a wallet is attributed");

        let results = portfolio.results();
        assert_eq!(results.realised, SignedMicroUsd(-650_000));
        assert_eq!(
            results.unrealised,
            Unrealised::Known {
                amount: SignedMicroUsd(8_000_000),
                // Dated at the STALER of the two slots. A difference is only as
                // current as its oldest half, and claiming 9_500 here would say
                // the cost had been re-read then.
                as_of: Slot(9_000),
            }
        );
        // Cash is not netted into the position result: SOL and USDC are not one
        // number, and cash is not exposure.
        assert_eq!(portfolio.cash().count(), 1);
    }

    #[test]
    fn an_account_holding_no_positions_has_a_measured_zero_unrealised() {
        // Not `Unknown`. Nothing open was checked at the watermark and found to
        // be nothing, which is a measurement -- the same distinction
        // `Completion::Complete` draws for a query that returned no rows.
        let results = Portfolio::at(wallet(), NOW).results();
        assert_eq!(
            results.unrealised,
            Unrealised::Known {
                amount: SignedMicroUsd::ZERO,
                as_of: NOW
            }
        );
    }

    #[test]
    fn a_holding_priced_on_one_side_only_is_not_a_result() {
        // Cost known, value unknown. Treating the missing side as zero would
        // report the position as a total loss; treating it as the cost would
        // report it as flat. Both are figures nobody measured.
        let six = Decimals::from_mint_account(6).expect("six");
        let held = Holding::new(
            AssetRole::Position,
            Balance::Counted(TokenQuantity::new(1, six)),
            Valuation::Unknown(Unvaluable::NoPrice),
            priced(40_000_000, NOW),
        );
        assert_eq!(
            held.unrealised(),
            Unrealised::Unknown(Unvaluable::NoPrice),
            "one known side is not a result"
        );
    }
}
