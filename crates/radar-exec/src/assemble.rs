// SPDX-License-Identifier: Apache-2.0
//! Compiling a Jupiter `/build` response into an unsigned Solana v0
//! transaction, with one zeroed signature slot for the fee payer.
//!
//! # Why this is hand-rolled
//!
//! No `solana-*` crate is anywhere in this workspace's dependency graph (there
//! is nothing to `grep` for). That is a standing choice, not an oversight:
//! [`radar_signer::tx`] decodes the whole legacy and v0 wire format from
//! scratch precisely so the one process holding a key never links a large,
//! frequently-revised SDK. Compiling a message is the mirror image of decoding
//! one, so this module continues that choice rather than reaching for
//! `solana-message`/`solana-transaction` for the one call site that needs to
//! write the format instead of read it. The wire format is small and stable
//! (signatures, header, static keys, blockhash, instructions, and the v0
//! address-table-lookups tail) and is exercised here against the committed
//! fixtures, not against a live network.
//!
//! # `tipInstruction`
//!
//! Both committed fixtures (`jupiter-build-sol-usdc.json`,
//! `jupiter-build-usdc-sol.json`) carry `"tipInstruction": null`, so nothing
//! this module has ever seen pays a Jito tip. [`assemble`] always omits the
//! field from the instruction list it compiles, and if a future response ever
//! sets it to something other than `null`, [`assemble`] refuses outright
//! ([`RouteError::Malformed`]) instead of either paying an account the caller
//! never asked about or silently dropping an instruction Jupiter's own answer
//! said should be there. ADR 0024 requires that no fee, tip or referral
//! account can creep into the assembled transaction; refusing is the only way
//! to keep that true unconditionally rather than "true of every response
//! captured so far."
//!
//! # Static keys versus lookup tables
//!
//! An account is compiled into the transaction's **static** key list when it
//! is the fee payer, when any instruction marks it as a signer, when it is
//! used as a `programId`, or when it appears in none of the response's
//! `addressesByLookupTableAddress` tables. Every other account — Jupiter's
//! routing pool and mint accounts, in practice — is left in its lookup table
//! and referenced through the v0 extended index space instead. Signers and
//! program IDs are kept static even when they also happen to appear in a
//! lookup table: nothing requires shrinking them, and a static reference is
//! unconditionally valid where a signer resolved through a lookup table is
//! not.
//!
//! When one address appears in more than one lookup table, the table whose
//! base58 address sorts first (ascending) wins, deterministically. Neither
//! fixture exercises this case, but the tie-break is total, so no address is
//! silently dropped if it ever does.

use std::collections::{BTreeMap, HashMap, HashSet};

use radar_types::{Address, b64};
use serde::Deserialize;

use crate::route::RouteError;

/// One instruction exactly as Jupiter's `/build` describes it.
#[derive(Debug, Clone, Deserialize)]
struct RawInstruction {
    #[serde(rename = "programId")]
    program_id: String,
    #[serde(default)]
    accounts: Vec<RawAccountMeta>,
    data: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAccountMeta {
    pubkey: String,
    #[serde(rename = "isSigner")]
    is_signer: bool,
    #[serde(rename = "isWritable")]
    is_writable: bool,
}

#[derive(Debug, Deserialize)]
struct RawBlockhashWithMetadata {
    /// A raw byte array on the wire, not base58 — see
    /// `crates/radar-exec/fixtures/README.md`.
    blockhash: Vec<u8>,
    #[serde(rename = "lastValidBlockHeight")]
    last_valid_block_height: u64,
}

/// The subset of Jupiter's `/build` body this module reads.
///
/// Deliberately separate from `route::BuildResponse`, which reads the quote
/// half of the same body: that type's own doc comment says its omission of
/// the instruction fields is on purpose, and this module existing is not a
/// reason to go back on that. The two parsers each look at the fields they
/// need and know nothing of each other.
#[derive(Debug, Deserialize)]
struct BuildInstructions {
    #[serde(rename = "computeBudgetInstructions", default)]
    compute_budget_instructions: Vec<RawInstruction>,
    #[serde(rename = "setupInstructions", default)]
    setup_instructions: Vec<RawInstruction>,
    #[serde(rename = "swapInstruction")]
    swap_instruction: RawInstruction,
    #[serde(rename = "cleanupInstruction")]
    cleanup_instruction: Option<RawInstruction>,
    #[serde(rename = "otherInstructions", default)]
    other_instructions: Vec<RawInstruction>,
    /// Read only to check it is `null`. See the module documentation.
    #[serde(rename = "tipInstruction")]
    tip_instruction: Option<serde_json::Value>,
    #[serde(rename = "addressesByLookupTableAddress", default)]
    addresses_by_lookup_table_address: BTreeMap<String, Vec<String>>,
    #[serde(rename = "blockhashWithMetadata")]
    blockhash_with_metadata: RawBlockhashWithMetadata,
}

/// An unsigned v0 transaction, ready for the fee payer to sign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledTransaction {
    /// The full serialized wire bytes: a shortvec signature count of `1`, one
    /// zeroed 64-byte signature slot, then the v0 message.
    pub wire: Vec<u8>,
    /// Jupiter's `lastValidBlockHeight` for the blockhash this transaction
    /// names — the height after which the blockhash (and so this
    /// transaction) can no longer land.
    pub last_valid_block_height: u64,
    /// The fee payer and sole signer, echoed back rather than requiring a
    /// caller to re-decode [`Self::wire`] to confirm it.
    pub fee_payer: Address,
}

impl AssembledTransaction {
    /// The wire bytes, base64-encoded — the form the API hands to a browser
    /// wallet.
    #[must_use]
    pub fn to_base64(&self) -> String {
        b64::encode(&self.wire)
    }
}

/// Compiles `body` (a Jupiter `/build` response) into an unsigned transaction
/// naming `taker` as fee payer and sole signer.
///
/// # Errors
///
/// [`RouteError::Malformed`] if the body does not parse, if any address or
/// the blockhash is the wrong shape, if `tipInstruction` is present and
/// non-null, or if compiling would need more than 256 distinct accounts (the
/// wire format's account index is a single byte, so this is a real limit and
/// not a round-number guess).
pub fn assemble(body: &str, taker: Address) -> Result<AssembledTransaction, RouteError> {
    let parsed: BuildInstructions =
        serde_json::from_str(body).map_err(|e| RouteError::Malformed(e.to_string()))?;

    if parsed.tip_instruction.is_some() {
        return Err(RouteError::Malformed(
            "response carries a tipInstruction; refusing rather than guessing whether it pays \
             only for the swap"
                .to_owned(),
        ));
    }

    let blockhash: [u8; 32] = parsed
        .blockhash_with_metadata
        .blockhash
        .try_into()
        .map_err(|v: Vec<u8>| {
            RouteError::Malformed(format!("blockhash is {} bytes, not 32", v.len()))
        })?;
    let last_valid_block_height = parsed.blockhash_with_metadata.last_valid_block_height;

    let instructions: Vec<RawInstruction> = parsed
        .compute_budget_instructions
        .into_iter()
        .chain(parsed.setup_instructions)
        .chain(std::iter::once(parsed.swap_instruction))
        .chain(parsed.cleanup_instruction)
        .chain(parsed.other_instructions)
        .collect();

    let plan = Plan::build(
        &instructions,
        taker,
        &parsed.addresses_by_lookup_table_address,
    )?;
    let message = plan.encode_message(&instructions, blockhash)?;

    let mut wire = Vec::with_capacity(1 + 64 + message.len());
    write_shortvec(&mut wire, 1); // one signature slot, for the fee payer
    wire.extend_from_slice(&[0u8; 64]); // zeroed: nothing here signs it
    wire.extend_from_slice(&message);

    Ok(AssembledTransaction {
        wire,
        last_valid_block_height,
        fee_payer: taker,
    })
}

/// Where one account will be found in the compiled message: a plain static
/// index, or an index into the v0 extended space a lookup table supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Static(u8),
    LookupWritable(u16),
    LookupReadonly(u16),
}

/// One resolved lookup-table reference in the compiled message.
struct CompiledLookup {
    table: Address,
    writable: Vec<u8>,
    readonly: Vec<u8>,
}

/// The compiled account layout: everything needed to write the header, the
/// static key list, and the lookup-table section, and to resolve every
/// instruction's accounts against them.
struct Plan {
    static_keys: Vec<Address>,
    num_required_signatures: u8,
    num_readonly_signed: u8,
    num_readonly_unsigned: u8,
    lookups: Vec<CompiledLookup>,
    slot_of: HashMap<Address, Slot>,
}

/// A lookup table's writable and readonly *original* member indices, keyed
/// by the table's address — the shape `Plan::build` accumulates before it
/// knows the final extended-index numbering.
type LookupMembers = BTreeMap<Address, (Vec<u8>, Vec<u8>)>;

/// Per-address (signer?, writable?) flags, keyed by address — the shape
/// `classify_accounts` derives from a walk of the raw instructions.
type AccountFlags = HashMap<Address, bool>;

/// `classify_accounts`'s full return: visitation order, signer flags,
/// writable flags, and the set of program-id addresses.
type ClassifiedAccounts = (Vec<Address>, AccountFlags, AccountFlags, HashSet<Address>);

/// Walks every instruction once, in first-appearance order, and returns:
/// the accounts named (with `taker` seeded first so it sorts first among
/// signers regardless of whether any instruction also names it), whether
/// each is ever asked to sign, whether each is ever asked to be writable,
/// and the set of addresses used as a program id (which — like signers —
/// must live in the static key list, never a lookup table).
fn classify_accounts(
    instructions: &[RawInstruction],
    taker: Address,
) -> Result<ClassifiedAccounts, RouteError> {
    let mut order: Vec<Address> = vec![taker];
    let mut seen: HashSet<Address> = HashSet::from([taker]);
    let mut is_signer: HashMap<Address, bool> = HashMap::from([(taker, true)]);
    let mut is_writable: HashMap<Address, bool> = HashMap::from([(taker, true)]);
    let mut programs: HashSet<Address> = HashSet::new();

    for ix in instructions {
        let program = parse_address(&ix.program_id)?;
        programs.insert(program);
        if seen.insert(program) {
            order.push(program);
        }
        is_signer.entry(program).or_insert(false);
        is_writable.entry(program).or_insert(false);

        for meta in &ix.accounts {
            let addr = parse_address(&meta.pubkey)?;
            if seen.insert(addr) {
                order.push(addr);
            }
            let signer = is_signer.entry(addr).or_insert(false);
            *signer = *signer || meta.is_signer;
            let writable = is_writable.entry(addr).or_insert(false);
            *writable = *writable || meta.is_writable;
        }
    }

    Ok((order, is_signer, is_writable, programs))
}

/// Which address a lookup table names at which index, and the reverse.
/// First table wins on a collision (ascending base58 order, since `tables`
/// iterates its `BTreeMap<String, _>` that way).
#[allow(clippy::type_complexity)]
fn index_lookup_tables(
    tables: &BTreeMap<String, Vec<String>>,
) -> Result<
    (
        HashMap<Address, (Address, u8)>,
        HashMap<(Address, u8), Address>,
    ),
    RouteError,
> {
    let mut membership: HashMap<Address, (Address, u8)> = HashMap::new();
    let mut idx_to_addr: HashMap<(Address, u8), Address> = HashMap::new();
    for (table, members) in tables {
        let table_addr = parse_address(table)?;
        for (idx, member) in members.iter().enumerate() {
            let member_addr = parse_address(member)?;
            let idx = u8::try_from(idx).map_err(|_| {
                RouteError::Malformed(format!("lookup table {table} has more than 256 members"))
            })?;
            idx_to_addr.entry((table_addr, idx)).or_insert(member_addr);
            membership.entry(member_addr).or_insert((table_addr, idx));
        }
    }
    Ok((membership, idx_to_addr))
}

/// Splits every account named by the instructions into the four static
/// groups (signer+writable, signer+readonly, static writable, static
/// readonly) plus the lookup-eligible accounts (bucketed by table, as
/// original per-table indices) — an account only lands in a lookup table
/// when it needs neither a signature nor static (program-id) placement.
fn partition_accounts(
    order: &[Address],
    is_signer: &HashMap<Address, bool>,
    is_writable: &HashMap<Address, bool>,
    programs: &HashSet<Address>,
    membership: &HashMap<Address, (Address, u8)>,
) -> (
    Vec<Address>,
    Vec<Address>,
    Vec<Address>,
    Vec<Address>,
    LookupMembers,
) {
    let mut signer_writable = Vec::new();
    let mut signer_readonly = Vec::new();
    let mut static_writable = Vec::new();
    let mut static_readonly = Vec::new();
    let mut by_table: LookupMembers = BTreeMap::new();

    for addr in order {
        let signer = is_signer[addr];
        let writable = is_writable[addr];
        let required_static = signer || programs.contains(addr);

        if !required_static && let Some((table, idx)) = membership.get(addr) {
            let entry = by_table.entry(*table).or_default();
            if writable {
                entry.0.push(*idx);
            } else {
                entry.1.push(*idx);
            }
            continue;
        }

        match (signer, writable) {
            (true, true) => signer_writable.push(*addr),
            (true, false) => signer_readonly.push(*addr),
            (false, true) => static_writable.push(*addr),
            (false, false) => static_readonly.push(*addr),
        }
    }

    (
        signer_writable,
        signer_readonly,
        static_writable,
        static_readonly,
        by_table,
    )
}

/// Numbers the lookup-eligible accounts into the wire format's extended
/// index space — every table's writable indices first, in table order,
/// then every table's readonly indices, in table order — and returns both
/// the wire-ready `CompiledLookup`s and each address's resolved `Slot`.
fn compile_lookups(
    by_table: LookupMembers,
    idx_to_addr: &HashMap<(Address, u8), Address>,
) -> Result<(Vec<CompiledLookup>, HashMap<Address, Slot>), RouteError> {
    let total_writable: usize = by_table.values().map(|(w, _)| w.len()).sum();
    let total_writable_u16 = u16::try_from(total_writable).map_err(|_| {
        RouteError::Malformed("more than 65536 lookup-writable accounts".to_owned())
    })?;

    let mut lookups = Vec::with_capacity(by_table.len());
    let mut slots: HashMap<Address, Slot> = HashMap::new();
    let mut writable_cursor: u16 = 0;
    let mut readonly_cursor: u16 = 0;
    for (table, (mut writable, mut readonly)) in by_table {
        writable.sort_unstable();
        readonly.sort_unstable();
        for &orig_idx in &writable {
            let addr = *idx_to_addr
                .get(&(table, orig_idx))
                .ok_or_else(|| RouteError::Malformed("lookup index vanished".to_owned()))?;
            slots.insert(addr, Slot::LookupWritable(writable_cursor));
            writable_cursor += 1;
        }
        for &orig_idx in &readonly {
            let addr = *idx_to_addr
                .get(&(table, orig_idx))
                .ok_or_else(|| RouteError::Malformed("lookup index vanished".to_owned()))?;
            slots.insert(
                addr,
                Slot::LookupReadonly(total_writable_u16 + readonly_cursor),
            );
            readonly_cursor += 1;
        }
        lookups.push(CompiledLookup {
            table,
            writable,
            readonly,
        });
    }

    Ok((lookups, slots))
}

impl Plan {
    fn build(
        instructions: &[RawInstruction],
        taker: Address,
        tables: &BTreeMap<String, Vec<String>>,
    ) -> Result<Self, RouteError> {
        let (order, is_signer, is_writable, programs) = classify_accounts(instructions, taker)?;
        let (membership, idx_to_addr) = index_lookup_tables(tables)?;
        let (signer_writable, signer_readonly, static_writable, static_readonly, by_table) =
            partition_accounts(&order, &is_signer, &is_writable, &programs, &membership);

        if signer_writable.first() != Some(&taker) {
            // Cannot happen: `taker` is seeded as (signer: true, writable:
            // true) in `classify_accounts` and nothing can weaken those
            // flags, only add to them. Kept as a hard check rather than an
            // assumption, since a fee payer anywhere but index 0 is a
            // transaction naming the wrong account as payer.
            return Err(RouteError::Malformed(
                "fee payer did not compile to the first signer slot".to_owned(),
            ));
        }

        let num_required_signatures =
            u8::try_from(signer_writable.len() + signer_readonly.len())
                .map_err(|_| RouteError::Malformed("more than 256 signers".to_owned()))?;
        let num_readonly_signed = u8::try_from(signer_readonly.len())
            .map_err(|_| RouteError::Malformed("more than 256 readonly signers".to_owned()))?;
        let num_readonly_unsigned = u8::try_from(static_readonly.len()).map_err(|_| {
            RouteError::Malformed("more than 256 readonly static accounts".to_owned())
        })?;

        let mut static_keys = Vec::with_capacity(
            signer_writable.len()
                + signer_readonly.len()
                + static_writable.len()
                + static_readonly.len(),
        );
        static_keys.extend(signer_writable);
        static_keys.extend(signer_readonly);
        static_keys.extend(static_writable);
        static_keys.extend(static_readonly);

        let mut slot_of: HashMap<Address, Slot> = HashMap::new();
        for (i, addr) in static_keys.iter().enumerate() {
            let idx = u8::try_from(i)
                .map_err(|_| RouteError::Malformed("more than 256 static accounts".to_owned()))?;
            slot_of.insert(*addr, Slot::Static(idx));
        }

        let (lookups, lookup_slots) = compile_lookups(by_table, &idx_to_addr)?;
        slot_of.extend(lookup_slots);

        Ok(Self {
            static_keys,
            num_required_signatures,
            num_readonly_signed,
            num_readonly_unsigned,
            lookups,
            slot_of,
        })
    }

    /// The virtual index of a resolved address, in the combined
    /// static-then-lookup-writable-then-lookup-readonly space the wire
    /// format uses for instruction account references.
    fn virtual_index(&self, addr: Address) -> Option<u16> {
        let base_lookup = self.static_keys.len();
        match *self.slot_of.get(&addr)? {
            Slot::Static(i) => Some(u16::from(i)),
            Slot::LookupWritable(i) | Slot::LookupReadonly(i) => {
                Some(u16::try_from(base_lookup).ok()? + i)
            }
        }
    }

    fn encode_message(
        &self,
        instructions: &[RawInstruction],
        blockhash: [u8; 32],
    ) -> Result<Vec<u8>, RouteError> {
        // 0x80: v0, high bit set per the wire format's version marker.
        let mut out = vec![
            0x80,
            self.num_required_signatures,
            self.num_readonly_signed,
            self.num_readonly_unsigned,
        ];

        write_shortvec(&mut out, self.static_keys.len());
        for key in &self.static_keys {
            out.extend_from_slice(key.as_bytes());
        }

        out.extend_from_slice(&blockhash);

        write_shortvec(&mut out, instructions.len());
        for ix in instructions {
            let program = parse_address(&ix.program_id)?;
            let program_index = self
                .virtual_index(program)
                .and_then(|v| u8::try_from(v).ok())
                .ok_or_else(|| {
                    RouteError::Malformed("program id did not compile to a static slot".to_owned())
                })?;
            out.push(program_index);

            write_shortvec(&mut out, ix.accounts.len());
            for meta in &ix.accounts {
                let addr = parse_address(&meta.pubkey)?;
                let idx = self.virtual_index(addr).ok_or_else(|| {
                    RouteError::Malformed(format!("{addr} did not compile to any slot"))
                })?;
                // The wire format's instruction account index is one byte,
                // even in the extended space: a v0 transaction cannot
                // address more than 256 accounts total, which is the same
                // limit `Plan::build` enforces on the static side.
                let idx = u8::try_from(idx).map_err(|_| {
                    RouteError::Malformed("more than 256 total accounts".to_owned())
                })?;
                out.push(idx);
            }

            let data = b64::decode(&ix.data).ok_or_else(|| {
                RouteError::Malformed(format!("instruction data is not base64: {}", ix.data))
            })?;
            write_shortvec(&mut out, data.len());
            out.extend_from_slice(&data);
        }

        write_shortvec(&mut out, self.lookups.len());
        for lookup in &self.lookups {
            out.extend_from_slice(lookup.table.as_bytes());
            write_shortvec(&mut out, lookup.writable.len());
            out.extend_from_slice(&lookup.writable);
            write_shortvec(&mut out, lookup.readonly.len());
            out.extend_from_slice(&lookup.readonly);
        }

        Ok(out)
    }
}

fn parse_address(s: &str) -> Result<Address, RouteError> {
    s.parse()
        .map_err(|_| RouteError::Malformed(format!("not a valid address: {s}")))
}

/// Writes `n` as a shortvec (compact-u16): 7 bits per byte, continuation in
/// the high bit, canonical (stops as soon as the remainder is zero).
fn write_shortvec(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let byte = u8::try_from(n & 0x7f).unwrap_or(0);
        n >>= 7;
        if n == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use radar_types::Address;

    use super::{assemble, write_shortvec};

    const SOL_USDC: &str = include_str!("../fixtures/jupiter-build-sol-usdc.json");
    const USDC_SOL: &str = include_str!("../fixtures/jupiter-build-usdc-sol.json");
    const TAKER: &str = "CjfBjFVBs6QRvRTpMdKTBxZ7PZuJvHXWQKGRvR7wFbdz";

    /// A minimal, independent reader for the wire this module writes: enough
    /// to walk signatures, header, static keys, blockhash, instructions and
    /// address table lookups back apart, without reusing any of
    /// `assemble`'s own encoding code. `radar_signer::tx::decode` cannot be
    /// reused here — it resolves every instruction account against the
    /// static key list alone and has no notion of the v0 extended index
    /// space, so it rejects (`AccountIndexOutOfRange`) exactly the
    /// lookup-table-bearing transactions this module exists to produce.
    struct Decoded {
        fee_payer: Address,
        num_required_signatures: u8,
        num_readonly_signed: u8,
        num_readonly_unsigned: u8,
        static_keys: Vec<Address>,
        blockhash: [u8; 32],
        instructions: Vec<(u8, Vec<u8>, Vec<u8>)>, // (program index, account indices, data)
        lookups: Vec<(Address, Vec<u8>, Vec<u8>)>, // (table, writable, readonly)
    }

    struct Reader<'a> {
        bytes: &'a [u8],
        pos: usize,
    }

    impl<'a> Reader<'a> {
        fn new(bytes: &'a [u8]) -> Self {
            Self { bytes, pos: 0 }
        }

        fn byte(&mut self) -> u8 {
            let b = self.bytes[self.pos];
            self.pos += 1;
            b
        }

        fn take(&mut self, n: usize) -> &'a [u8] {
            let s = &self.bytes[self.pos..self.pos + n];
            self.pos += n;
            s
        }

        fn shortvec(&mut self) -> usize {
            let mut value: usize = 0;
            let mut shift = 0;
            loop {
                let byte = self.byte();
                value |= usize::from(byte & 0x7f) << shift;
                if byte & 0x80 == 0 {
                    break;
                }
                shift += 7;
            }
            value
        }

        fn address(&mut self) -> Address {
            let bytes: [u8; 32] = self.take(32).try_into().unwrap();
            Address::new(bytes)
        }
    }

    fn decode(wire: &[u8]) -> Decoded {
        let mut r = Reader::new(wire);
        let sig_count = r.shortvec();
        assert_eq!(
            sig_count, 1,
            "exactly one signature slot, for the fee payer"
        );
        let sig = r.take(64);
        assert_eq!(
            sig, [0u8; 64],
            "the fee payer's slot must be zeroed, unsigned"
        );

        let version = r.byte();
        assert_eq!(version & 0x80, 0x80, "must be a versioned (v0) message");
        assert_eq!(version & 0x7f, 0, "must be version 0");

        let num_required_signatures = r.byte();
        let num_readonly_signed = r.byte();
        let num_readonly_unsigned = r.byte();

        let key_count = r.shortvec();
        let static_keys: Vec<Address> = (0..key_count).map(|_| r.address()).collect();
        let fee_payer = static_keys[0];

        let blockhash: [u8; 32] = r.take(32).try_into().unwrap();

        let ix_count = r.shortvec();
        let mut instructions = Vec::with_capacity(ix_count);
        for _ in 0..ix_count {
            let program_index = r.byte();
            let acc_count = r.shortvec();
            let accounts = (0..acc_count).map(|_| r.byte()).collect();
            let data_len = r.shortvec();
            let data = r.take(data_len).to_vec();
            instructions.push((program_index, accounts, data));
        }

        let lookup_count = r.shortvec();
        let mut lookups = Vec::with_capacity(lookup_count);
        for _ in 0..lookup_count {
            let table = r.address();
            let w_count = r.shortvec();
            let writable = (0..w_count).map(|_| r.byte()).collect();
            let ro_count = r.shortvec();
            let readonly = (0..ro_count).map(|_| r.byte()).collect();
            lookups.push((table, writable, readonly));
        }

        assert_eq!(r.pos, wire.len(), "no trailing bytes left unread");

        Decoded {
            fee_payer,
            num_required_signatures,
            num_readonly_signed,
            num_readonly_unsigned,
            static_keys,
            blockhash,
            instructions,
            lookups,
        }
    }

    /// One fixture instruction as (program id, [(pubkey, signer, writable)],
    /// data-base64), read independently of `assemble`'s internal
    /// `RawInstruction` — straight off `serde_json::Value` — so tests never
    /// compare `assemble`'s output against its own input types.
    type FixtureInstruction = (String, Vec<(String, bool, bool)>, String);

    fn fixture_instructions(body: &str) -> Vec<FixtureInstruction> {
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        let one = |ix: &serde_json::Value| {
            let program = ix["programId"].as_str().unwrap().to_owned();
            let accounts = ix["accounts"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|a| {
                    (
                        a["pubkey"].as_str().unwrap().to_owned(),
                        a["isSigner"].as_bool().unwrap(),
                        a["isWritable"].as_bool().unwrap(),
                    )
                })
                .collect();
            let data = ix["data"].as_str().unwrap().to_owned();
            (program, accounts, data)
        };
        let mut out = Vec::new();
        for ix in v["computeBudgetInstructions"].as_array().unwrap() {
            out.push(one(ix));
        }
        for ix in v["setupInstructions"].as_array().unwrap() {
            out.push(one(ix));
        }
        out.push(one(&v["swapInstruction"]));
        if !v["cleanupInstruction"].is_null() {
            out.push(one(&v["cleanupInstruction"]));
        }
        for ix in v["otherInstructions"].as_array().unwrap() {
            out.push(one(ix));
        }
        out
    }

    fn lookup_tables(body: &str) -> BTreeMap<String, Vec<String>> {
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        v["addressesByLookupTableAddress"]
            .as_object()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(k, members)| {
                let members = members
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|m| m.as_str().unwrap().to_owned())
                    .collect();
                (k, members)
            })
            .collect()
    }

    fn taker() -> Address {
        TAKER.parse().unwrap()
    }

    /// The (signer, writable) each address should compile to: the union, over
    /// every instruction, of what that instruction's own metas said about it
    /// — computed here independently of `assemble`'s `Plan::build`, which
    /// unions the same way, so this checks the union is done right rather
    /// than just replaying the same computation.
    fn expected_flags(
        expected: &[FixtureInstruction],
        taker: &str,
    ) -> HashMap<String, (bool, bool)> {
        let mut map: HashMap<String, (bool, bool)> = HashMap::new();
        map.insert(taker.to_owned(), (true, true));
        for (program, accounts, _) in expected {
            map.entry(program.clone()).or_insert((false, false));
            for (pubkey, signer, writable) in accounts {
                let entry = map.entry(pubkey.clone()).or_insert((false, false));
                entry.0 = entry.0 || *signer;
                entry.1 = entry.1 || *writable;
            }
        }
        map
    }

    /// Reads a compiled index's (signer, writable) flags straight off the
    /// decoded header and lookup-table split, independently of the `Slot`
    /// enum `assemble` uses internally.
    fn flags_at(decoded: &Decoded, idx: u8) -> (bool, bool) {
        let idx = usize::from(idx);
        let num_required_signatures = usize::from(decoded.num_required_signatures);
        if idx < num_required_signatures {
            let writable_signers =
                num_required_signatures - usize::from(decoded.num_readonly_signed);
            return (true, idx < writable_signers);
        }
        if idx < decoded.static_keys.len() {
            let readonly_start =
                decoded.static_keys.len() - usize::from(decoded.num_readonly_unsigned);
            return (false, idx < readonly_start);
        }
        let extended = idx - decoded.static_keys.len();
        let total_writable: usize = decoded.lookups.iter().map(|(_, w, _)| w.len()).sum();
        (false, extended < total_writable)
    }

    /// Resolves a compiled account index back to the address it names, for
    /// comparing against the fixture's own pubkeys.
    fn address_at(decoded: &Decoded, index: u8) -> Address {
        let index = usize::from(index);
        if index < decoded.static_keys.len() {
            return decoded.static_keys[index];
        }
        let extended = index - decoded.static_keys.len();
        let total_writable: usize = decoded.lookups.iter().map(|(_, w, _)| w.len()).sum();
        if extended < total_writable {
            let mut remaining = extended;
            for (table, writable, _) in &decoded.lookups {
                if remaining < writable.len() {
                    let orig_idx = writable[remaining];
                    return member_of(table, orig_idx);
                }
                remaining -= writable.len();
            }
        } else {
            let mut remaining = extended - total_writable;
            for (table, _, readonly) in &decoded.lookups {
                if remaining < readonly.len() {
                    let orig_idx = readonly[remaining];
                    return member_of(table, orig_idx);
                }
                remaining -= readonly.len();
            }
        }
        panic!("index {index} resolves to nothing");
    }

    // Test-local table membership, looked up by re-parsing the fixture: kept
    // separate from `assemble`'s own tables so this stays an independent
    // check.
    thread_local! {
        static TABLES_FOR_LOOKUP: std::cell::RefCell<BTreeMap<String, Vec<String>>> =
            const { std::cell::RefCell::new(BTreeMap::new()) };
    }

    fn member_of(table: &Address, idx: u8) -> Address {
        TABLES_FOR_LOOKUP.with(|t| {
            let t = t.borrow();
            let members = &t[&table.to_string()];
            members[usize::from(idx)].parse().unwrap()
        })
    }

    fn check_fixture(body: &str) {
        TABLES_FOR_LOOKUP.with(|t| *t.borrow_mut() = lookup_tables(body));

        let assembled = assemble(body, taker()).expect("both fixtures assemble");
        let decoded = decode(&assembled.wire);

        assert_eq!(decoded.fee_payer, taker(), "fee payer must be the taker");
        assert_eq!(assembled.fee_payer, taker());
        assert_eq!(
            decoded.num_required_signatures, 1,
            "only the taker signs; every fixture instruction marks it as the sole signer"
        );
        assert_eq!(decoded.num_readonly_signed, 0);
        assert!(
            usize::from(decoded.num_required_signatures)
                + usize::from(decoded.num_readonly_unsigned)
                <= decoded.static_keys.len()
        );

        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        let expected_last_valid = v["blockhashWithMetadata"]["lastValidBlockHeight"]
            .as_u64()
            .unwrap();
        assert_eq!(assembled.last_valid_block_height, expected_last_valid);
        let expected_blockhash: Vec<u8> = v["blockhashWithMetadata"]["blockhash"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| u8::try_from(b.as_u64().unwrap()).unwrap())
            .collect();
        assert_eq!(decoded.blockhash.to_vec(), expected_blockhash);

        // Lookup tables round-trip: every table the fixture offered that this
        // module actually used must resolve to exactly the members it named.
        for (table, writable, readonly) in &decoded.lookups {
            let table_str = table.to_string();
            let members = &lookup_tables(body)[&table_str];
            for &idx in writable.iter().chain(readonly) {
                assert!(
                    (idx as usize) < members.len(),
                    "table {table_str} index {idx} out of range"
                );
            }
        }

        // The instruction set equals exactly what /build returned (minus the
        // null tipInstruction, which both fixtures already omit): same
        // programs, same per-account (pubkey, signer, writable), same order,
        // same data, so no fee, tip or referral account could have crept in.
        let expected = fixture_instructions(body);
        assert_eq!(decoded.instructions.len(), expected.len());
        for (compiled, (exp_program, exp_accounts, exp_data)) in
            decoded.instructions.iter().zip(expected.iter())
        {
            let (program_idx, account_idxs, data) = compiled;
            let program = address_at(&decoded, *program_idx);
            assert_eq!(program.to_string(), *exp_program);

            assert_eq!(account_idxs.len(), exp_accounts.len());
            for (idx, (exp_pubkey, _, _)) in account_idxs.iter().zip(exp_accounts.iter()) {
                let addr = address_at(&decoded, *idx);
                assert_eq!(&addr.to_string(), exp_pubkey);
            }

            let expected_data = radar_types::b64::decode(exp_data).unwrap();
            assert_eq!(data, &expected_data);
        }

        // Every compiled account's signer/writable flags equal the union of
        // what the fixture's own instructions said about it — see
        // `expected_flags` and `flags_at`.
        let flags = expected_flags(&expected, TAKER);
        let total_lookup: usize = decoded
            .lookups
            .iter()
            .map(|(_, w, ro)| w.len() + ro.len())
            .sum();
        let total_accounts = decoded.static_keys.len() + total_lookup;
        for idx in 0..total_accounts {
            let idx = u8::try_from(idx).expect("both fixtures stay under 256 accounts");
            let addr = address_at(&decoded, idx);
            if let Some(&(exp_signer, exp_writable)) = flags.get(&addr.to_string()) {
                let (is_signer, is_writable) = flags_at(&decoded, idx);
                assert_eq!(is_signer, exp_signer, "signer flag for {addr}");
                assert_eq!(is_writable, exp_writable, "writable flag for {addr}");
            }
        }
    }

    #[test]
    fn a_sol_to_usdc_swap_assembles_with_five_lookup_tables() {
        check_fixture(SOL_USDC);
    }

    #[test]
    fn a_usdc_to_sol_swap_assembles_with_one_lookup_table() {
        check_fixture(USDC_SOL);
    }

    #[test]
    fn the_fee_payer_is_always_first_and_a_signer() {
        for body in [SOL_USDC, USDC_SOL] {
            let assembled = assemble(body, taker()).unwrap();
            let decoded = decode(&assembled.wire);
            assert_eq!(decoded.static_keys[0], taker());
            assert!(decoded.num_required_signatures >= 1);
        }
    }

    #[test]
    fn a_present_tip_instruction_is_refused_not_dropped_or_kept() {
        let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        v["tipInstruction"] = v["cleanupInstruction"].clone();
        let body = v.to_string();
        let err = assemble(&body, taker()).unwrap_err();
        assert!(
            format!("{err}").contains("tipInstruction"),
            "refusal should name the reason: {err}"
        );
    }

    #[test]
    fn a_bad_address_is_refused() {
        let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        v["swapInstruction"]["programId"] = serde_json::Value::String("not-base58!!".to_owned());
        let body = v.to_string();
        assert!(assemble(&body, taker()).is_err());
    }

    #[test]
    fn shortvec_encodes_canonically() {
        let mut out = Vec::new();
        write_shortvec(&mut out, 0);
        assert_eq!(out, vec![0]);

        let mut out = Vec::new();
        write_shortvec(&mut out, 127);
        assert_eq!(out, vec![0x7f]);

        let mut out = Vec::new();
        write_shortvec(&mut out, 128);
        assert_eq!(out, vec![0x80, 0x01]);

        let mut out = Vec::new();
        write_shortvec(&mut out, 300);
        assert_eq!(out, vec![0xac, 0x02]);
    }
}
