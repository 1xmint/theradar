// SPDX-License-Identifier: Apache-2.0
//! One confirmed transaction, reduced to what the fold reads.
//!
//! A separate type from [`crate::proto`] for one reason: the decoder is tested
//! against real mainnet transactions fetched as RPC JSON, and the feed hands it
//! protobuf. Both become a [`Tx`], so the code the tests exercise is the code
//! the feed runs, and only the two small conversions differ.

use radar_types::{Address, Signature};

use crate::proto;

/// A transaction the fold can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tx {
    /// The slot it landed in.
    pub slot: u64,
    /// Its first signature, which is its identity.
    pub signature: Signature,
    /// Every account the transaction touches, in the order indices refer to:
    /// the message's own keys, then addresses loaded from lookup tables
    /// (writable, then read-only). An index into the message alone is wrong
    /// for any versioned transaction that uses a lookup table, which is most
    /// of the ones routed through an aggregator.
    pub accounts: Vec<Address>,
    /// The fee the signer paid, in lamports.
    pub fee: u64,
    /// Lamports per account before, parallel to [`Self::accounts`].
    pub pre_lamports: Vec<u64>,
    /// Lamports per account after.
    pub post_lamports: Vec<u64>,
    /// Token balances before.
    pub pre_tokens: Vec<TokenBalance>,
    /// Token balances after.
    pub post_tokens: Vec<TokenBalance>,
    /// Every instruction, top-level then inner, flattened. Order is not
    /// relied on: a launch is found wherever it sits, because launchpads call
    /// pump.fun's `create` from inside their own instruction.
    pub instructions: Vec<Instruction>,
}

/// A token account's balance of one mint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenBalance {
    /// Index into [`Tx::accounts`] naming the token account.
    pub account_index: usize,
    /// The mint.
    pub mint: Address,
    /// The wallet or program that owns the token account. `None` when the
    /// node did not say, and then this balance is attributed to nobody rather
    /// than guessed.
    pub owner: Option<Address>,
    /// Base units.
    pub amount: u64,
    /// The mint's decimals.
    pub decimals: u8,
}

/// One instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// Index into [`Tx::accounts`] naming the program.
    pub program: usize,
    /// Indices into [`Tx::accounts`], in the instruction's own order.
    pub accounts: Vec<usize>,
    /// The instruction data.
    pub data: Vec<u8>,
}

impl Tx {
    /// The account at an index, if the index is in range.
    #[must_use]
    pub fn account(&self, index: usize) -> Option<&Address> {
        self.accounts.get(index)
    }
}

/// Why a transaction update could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unreadable {
    /// The transaction failed on chain. Not an error in the feed, and not a
    /// trade: nothing moved.
    Failed,
    /// A vote. Never requested; refused anyway if one arrives.
    Vote,
    /// A field that must be present was missing or malformed. Counted, never
    /// half-read: a transaction with one balance unreadable would fold into a
    /// wrong delta rather than no delta.
    Malformed(&'static str),
}

fn address(bytes: &[u8], what: &'static str) -> Result<Address, Unreadable> {
    <[u8; 32]>::try_from(bytes)
        .map(Address::new)
        .map_err(|_| Unreadable::Malformed(what))
}

fn token_balance(b: &proto::TokenBalance) -> Result<TokenBalance, Unreadable> {
    let ui = b
        .ui_token_amount
        .as_ref()
        .ok_or(Unreadable::Malformed("token balance without an amount"))?;
    Ok(TokenBalance {
        account_index: usize::try_from(b.account_index)
            .map_err(|_| Unreadable::Malformed("token balance index"))?,
        mint: b
            .mint
            .parse()
            .map_err(|_| Unreadable::Malformed("token balance mint"))?,
        owner: b.owner.parse().ok(),
        amount: ui
            .amount
            .parse()
            .map_err(|_| Unreadable::Malformed("token balance amount"))?,
        decimals: u8::try_from(ui.decimals)
            .map_err(|_| Unreadable::Malformed("token balance decimals"))?,
    })
}

fn instruction(program: u32, accounts: &[u8], data: &[u8]) -> Instruction {
    Instruction {
        program: program as usize,
        accounts: accounts.iter().map(|a| usize::from(*a)).collect(),
        data: data.to_vec(),
    }
}

impl TryFrom<&proto::SubscribeUpdateTransaction> for Tx {
    type Error = Unreadable;

    fn try_from(update: &proto::SubscribeUpdateTransaction) -> Result<Self, Self::Error> {
        let info = update
            .transaction
            .as_ref()
            .ok_or(Unreadable::Malformed("update without a transaction"))?;
        if info.is_vote {
            return Err(Unreadable::Vote);
        }
        let meta = info
            .meta
            .as_ref()
            .ok_or(Unreadable::Malformed("transaction without meta"))?;
        if meta.err.is_some() {
            return Err(Unreadable::Failed);
        }
        let message = info
            .transaction
            .as_ref()
            .and_then(|t| t.message.as_ref())
            .ok_or(Unreadable::Malformed("transaction without a message"))?;

        let signature = <[u8; 64]>::try_from(info.signature.as_slice())
            .map(Signature::new)
            .map_err(|_| Unreadable::Malformed("signature"))?;

        let accounts = message
            .account_keys
            .iter()
            .chain(&meta.loaded_writable_addresses)
            .chain(&meta.loaded_readonly_addresses)
            .map(|k| address(k, "account key"))
            .collect::<Result<Vec<_>, _>>()?;
        if meta.pre_balances.len() != accounts.len() || meta.post_balances.len() != accounts.len() {
            return Err(Unreadable::Malformed(
                "balances do not line up with accounts",
            ));
        }

        let mut instructions: Vec<Instruction> = message
            .instructions
            .iter()
            .map(|i| instruction(i.program_id_index, &i.accounts, &i.data))
            .collect();
        for group in &meta.inner_instructions {
            instructions.extend(
                group
                    .instructions
                    .iter()
                    .map(|i| instruction(i.program_id_index, &i.accounts, &i.data)),
            );
        }

        Ok(Self {
            slot: update.slot,
            signature,
            accounts,
            fee: meta.fee,
            pre_lamports: meta.pre_balances.clone(),
            post_lamports: meta.post_balances.clone(),
            pre_tokens: meta
                .pre_token_balances
                .iter()
                .map(token_balance)
                .collect::<Result<_, _>>()?,
            post_tokens: meta
                .post_token_balances
                .iter()
                .map(token_balance)
                .collect::<Result<_, _>>()?,
            instructions,
        })
    }
}
