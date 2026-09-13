// SPDX-License-Identifier: Apache-2.0
//! The Yellowstone gRPC messages this crate reads and writes, and no others.
//!
//! Field numbers are copied from the vendored `proto/geyser.proto` and
//! `proto/solana-storage.proto` (Triton One's `yellowstone-grpc-proto` 12.7.0,
//! Apache-2.0). Protobuf skips fields a reader does not declare, so a message
//! here that omits most of the upstream fields still decodes the upstream bytes
//! correctly — **as long as every number that is declared is right.** That is
//! the one thing a hand-written copy can get wrong silently, and
//! `tests/the_messages_match_yellowstone.rs` checks each of them against the
//! `.proto` text rather than against a second hand-typed table.
//!
//! Everything is `pub` because the probe binary and those tests build and read
//! these directly; nothing outside this crate should need to.

#![allow(missing_docs, reason = "wire types; each field is named for its .proto field")]

use std::collections::HashMap;

/// `geyser.CommitmentLevel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum CommitmentLevel {
    Processed = 0,
    Confirmed = 1,
    Finalized = 2,
}

#[derive(Clone, PartialEq, prost::Message)]
#[expect(
    clippy::zero_sized_map_values,
    reason = "the wire format is a map of named filters, and blocks_meta's filter has no fields"
)]
pub struct SubscribeRequest {
    #[prost(map = "string, message", tag = "3")]
    pub transactions: HashMap<String, SubscribeRequestFilterTransactions>,
    #[prost(map = "string, message", tag = "5")]
    pub blocks_meta: HashMap<String, SubscribeRequestFilterBlocksMeta>,
    #[prost(enumeration = "CommitmentLevel", optional, tag = "6")]
    pub commitment: Option<i32>,
    #[prost(message, optional, tag = "9")]
    pub ping: Option<SubscribeRequestPing>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SubscribeRequestFilterTransactions {
    #[prost(bool, optional, tag = "1")]
    pub vote: Option<bool>,
    #[prost(bool, optional, tag = "2")]
    pub failed: Option<bool>,
    #[prost(string, repeated, tag = "3")]
    pub account_include: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct SubscribeRequestFilterBlocksMeta {}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct SubscribeRequestPing {
    #[prost(int32, tag = "1")]
    pub id: i32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SubscribeUpdate {
    #[prost(string, repeated, tag = "1")]
    pub filters: Vec<String>,
    #[prost(oneof = "UpdateOneof", tags = "4, 6, 7, 9")]
    pub update_oneof: Option<UpdateOneof>,
}

/// The variants of `SubscribeUpdate.update_oneof` this crate subscribes to.
///
/// Accounts, slots, whole blocks and entries are never requested, so they are
/// never sent; if a provider sent one anyway it would decode as `None` here
/// and be ignored, which is the right thing to do with a message nobody asked
/// for.
#[derive(Clone, PartialEq, prost::Oneof)]
#[expect(
    clippy::large_enum_variant,
    reason = "one update is decoded, folded and dropped at a time; boxing buys nothing"
)]
pub enum UpdateOneof {
    #[prost(message, tag = "4")]
    Transaction(SubscribeUpdateTransaction),
    #[prost(message, tag = "6")]
    Ping(SubscribeUpdatePing),
    #[prost(message, tag = "7")]
    BlockMeta(SubscribeUpdateBlockMeta),
    #[prost(message, tag = "9")]
    Pong(SubscribeUpdatePong),
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SubscribeUpdateTransaction {
    #[prost(message, optional, tag = "1")]
    pub transaction: Option<SubscribeUpdateTransactionInfo>,
    #[prost(uint64, tag = "2")]
    pub slot: u64,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SubscribeUpdateTransactionInfo {
    #[prost(bytes = "vec", tag = "1")]
    pub signature: Vec<u8>,
    #[prost(bool, tag = "2")]
    pub is_vote: bool,
    #[prost(message, optional, tag = "3")]
    pub transaction: Option<Transaction>,
    #[prost(message, optional, tag = "4")]
    pub meta: Option<TransactionStatusMeta>,
    #[prost(uint64, tag = "5")]
    pub index: u64,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct SubscribeUpdateBlockMeta {
    #[prost(uint64, tag = "1")]
    pub slot: u64,
    #[prost(message, optional, tag = "4")]
    pub block_time: Option<UnixTimestamp>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct SubscribeUpdatePing {}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct SubscribeUpdatePong {
    #[prost(int32, tag = "1")]
    pub id: i32,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct UnixTimestamp {
    #[prost(int64, tag = "1")]
    pub timestamp: i64,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Transaction {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub signatures: Vec<Vec<u8>>,
    #[prost(message, optional, tag = "2")]
    pub message: Option<Message>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Message {
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub account_keys: Vec<Vec<u8>>,
    #[prost(message, repeated, tag = "4")]
    pub instructions: Vec<CompiledInstruction>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct CompiledInstruction {
    #[prost(uint32, tag = "1")]
    pub program_id_index: u32,
    #[prost(bytes = "vec", tag = "2")]
    pub accounts: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub data: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct TransactionStatusMeta {
    #[prost(message, optional, tag = "1")]
    pub err: Option<TransactionError>,
    #[prost(uint64, tag = "2")]
    pub fee: u64,
    #[prost(uint64, repeated, tag = "3")]
    pub pre_balances: Vec<u64>,
    #[prost(uint64, repeated, tag = "4")]
    pub post_balances: Vec<u64>,
    #[prost(message, repeated, tag = "5")]
    pub inner_instructions: Vec<InnerInstructions>,
    #[prost(message, repeated, tag = "7")]
    pub pre_token_balances: Vec<TokenBalance>,
    #[prost(message, repeated, tag = "8")]
    pub post_token_balances: Vec<TokenBalance>,
    #[prost(bytes = "vec", repeated, tag = "12")]
    pub loaded_writable_addresses: Vec<Vec<u8>>,
    #[prost(bytes = "vec", repeated, tag = "13")]
    pub loaded_readonly_addresses: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct TransactionError {
    #[prost(bytes = "vec", tag = "1")]
    pub err: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct InnerInstructions {
    #[prost(uint32, tag = "1")]
    pub index: u32,
    #[prost(message, repeated, tag = "2")]
    pub instructions: Vec<InnerInstruction>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct InnerInstruction {
    #[prost(uint32, tag = "1")]
    pub program_id_index: u32,
    #[prost(bytes = "vec", tag = "2")]
    pub accounts: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub data: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct TokenBalance {
    #[prost(uint32, tag = "1")]
    pub account_index: u32,
    #[prost(string, tag = "2")]
    pub mint: String,
    #[prost(message, optional, tag = "3")]
    pub ui_token_amount: Option<UiTokenAmount>,
    #[prost(string, tag = "4")]
    pub owner: String,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct UiTokenAmount {
    #[prost(uint32, tag = "2")]
    pub decimals: u32,
    #[prost(string, tag = "3")]
    pub amount: String,
}
