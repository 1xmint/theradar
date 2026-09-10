// SPDX-License-Identifier: Apache-2.0
//! What a PumpSwap pool holds, read so that the numbers describe one instant.
//!
//! # Why one slot is the whole design
//!
//! A pool's reserves are two balances that are only meaningful together. Read
//! the base vault at slot N and the quote vault at slot N + 40 and the ratio
//! between them is a price that never existed on the chain -- and it is
//! indistinguishable from one that did, because both numbers are real and both
//! are checkable on an explorer. Forty slots is about sixteen seconds. On a coin
//! minutes old that is most of its history.
//!
//! So this module does not offer a way to read the accounts separately. The five
//! accounts it needs -- the pool, its two vaults and their two mints -- go to the
//! node in **one `getMultipleAccounts` call**, which answers with a single
//! `context.slot` covering all of them. That slot is [`PoolReserves::slot`], and
//! it is the only slot there is.
//!
//! **When it cannot be had, that is a refusal.** A node that omits the context
//! has not said when it read, and [`Unreadable::NoSlot`] is the answer rather
//! than a best-effort figure. Rule 9: unknown is not safe, and a reserve whose
//! instant is unknown is worse than no reserve, because it can be published.
//!
//! # The pool is read twice, and only the second read is kept
//!
//! The vault addresses are inside the pool account, so there is no way to ask
//! for all five in one call without first knowing which five. The first
//! `getAccountInfo` is **discovery only**: it says which accounts to ask for, and
//! every value it returned is then thrown away. The [`Pool`] in the result comes
//! out of the atomic call, alongside the balances, at their slot.
//!
//! That matters because it is exactly the shortcut worth taking and exactly the
//! bug worth catching: keeping the discovery pool would save a few hundred bytes
//! of parsing and would put the pool's own fields -- including `lp_supply` and
//! `virtual_quote_reserves` -- at a different instant from the balances beside
//! them. `the_pool_and_its_vaults_come_from_one_slot` is the test that catches
//! it.
//!
//! # What is not here
//!
//! No price, no impact, no capacity, no exit simulation, and **no effective
//! reserve**. The venue's rule is
//! `effective_quote_reserves = quote vault balance + virtual_quote_reserves`,
//! and research 0033 read the second term without establishing its meaning. What
//! this module reports is the raw vault balance, and [`Reserve`] says so.

use radar_decode::pumpswap;
use radar_pumpfun::Pool;
use radar_pumpfun::curve::Malformed;
use radar_pumpfun::token::{MintAccount, Reserve, TokenAccount, TokenMalformed};
use radar_types::{Address, Slot};

use crate::budget::Budget;
use crate::rpc::{MultiAccountRead, OwnedAccount, RpcClient, RpcError};

/// Which of the five accounts a failure is about.
///
/// Carried in every error that names an account, because "a vault was frozen"
/// and "the quote vault was frozen" are different amounts of help at three in
/// the morning.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// The pool account itself.
    Pool,
    /// The mint the pool is priced in units of.
    BaseMint,
    /// The mint the pool is priced in.
    QuoteMint,
    /// The token account holding the base reserve.
    BaseVault,
    /// The token account holding the quote reserve.
    QuoteVault,
}

impl Role {
    /// The order the five accounts are requested and returned in.
    ///
    /// One list, used to build the request and to read the response, so the two
    /// cannot drift apart. A response mapped onto the wrong roles would put the
    /// base balance against the quote mint's decimals -- a number wrong by a
    /// power of ten that looks entirely ordinary.
    pub const ORDER: [Self; 5] = [
        Self::Pool,
        Self::BaseMint,
        Self::QuoteMint,
        Self::BaseVault,
        Self::QuoteVault,
    ];

    /// A name for an error message.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pool => "pool",
            Self::BaseMint => "base mint",
            Self::QuoteMint => "quote mint",
            Self::BaseVault => "base vault",
            Self::QuoteVault => "quote vault",
        }
    }
}

impl core::fmt::Display for Role {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// A pool and both its reserves, all read at the same slot.
///
/// There is one [`slot`](Self::slot) and it covers everything in the struct.
/// That is not a convention this type asks callers to respect -- it is what the
/// only constructor can produce, because the only constructor takes a single
/// [`MultiAccountRead`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PoolReserves {
    /// The pool's address.
    pub address: Address,
    /// The pool account, read in the same call as the balances below.
    pub pool: Pool,
    /// The slot every figure here was read at.
    pub slot: Slot,
    /// The base side. Raw, not effective.
    pub base: Reserve,
    /// The quote side. Raw, not effective -- and **not lamports**: six of the ten
    /// pools captured on 2026-09-09 quote in something other than SOL.
    pub quote: Reserve,
}

/// Why a pool's reserves could not be read at one instant.
#[derive(Debug, thiserror::Error)]
pub enum Unreadable {
    /// The node could not be reached, or answered with something unreadable.
    #[error("{0}")]
    Rpc(#[from] RpcError),
    /// Nothing exists at that address.
    #[error("no account at {address}")]
    NoAccount {
        /// Where nothing was.
        address: Address,
    },
    /// One of the five accounts was absent from the atomic read.
    ///
    /// A vault that existed at discovery and not a moment later is a pool being
    /// torn down. Refused rather than reported as an empty reserve, which is
    /// what a zero balance would look like.
    #[error("the {role} at {address} was not there when the five were read together")]
    Missing {
        /// Which one.
        role: Role,
        /// Its address.
        address: Address,
    },
    /// The pool account is not owned by the PumpSwap program.
    #[error("{address} is owned by {owner}, not by PumpSwap")]
    NotAPool {
        /// The address asked about.
        address: Address,
        /// Who actually owns it.
        owner: String,
    },
    /// The node did not say which program owns an account.
    ///
    /// The owner is what distinguishes a classic SPL token account from a
    /// Token-2022 one, and it is not in the account's own bytes. Without it there
    /// is no way to know whether extensions could be present, so there is nothing
    /// to do but refuse.
    #[error("the node did not say which program owns the {role} at {address}")]
    NoOwner {
        /// Which one.
        role: Role,
        /// Its address.
        address: Address,
    },
    /// An owner came back as something that is not an address.
    #[error("the {role}'s owner {owner:?} is not an address")]
    UnreadableOwner {
        /// Which one.
        role: Role,
        /// What the node said.
        owner: String,
    },
    /// The pool account could not be parsed.
    ///
    /// `Debug` rather than `Display` in the message because
    /// [`Malformed`](radar_pumpfun::curve::Malformed) is a plain data enum with
    /// no `Error` impl, and giving it one is a change to a crate this task does
    /// not otherwise touch.
    #[error("the pool account is malformed: {0:?}")]
    Pool(Malformed),
    /// A vault or a mint could not be parsed, or was refused.
    ///
    /// This is where a Token-2022 extension arrives. The inner error names it.
    #[error("the {role}: {source}")]
    Token {
        /// Which one.
        role: Role,
        /// Why.
        source: TokenMalformed,
    },
    /// The pool no longer names the accounts that were asked for.
    ///
    /// Between discovery and the atomic read the pool's fields moved. A pool does
    /// not normally change its vaults or its mints, so this is either a program
    /// upgrade or a different account at the same address, and reading balances
    /// out of accounts the pool no longer claims would be reading somebody
    /// else's liquidity.
    #[error("the pool's {role} is {found} and {asked} was read")]
    PoolMoved {
        /// Which one moved.
        role: Role,
        /// What the pool names now.
        found: Address,
        /// What was actually read.
        asked: Address,
    },
    /// The node did not say which slot it read at.
    ///
    /// **A refusal, not a best effort.** Three balances whose instant is unknown
    /// are not three balances at an unknown instant; they may be three different
    /// instants, and nothing downstream could tell.
    #[error("the node did not say which slot it read these accounts at")]
    NoSlot,
    /// The node returned a different number of accounts than were asked for.
    #[error("asked for {asked} accounts and {returned} came back")]
    WrongCount {
        /// How many were asked for.
        asked: usize,
        /// How many came back.
        returned: usize,
    },
}

/// Reads a pool and both its reserves at one slot.
///
/// Two calls: one to discover which accounts the pool names, and one to read all
/// five of them together. Only the second one's values reach the result -- see
/// the module documentation for why that is the whole point.
///
/// # Errors
///
/// [`Unreadable`]. In particular [`NoSlot`](Unreadable::NoSlot) when the three
/// figures cannot be tied to one instant, and
/// [`Token`](Unreadable::Token) carrying the named Token-2022 extension when a
/// vault or mint holds one whose effect is not modelled.
pub fn read(
    client: &RpcClient,
    budget: &mut Budget,
    address: &Address,
) -> Result<PoolReserves, Unreadable> {
    let found = client
        .account(budget, address)?
        .ok_or(Unreadable::NoAccount { address: *address })?;
    // Discovery only. Nothing from this read survives into the result: it is
    // consulted for five addresses and then dropped.
    let discovered = Pool::parse(&found.data).map_err(Unreadable::Pool)?;
    let asked = [
        *address,
        discovered.base_mint,
        discovered.quote_mint,
        discovered.pool_base_token_account,
        discovered.pool_quote_token_account,
    ];

    let together = client.accounts(budget, &asked)?;
    // Deliberately not `discovered`. Reusing the pool already parsed is the one
    // shortcut here worth taking and the one bug worth catching: it would put
    // the pool's own fields a slot or more away from the balances beside them.
    // Verified by re-applying it -- `the_pool_and_its_vaults_come_from_one_slot`
    // fails on the slot with the whole shortcut, and on `lp_supply` with just
    // the pool reused.
    at_one_slot(&asked, &together)
}

/// Builds the reserves out of one multi-account read.
///
/// Separate from [`read`] and public because this is where every rule lives, and
/// a rule reachable only through a network call is a rule that is checked once
/// by hand and then trusted. `asked` is the request that produced `together`, in
/// [`Role::ORDER`].
///
/// # Errors
///
/// [`Unreadable`], as [`read`].
pub fn at_one_slot(
    asked: &[Address; 5],
    together: &MultiAccountRead,
) -> Result<PoolReserves, Unreadable> {
    // Before anything is parsed: if the node did not say when it read, there is
    // no instant to attribute these numbers to and nothing below is worth doing.
    let slot = together.slot.ok_or(Unreadable::NoSlot)?;
    if together.accounts.len() != asked.len() {
        return Err(Unreadable::WrongCount {
            asked: asked.len(),
            returned: together.accounts.len(),
        });
    }

    let mut present: Vec<(Role, Address, &OwnedAccount)> = Vec::with_capacity(asked.len());
    for ((role, address), account) in Role::ORDER.iter().zip(asked).zip(&together.accounts) {
        let account = account.as_ref().ok_or(Unreadable::Missing {
            role: *role,
            address: *address,
        })?;
        present.push((*role, *address, account));
    }

    let (_, pool_address, pool_account) = present[0];
    let owner = owner_of(Role::Pool, pool_address, pool_account)?;
    if owner != pumpswap::PROGRAM_ID {
        return Err(Unreadable::NotAPool {
            address: pool_address,
            owner: owner.to_string(),
        });
    }
    let pool = Pool::parse(&pool_account.data).map_err(Unreadable::Pool)?;

    // The pool as it stands *now* has to name the accounts that were actually
    // read. Anything else and the balances below belong to a pool that no longer
    // exists in this shape.
    for (role, found) in [
        (Role::BaseMint, pool.base_mint),
        (Role::QuoteMint, pool.quote_mint),
        (Role::BaseVault, pool.pool_base_token_account),
        (Role::QuoteVault, pool.pool_quote_token_account),
    ] {
        let asked = present[index_of(role)].1;
        if found != asked {
            return Err(Unreadable::PoolMoved { role, found, asked });
        }
    }

    let base = side(&present, Role::BaseVault, Role::BaseMint)?;
    let quote = side(&present, Role::QuoteVault, Role::QuoteMint)?;

    Ok(PoolReserves {
        address: pool_address,
        pool,
        slot,
        base,
        quote,
    })
}

/// Where a role sits in [`Role::ORDER`].
fn index_of(role: Role) -> usize {
    Role::ORDER
        .iter()
        .position(|candidate| *candidate == role)
        .expect("every role is in the order")
}

/// One side's reserve, from its vault and its mint.
fn side(
    present: &[(Role, Address, &OwnedAccount)],
    vault: Role,
    mint: Role,
) -> Result<Reserve, Unreadable> {
    let (_, vault_address, vault_account) = present[index_of(vault)];
    let (_, mint_address, mint_account) = present[index_of(mint)];

    let account = TokenAccount::parse(
        &vault_account.data,
        &owner_of(vault, vault_address, vault_account)?,
    )
    .map_err(|source| Unreadable::Token {
        role: vault,
        source,
    })?;
    let parsed_mint = MintAccount::parse(
        &mint_account.data,
        &owner_of(mint, mint_address, mint_account)?,
    )
    .map_err(|source| Unreadable::Token { role: mint, source })?;

    Reserve::new(vault_address, &account, mint_address, &parsed_mint).map_err(|source| {
        Unreadable::Token {
            role: vault,
            source,
        }
    })
}

/// The program that owns an account, refusing rather than guessing.
fn owner_of(role: Role, address: Address, account: &OwnedAccount) -> Result<Address, Unreadable> {
    let owner = account
        .owner
        .as_ref()
        .ok_or(Unreadable::NoOwner { role, address })?;
    owner.parse().map_err(|_| Unreadable::UnreadableOwner {
        role,
        owner: owner.clone(),
    })
}
