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
/// non-null, if compiling would need more than 256 distinct accounts (the
/// wire format's account index is a single byte, so this is a real limit and
/// not a round-number guess), or if the finished transaction is over
/// [`MAX_PACKET_BYTES`].
///
/// [`RouteError::PriorityFeeExceeded`] if `computeBudgetInstructions` would
/// spend more than [`MAX_PRIORITY_FEE_LAMPORTS`], if it holds an instruction
/// this module cannot read as a compute-budget setting, or if a Compute
/// Budget instruction is found outside `computeBudgetInstructions` (setup,
/// swap, cleanup or other instructions) -- the runtime reads Compute Budget
/// settings from anywhere in the message, so one hiding there would spend or
/// size the transaction without ever being priced.
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

    // Solana's runtime (Agave's `ComputeBudgetInstructionDetails::try_from`)
    // reads Compute Budget instructions wherever they sit in the message, not
    // only from a designated slice -- Jupiter's `computeBudgetInstructions`
    // field is a convention of its own response shape, not something the
    // chain enforces. An instruction naming the Compute Budget program
    // anywhere else would set a price or a limit that `check_priority_fee`
    // below never sees, so it is refused here before that check runs.
    for ix in parsed
        .setup_instructions
        .iter()
        .chain(std::iter::once(&parsed.swap_instruction))
        .chain(parsed.cleanup_instruction.iter())
        .chain(parsed.other_instructions.iter())
    {
        if ix.program_id == COMPUTE_BUDGET_PROGRAM {
            return Err(RouteError::PriorityFeeExceeded(
                "a Compute Budget instruction was found outside computeBudgetInstructions; the \
                 runtime would still execute it, unpriced"
                    .to_owned(),
            ));
        }
    }

    let non_compute_budget_count = parsed.setup_instructions.len()
        + 1 // swap_instruction
        + usize::from(parsed.cleanup_instruction.is_some())
        + parsed.other_instructions.len();
    check_priority_fee(
        &parsed.compute_budget_instructions,
        non_compute_budget_count,
    )?;

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
    write_shortvec(&mut wire, 1)?; // one signature slot, for the fee payer
    wire.extend_from_slice(&[0u8; 64]); // zeroed: nothing here signs it
    wire.extend_from_slice(&message);

    if wire.len() > MAX_PACKET_BYTES {
        // Address-free on purpose, same reasoning as `classify_accounts`'s
        // signer refusal just above in this module: this error can reach a
        // log line (`radar-serve`'s `route_error_response`) or a refusal
        // body, and a byte count is not visitor data.
        return Err(RouteError::Malformed(format!(
            "assembled transaction is {} bytes, over Solana's {MAX_PACKET_BYTES}-byte packet \
             limit",
            wire.len()
        )));
    }

    Ok(AssembledTransaction {
        wire,
        last_valid_block_height,
        fee_payer: taker,
    })
}

/// Solana's maximum packet size (`PACKET_DATA_SIZE`): a transaction serialized
/// larger than this cannot be sent over the wire at all, wallet signature
/// included. Checked here, against the exact bytes this module is about to
/// hand back, rather than left for the wallet or the RPC node to discover.
const MAX_PACKET_BYTES: usize = 1232;

/// The Compute Budget program's address. An instruction in
/// `computeBudgetInstructions` naming any other program is refused: its
/// bytes decoding like a price would say nothing about what it does.
const COMPUTE_BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";
/// The Compute Budget program's `SetComputeUnitLimit` instruction tag: a `u32`
/// unit count follows.
const SET_COMPUTE_UNIT_LIMIT: u8 = 2;
/// The Compute Budget program's `SetComputeUnitPrice` instruction tag: a `u64`
/// of micro-lamports per compute unit follows.
const SET_COMPUTE_UNIT_PRICE: u8 = 3;

/// Solana's own ceiling on compute units for one transaction
/// (`MAX_COMPUTE_UNIT_LIMIT`). The default this module assumes when Jupiter's
/// response names no explicit limit is capped at this, the same as the
/// runtime's own cap, so an assumed limit can never claim more than a
/// transaction could ever be granted.
const MAX_COMPUTE_UNITS: u64 = 1_400_000;

/// What one compute unit costs, in the absence of an explicit
/// `SetComputeUnitLimit`: 200,000 units for every instruction this module
/// cannot prove is a cheaper builtin.
///
/// This is Agave's own `DEFAULT_INSTRUCTION_COMPUTE_UNIT_LIMIT`
/// (`program-runtime/src/execution_budget.rs:31`, master as of 2026-09-27),
/// the per-instruction default `calculate_default_compute_unit_limit`
/// (`compute-budget-instruction/src/compute_budget_instruction_details.rs:196-217`)
/// charges any instruction it does not recognise as one of a short list of
/// non-migrated builtins. This module only ever recognises Compute Budget
/// instructions (below); every setup, swap, cleanup and other instruction --
/// including a System Program instruction, which the runtime would actually
/// charge [`BUILTIN_COMPUTE_UNITS`] for -- is priced at this higher rate.
/// That is deliberately conservative, in the same direction as
/// [`MAX_COMPUTE_UNITS`]: overcounting a builtin's cost only ever makes the
/// modelled limit (and so the modelled fee) larger than the runtime's own,
/// never smaller.
const DEFAULT_COMPUTE_UNITS_PER_INSTRUCTION: u64 = 200_000;

/// What the runtime charges a non-migrated builtin instruction by default --
/// Compute Budget instructions among them -- when no explicit
/// `SetComputeUnitLimit` is given.
///
/// Agave's `MAX_BUILTIN_ALLOCATION_COMPUTE_UNIT_LIMIT`
/// (`program-runtime/src/execution_budget.rs:34`, master as of 2026-09-27) is
/// `3_000`. `calculate_default_compute_unit_limit`
/// (`compute-budget-instruction/src/compute_budget_instruction_details.rs:196-217`)
/// re-walks every instruction in the message -- Compute Budget instructions
/// included, since the Compute Budget program is itself a non-migrated
/// builtin -- and charges each one this amount rather than
/// [`DEFAULT_COMPUTE_UNITS_PER_INSTRUCTION`]. Used only for the
/// `computeBudgetInstructions` this module reads; every other instruction is
/// charged the (higher, conservative) per-instruction default above.
const BUILTIN_COMPUTE_UNITS: u64 = 3_000;

/// The most a transaction this module assembles may spend on priority fees:
/// 0.001 SOL, in lamports.
///
/// A refused build costs the visitor nothing -- nothing was sent, nothing was
/// signed. A route that would spend six figures of lamports on priority fees
/// alone (the fixtures captured on 2026-09-09 pay on the order of 13,000) is
/// either a Jupiter default this module should not silently forward, or a
/// congested moment Radar should say so about rather than pay through.
pub const MAX_PRIORITY_FEE_LAMPORTS: u128 = 1_000_000;

/// Reads `SetComputeUnitPrice` and `SetComputeUnitLimit` out of Jupiter's
/// `computeBudgetInstructions` and refuses if the fee they would spend, at
/// `non_compute_budget_count` other instructions plus `instructions.len()`
/// compute-budget ones (each priced at [`BUILTIN_COMPUTE_UNITS`] when no
/// explicit limit is given -- see that constant's doc comment), is over
/// [`MAX_PRIORITY_FEE_LAMPORTS`] -- or if an instruction in that list is not
/// the Compute Budget program's, cannot be decoded as either, or repeats one
/// Agave only accepts once (`SetComputeUnitPrice` or `SetComputeUnitLimit`).
///
/// # Errors
///
/// [`RouteError::PriorityFeeExceeded`] for all three cases above. Refusing an
/// undecodable instruction rather than passing it through is the same
/// reasoning as the module's `tipInstruction` refusal: an instruction this
/// code cannot read is one it cannot prove is only a compute-budget setting.
/// Refusing a duplicate rather than keeping the last one seen is the same
/// reasoning applied to a transaction Agave would refuse outright
/// (`DuplicateInstruction`): a visitor should never be handed a transaction
/// to sign that can never land.
fn check_priority_fee(
    instructions: &[RawInstruction],
    non_compute_budget_count: usize,
) -> Result<(), RouteError> {
    let mut price_micro_lamports: u64 = 0;
    let mut explicit_limit: Option<u32> = None;
    let mut seen_price = false;
    let mut seen_limit = false;

    for ix in instructions {
        if ix.program_id != COMPUTE_BUDGET_PROGRAM {
            return Err(RouteError::PriorityFeeExceeded(format!(
                "compute budget list holds an instruction for {}, not the Compute Budget program",
                ix.program_id
            )));
        }
        let data = b64::decode(&ix.data).ok_or_else(|| {
            RouteError::Malformed(format!(
                "compute budget instruction data is not base64: {}",
                ix.data
            ))
        })?;
        match data.first() {
            Some(&SET_COMPUTE_UNIT_PRICE) if data.len() == 9 => {
                // Agave refuses a transaction carrying `SetComputeUnitPrice`
                // twice (`DuplicateInstruction`, compute-budget-instruction/
                // src/compute_budget_instruction_details.rs's
                // `process_instruction`): signing this would produce a
                // transaction that can never land.
                if seen_price {
                    return Err(RouteError::PriorityFeeExceeded(
                        "duplicate compute budget instruction: SetComputeUnitPrice appears twice"
                            .to_owned(),
                    ));
                }
                seen_price = true;
                let bytes: [u8; 8] = data[1..9]
                    .try_into()
                    .expect("checked length 9 above, so 8 bytes remain after the tag");
                price_micro_lamports = u64::from_le_bytes(bytes);
            }
            Some(&SET_COMPUTE_UNIT_LIMIT) if data.len() == 5 => {
                // Same rule, for `SetComputeUnitLimit`.
                if seen_limit {
                    return Err(RouteError::PriorityFeeExceeded(
                        "duplicate compute budget instruction: SetComputeUnitLimit appears twice"
                            .to_owned(),
                    ));
                }
                seen_limit = true;
                let bytes: [u8; 4] = data[1..5]
                    .try_into()
                    .expect("checked length 5 above, so 4 bytes remain after the tag");
                explicit_limit = Some(u32::from_le_bytes(bytes));
            }
            _ => {
                return Err(RouteError::PriorityFeeExceeded(format!(
                    "undecodable compute budget instruction: {} bytes, tag {:?}",
                    data.len(),
                    data.first()
                )));
            }
        }
    }

    let non_compute_budget_count = u64::try_from(non_compute_budget_count).unwrap_or(u64::MAX);
    // `instructions` is exactly the compute-budget list, so its length is the
    // count `calculate_default_compute_unit_limit` would charge
    // `BUILTIN_COMPUTE_UNITS` for -- see that constant's doc comment.
    let compute_budget_count = u64::try_from(instructions.len()).unwrap_or(u64::MAX);
    let default_limit = DEFAULT_COMPUTE_UNITS_PER_INSTRUCTION
        .saturating_mul(non_compute_budget_count)
        .saturating_add(BUILTIN_COMPUTE_UNITS.saturating_mul(compute_budget_count))
        .min(MAX_COMPUTE_UNITS);
    let limit = explicit_limit
        .map_or(default_limit, u64::from)
        .min(MAX_COMPUTE_UNITS);

    // Rounded up, as the runtime charges it: flooring would let a fee a
    // fraction of a lamport over the cap through.
    let fee_lamports = (u128::from(price_micro_lamports) * u128::from(limit)).div_ceil(1_000_000);
    if fee_lamports > MAX_PRIORITY_FEE_LAMPORTS {
        return Err(RouteError::PriorityFeeExceeded(format!(
            "priority fee would be {fee_lamports} lamports ({price_micro_lamports} micro-lamports \
             per unit at a {limit}-unit limit), over the {MAX_PRIORITY_FEE_LAMPORTS}-lamport cap"
        )));
    }
    Ok(())
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
            if meta.is_signer && addr != taker {
                // Jupiter's own instructions can name a signer that is not
                // the taker -- a delegate, a temporary account it expects to
                // create and sign for itself, or similar. Radar has no
                // signature for that address and never will: the wallet
                // signs once, for itself. Merging the address in as an
                // additional required signer would compile a transaction
                // whose header claims more signatures than the one the
                // wallet is about to produce (`Plan::build`'s
                // `num_required_signatures`), which is a malformed
                // transaction handed to the wallet naming someone else as a
                // signer -- refuse instead of building it.
                //
                // The message is address-free on purpose: this refusal is
                // surfaced to a visitor's session and logged by
                // `radar-serve`'s `route_error_response`, and neither the
                // visitor's own wallet nor the account Jupiter named belongs
                // in a log line or an error body -- see that function's own
                // doc comment.
                return Err(RouteError::Malformed(
                    "route names an account other than the taker as a signer".to_owned(),
                ));
            }
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

        // `classify_accounts` refuses (`RouteError::Malformed`) any account
        // that is a signer other than `taker`, before any of it reaches
        // `partition_accounts` -- so `taker` is the only address `is_signer`
        // is ever true for, and the check just above already established it
        // landed in `signer_writable`, not `signer_readonly`. One signer,
        // and it is never readonly: `num_required_signatures` is `1`, plain,
        // rather than a sum with a term that can only ever be `0` -- which
        // is what let `signer_writable.len() + signer_readonly.len()` look
        // like real arithmetic for a mutation test to flag when it was not.
        debug_assert!(
            signer_readonly.is_empty(),
            "the taker is seeded signer+writable and no other address may be a signer; \
             classify_accounts must have refused one that was"
        );
        let num_required_signatures: u8 = 1;
        let num_readonly_signed = u8::try_from(signer_readonly.len())
            .map_err(|_| RouteError::Malformed("more than 256 readonly signers".to_owned()))?;
        let num_readonly_unsigned = u8::try_from(static_readonly.len()).map_err(|_| {
            RouteError::Malformed("more than 256 readonly static accounts".to_owned())
        })?;

        // `order` names every account `classify_accounts` ever saw, and
        // `static_keys` is a subset of it (whatever `partition_accounts` did
        // not send to a lookup table) -- so its length is always a safe,
        // if occasionally loose, upper bound. Sized this way rather than as
        // a sum of the four partition lengths so there is no `+` here for a
        // mutation test to flag: capacity is an allocator hint with no
        // effect `static_keys.extend` below could ever make observable, so
        // a wrong arithmetic operator on the old sum was never something a
        // test could have caught by looking at the built transaction.
        let mut static_keys = Vec::with_capacity(order.len());
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

        write_shortvec(&mut out, self.static_keys.len())?;
        for key in &self.static_keys {
            out.extend_from_slice(key.as_bytes());
        }

        out.extend_from_slice(&blockhash);

        write_shortvec(&mut out, instructions.len())?;
        for ix in instructions {
            let program = parse_address(&ix.program_id)?;
            let program_index = self
                .virtual_index(program)
                .and_then(|v| u8::try_from(v).ok())
                .ok_or_else(|| {
                    RouteError::Malformed("program id did not compile to a static slot".to_owned())
                })?;
            out.push(program_index);

            write_shortvec(&mut out, ix.accounts.len())?;
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
            write_shortvec(&mut out, data.len())?;
            out.extend_from_slice(&data);
        }

        write_shortvec(&mut out, self.lookups.len())?;
        for lookup in &self.lookups {
            out.extend_from_slice(lookup.table.as_bytes());
            write_shortvec(&mut out, lookup.writable.len())?;
            out.extend_from_slice(&lookup.writable);
            write_shortvec(&mut out, lookup.readonly.len())?;
            out.extend_from_slice(&lookup.readonly);
        }

        Ok(out)
    }
}

fn parse_address(s: &str) -> Result<Address, RouteError> {
    s.parse()
        .map_err(|_| RouteError::Malformed(format!("not a valid address: {s}")))
}

/// The most bytes this module ever needs to encode a shortvec: 3 bytes
/// covers every value up to 2,097,151, and every count [`write_shortvec`] is
/// called with here — accounts, instructions, instruction data length,
/// static keys, lookup table entries — is bounded by the ~1,232-byte legacy
/// transaction size limit long before that.
const SHORTVEC_MAX_BYTES: usize = 3;

/// Writes `n` as a shortvec (compact-u16): 7 bits per byte, continuation in
/// the high bit, canonical (stops as soon as the remainder is zero).
///
/// # Errors
///
/// [`RouteError::Malformed`] if `n` is over [`u16::MAX`]. Compact-u16 is, by
/// name and by every caller's use of it here, an encoding of a *16-bit*
/// count -- but three 7-bit bytes can represent values up to 2,097,151
/// without ever tripping [`SHORTVEC_MAX_BYTES`]'s own bound. Without this
/// guard a count that overran `u16` (a bug upstream of this function, since
/// every real caller's count is independently bounded well under it) would
/// still produce a well-formed-*looking* three-byte shortvec that decodes
/// back to the wrong number of items on the other end -- silently, not as a
/// truncation `SHORTVEC_MAX_BYTES` would catch. Refusing is the same choice
/// `assemble` already makes for every other "this cannot happen, but if it
/// does, say so" case.
///
/// Bounded by [`SHORTVEC_MAX_BYTES`] rather than `loop { .. }` on purpose: a
/// single comparison flipped by mutation testing turned an unbounded loop
/// into one that never returns for `n == 0` — a real, reached input (an
/// empty lookup table's readonly-address count, among others) — which
/// cargo-mutants can only report as a 60-second timeout rather than a caught
/// mutant. Same shape as `radar_pumpfun::transaction::compact_u16`, which
/// needed this exact bound first.
fn write_shortvec(out: &mut Vec<u8>, mut n: usize) -> Result<(), RouteError> {
    if n > usize::from(u16::MAX) {
        return Err(RouteError::Malformed(
            "count is over compact-u16's 16-bit range".to_owned(),
        ));
    }
    for _ in 0..SHORTVEC_MAX_BYTES {
        let byte = u8::try_from(n & 0x7f).unwrap_or(0);
        n >>= 7;
        if n == 0 {
            out.push(byte);
            return Ok(());
        }
        out.push(byte | 0x80);
    }
    debug_assert!(
        n == 0,
        "shortvec value truncated: does not fit in {SHORTVEC_MAX_BYTES} bytes"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use radar_types::{Address, b64};

    use super::{
        COMPUTE_BUDGET_PROGRAM, MAX_PRIORITY_FEE_LAMPORTS, RawInstruction, RouteError, assemble,
        check_priority_fee, write_shortvec,
    };

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

    /// The fixture's own `setupInstructions`/`swapInstruction` name `TAKER`
    /// as a signer. Assembling it for a *different* taker must be refused,
    /// not silently compiled with two names claiming a signature the wallet
    /// never gives: a transaction whose header says
    /// `num_required_signatures >= 2` while only one signature (the caller's)
    /// will ever be produced is malformed, and a wallet asked to sign it
    /// would be naming someone else as a required signer without being told.
    #[test]
    fn a_signer_that_is_not_the_taker_is_refused_rather_than_merged_in() {
        let someone_else = Address::new([0x42; 32]);
        let err = assemble(SOL_USDC, someone_else).expect_err(
            "the fixture's setup and swap instructions name TAKER as a signer, and \
             someone_else is not TAKER",
        );
        assert!(
            matches!(err, super::RouteError::Malformed(ref m) if m.contains("signer")),
            "the refusal must name why, got {err}"
        );
    }

    /// This refusal is surfaced to a visitor's session and logged verbatim by
    /// `radar-serve`'s `route_error_response` (which only ever logs a fixed
    /// label, never an error's `Display`, but the error body itself must
    /// still be address-free as defense in depth). Neither the taker's own
    /// wallet nor the account Jupiter incorrectly named as an extra signer
    /// belongs in a log line, so the message text must not contain either
    /// address's base58 form.
    #[test]
    fn the_extra_signer_refusal_does_not_log_either_wallet_address() {
        let someone_else = Address::new([0x42; 32]);
        let err = assemble(SOL_USDC, someone_else)
            .expect_err("the fixture names TAKER as a signer, and someone_else is not TAKER");
        let message = match &err {
            super::RouteError::Malformed(m) => m,
            other => panic!("expected Malformed, got {other:?}"),
        };
        assert!(
            !message.contains(TAKER),
            "message must not log the taker's address: {message}"
        );
        assert!(
            !message.contains(someone_else.to_string().as_str()),
            "message must not log the extra signer's address: {message}"
        );
    }

    /// A transaction that would serialize larger than Solana's
    /// [`super::MAX_PACKET_BYTES`] packet limit is refused rather than handed
    /// back for a wallet (or an RPC node) to discover the hard way. Inflating
    /// the swap instruction's own data is enough to push the fixture over the
    /// limit without touching account counts or lookup tables.
    #[test]
    fn a_transaction_over_the_packet_limit_is_refused() {
        let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        let oversized_data = radar_types::b64::encode(&vec![0u8; 2000]);
        v["swapInstruction"]["data"] = serde_json::Value::String(oversized_data);
        let body = v.to_string();

        let err = assemble(&body, taker())
            .expect_err("2000 bytes of instruction data alone is over the 1232-byte packet limit");
        assert!(
            matches!(err, super::RouteError::Malformed(ref m) if m.contains("1232")),
            "the refusal must name the packet limit, got {err}"
        );
    }

    /// A transaction that serializes to exactly [`super::MAX_PACKET_BYTES`]
    /// must still be accepted -- the guard above is a limit, not a margin.
    /// This pins the boundary itself: `wire.len() > MAX_PACKET_BYTES` and
    /// `wire.len() >= MAX_PACKET_BYTES` both refuse everything one byte past
    /// the limit, and only a case sitting exactly on it tells them apart.
    ///
    /// The swap instruction's data length needed to land exactly on the
    /// limit is derived, not hardcoded, because the exact byte count depends
    /// on the fixture's account layout and shortvec encoding, neither of
    /// which this test should have to know in advance. Growing that data
    /// length by one grows the wire by exactly one byte, with one exception:
    /// the single step where the data length's own shortvec prefix grows
    /// from one byte to two (data length 127 -> 128) grows the wire by two,
    /// skipping exactly one otherwise-reachable wire length. The `assert_ne`
    /// below names that skipped offset rather than silently landing on the
    /// wrong length if the limit and the fixture ever line up on it.
    #[test]
    fn a_transaction_at_exactly_the_packet_limit_is_accepted() {
        fn wire_len_for_data_len(n: usize) -> usize {
            let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
            let data = radar_types::b64::encode(&vec![0u8; n]);
            v["swapInstruction"]["data"] = serde_json::Value::String(data);
            let body = v.to_string();
            assemble(&body, taker())
                .expect("a data length picked to land on, not over, the limit must assemble")
                .wire
                .len()
        }

        let target = super::MAX_PACKET_BYTES;
        let base = wire_len_for_data_len(0);
        assert!(
            target >= base,
            "the fixture's baseline wire length ({base} bytes, empty swap data) already exceeds \
             the packet limit ({target} bytes); this test's assumptions no longer hold"
        );
        let offset = target - base;
        assert_ne!(
            offset, 128,
            "the packet limit sits exactly on the one offset this instruction's data length \
             alone cannot reach (its shortvec length prefix growing from one byte to two skips \
             it); this test needs a second knob to reach the limit"
        );
        let n = if offset <= 127 { offset } else { offset - 1 };

        let len = wire_len_for_data_len(n);
        assert_eq!(
            len, target,
            "data_len={n} should have produced a wire of exactly {target} bytes, got {len}"
        );
    }

    /// `AssembledTransaction::to_base64` must encode exactly the compiled
    /// wire bytes -- not merely return *some* non-empty string. Decoding it
    /// back and comparing against `wire` catches a stub implementation that
    /// returns a fixed placeholder just as surely as one that returns an
    /// empty string.
    #[test]
    fn to_base64_round_trips_the_exact_wire_bytes() {
        let assembled = assemble(SOL_USDC, taker()).unwrap();
        let decoded = radar_types::b64::decode(&assembled.to_base64())
            .expect("to_base64 must produce valid base64");
        assert_eq!(
            decoded, assembled.wire,
            "to_base64 must encode exactly the compiled wire bytes, not a placeholder"
        );
    }

    /// A program id is kept static even when the same address also appears
    /// in a lookup table (module doc, "Static keys versus lookup tables").
    /// This exercises `partition_accounts`'s `required_static = signer ||
    /// programs.contains(addr)` and its `!required_static` guard directly:
    /// flipping the `||` to `&&`, or dropping the `!`, both let this
    /// non-signer program id slip into the lookup-table branch instead of
    /// staying in the static key list.
    #[test]
    fn a_program_id_that_also_sits_in_a_lookup_table_stays_static() {
        let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        let program = v["swapInstruction"]["programId"]
            .as_str()
            .unwrap()
            .to_owned();
        let tables = v["addressesByLookupTableAddress"].as_object_mut().unwrap();
        let (_, first_table) = tables
            .iter_mut()
            .next()
            .expect("the SOL/USDC fixture has at least one lookup table");
        first_table
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::String(program.clone()));
        let body = v.to_string();

        let assembled = assemble(&body, taker())
            .expect("still assembles once the program id is duplicated into a lookup table");
        let decoded = decode(&assembled.wire);
        let program_addr: Address = program.parse().unwrap();
        assert!(
            decoded.static_keys.contains(&program_addr),
            "a program id must always compile into the static key list, even when a lookup \
             table also lists it"
        );
    }

    #[test]
    fn shortvec_encodes_canonically() {
        let mut out = Vec::new();
        write_shortvec(&mut out, 0).unwrap();
        assert_eq!(out, vec![0]);

        let mut out = Vec::new();
        write_shortvec(&mut out, 127).unwrap();
        assert_eq!(out, vec![0x7f]);

        let mut out = Vec::new();
        write_shortvec(&mut out, 128).unwrap();
        assert_eq!(out, vec![0x80, 0x01]);

        let mut out = Vec::new();
        write_shortvec(&mut out, 300).unwrap();
        assert_eq!(out, vec![0xac, 0x02]);

        // The three boundary values right around the two-byte/three-byte
        // seams, and the largest value compact-u16 is defined for at all
        // (`u16::MAX`) -- see `write_shortvec`'s own guard, tested separately
        // below.
        let mut out = Vec::new();
        write_shortvec(&mut out, 16383).unwrap();
        assert_eq!(out, vec![0xff, 0x7f]);

        let mut out = Vec::new();
        write_shortvec(&mut out, 16384).unwrap();
        assert_eq!(out, vec![0x80, 0x80, 0x01]);

        let mut out = Vec::new();
        write_shortvec(&mut out, 65535).unwrap();
        assert_eq!(out, vec![0xff, 0xff, 0x03]);
    }

    /// A count over `u16::MAX` is refused rather than silently compiled into
    /// a three-byte shortvec that looks well-formed but decodes back to the
    /// wrong number on the other end -- see `write_shortvec`'s own doc
    /// comment for why a fourth-byte-worth of value is representable in three
    /// 7-bit bytes but not valid compact-u16.
    #[test]
    fn shortvec_refuses_a_count_over_u16_max() {
        let mut out = Vec::new();
        let err = write_shortvec(&mut out, usize::from(u16::MAX) + 1)
            .expect_err("65,536 is one past compact-u16's range");
        assert!(matches!(err, super::RouteError::Malformed(_)), "got {err}");
        assert!(
            out.is_empty(),
            "nothing is written once the count is refused"
        );
    }

    fn compute_unit_price(micro_lamports: u64) -> RawInstruction {
        let mut data = vec![3u8];
        data.extend_from_slice(&micro_lamports.to_le_bytes());
        RawInstruction {
            program_id: "ComputeBudget111111111111111111111111111111".to_owned(),
            accounts: Vec::new(),
            data: b64::encode(&data),
        }
    }

    fn compute_unit_limit(units: u32) -> RawInstruction {
        let mut data = vec![2u8];
        data.extend_from_slice(&units.to_le_bytes());
        RawInstruction {
            program_id: "ComputeBudget111111111111111111111111111111".to_owned(),
            accounts: Vec::new(),
            data: b64::encode(&data),
        }
    }

    /// `RawInstruction` has no `Serialize` impl (nothing outside tests needs
    /// to write one back out), so tests that plant an instruction into a
    /// fixture's JSON build the object by hand instead.
    fn ix_json(ix: &RawInstruction) -> serde_json::Value {
        serde_json::json!({
            "programId": ix.program_id,
            "accounts": [],
            "data": ix.data,
        })
    }

    /// Finding 1, failure A: an empty `computeBudgetInstructions` said
    /// nothing about it, but `otherInstructions` (which nothing checked)
    /// carried a real `SetComputeUnitPrice`. The runtime reads Compute
    /// Budget instructions from anywhere in the message
    /// (`ComputeBudgetInstructionDetails::try_from`), so this would have
    /// spent 50,000,000 micro-lamports a unit -- on the order of 0.04 SOL --
    /// while `check_priority_fee` saw an empty list and priced it at zero.
    #[test]
    fn a_compute_budget_instruction_hidden_in_other_instructions_is_refused() {
        let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        v["computeBudgetInstructions"] = serde_json::Value::Array(Vec::new());
        v["otherInstructions"] =
            serde_json::Value::Array(vec![ix_json(&compute_unit_price(50_000_000))]);
        let err = assemble(&v.to_string(), taker()).unwrap_err();
        let RouteError::PriorityFeeExceeded(msg) = &err else {
            panic!("got {err}");
        };
        assert!(
            msg.contains("outside computeBudgetInstructions"),
            "got {err}"
        );
    }

    /// Finding 1, failure B: a `SetComputeUnitLimit` planted in
    /// `setupInstructions` would still be read by the runtime and would
    /// widen the real compute-unit limit past whatever
    /// `computeBudgetInstructions` modelled, the same passthrough Finding B
    /// (`the_real_fixtures_priority_fee_is_within_the_cap`) closed for the
    /// designated list.
    #[test]
    fn a_compute_budget_instruction_hidden_in_setup_instructions_is_refused() {
        let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        let mut setup = v["setupInstructions"].as_array().unwrap().clone();
        setup.push(ix_json(&compute_unit_limit(1_400_000)));
        v["setupInstructions"] = serde_json::Value::Array(setup);
        let err = assemble(&v.to_string(), taker()).unwrap_err();
        let RouteError::PriorityFeeExceeded(msg) = &err else {
            panic!("got {err}");
        };
        assert!(
            msg.contains("outside computeBudgetInstructions"),
            "got {err}"
        );
    }

    /// The same loop reaches `swapInstruction`, which — unlike setup, cleanup
    /// and other — is a single required object rather than an array: a
    /// per-slice check would miss it, so the loop must reach it too.
    #[test]
    fn a_compute_budget_instruction_hidden_in_swap_instruction_is_refused() {
        let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        v["swapInstruction"]["programId"] =
            serde_json::Value::String(COMPUTE_BUDGET_PROGRAM.to_owned());
        let err = assemble(&v.to_string(), taker()).unwrap_err();
        let RouteError::PriorityFeeExceeded(msg) = &err else {
            panic!("got {err}");
        };
        assert!(
            msg.contains("outside computeBudgetInstructions"),
            "got {err}"
        );
    }

    /// Same again for `cleanupInstruction`, present in the fixture and
    /// checked through `Option::iter` rather than a `Vec`.
    #[test]
    fn a_compute_budget_instruction_hidden_in_cleanup_instruction_is_refused() {
        let mut v: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        assert!(
            !v["cleanupInstruction"].is_null(),
            "fixture must carry a cleanup instruction for this test to say anything"
        );
        v["cleanupInstruction"]["programId"] =
            serde_json::Value::String(COMPUTE_BUDGET_PROGRAM.to_owned());
        let err = assemble(&v.to_string(), taker()).unwrap_err();
        let RouteError::PriorityFeeExceeded(msg) = &err else {
            panic!("got {err}");
        };
        assert!(
            msg.contains("outside computeBudgetInstructions"),
            "got {err}"
        );
    }

    /// Finding B: passing `computeBudgetInstructions` through unread meant a
    /// widened priority fee never bound. These decode the two instructions
    /// Jupiter actually sends and check the cap that stands in for the
    /// passthrough now.
    #[test]
    fn the_real_fixtures_priority_fee_is_within_the_cap() {
        // Real captures, real math: 9386 micro-lamports/unit (sol-usdc) and
        // 9583 (usdc-sol), each at a default limit sized off the fixture's
        // own instruction counts -- 6 non-compute-budget + 1 compute-budget
        // for sol-usdc (200_000*6 + 3_000*1 = 1,203,000 units), 3 + 1 for
        // usdc-sol (200_000*3 + 3_000*1 = 603,000 units) -- both well under
        // the cap.
        let sol_usdc: serde_json::Value = serde_json::from_str(SOL_USDC).unwrap();
        let ixs: Vec<RawInstruction> =
            serde_json::from_value(sol_usdc["computeBudgetInstructions"].clone()).unwrap();
        assert!(
            check_priority_fee(&ixs, 6).is_ok(),
            "the sol-usdc fixture must still build"
        );

        let usdc_sol: serde_json::Value = serde_json::from_str(USDC_SOL).unwrap();
        let ixs: Vec<RawInstruction> =
            serde_json::from_value(usdc_sol["computeBudgetInstructions"].clone()).unwrap();
        assert!(
            check_priority_fee(&ixs, 3).is_ok(),
            "the usdc-sol fixture must still build"
        );
    }

    #[test]
    fn a_price_over_the_cap_is_refused() {
        // At a 1,203,000-unit default limit (6 non-compute-budget
        // instructions at 200,000 each, plus this one compute-budget
        // instruction at 3,000: 200_000*6 + 3_000*1), this price spends well
        // over the cap.
        let ixs = vec![compute_unit_price(1_000_000)];
        let err = check_priority_fee(&ixs, 6).expect_err("an over-cap price must refuse");
        assert!(
            matches!(err, RouteError::PriorityFeeExceeded(_)),
            "got {err}"
        );
    }

    #[test]
    fn the_boundary_exactly_at_the_cap_is_accepted() {
        let limit = 200_000u32;
        let price =
            u64::try_from(MAX_PRIORITY_FEE_LAMPORTS * 1_000_000 / u128::from(limit)).unwrap();
        let ixs = vec![compute_unit_limit(limit), compute_unit_price(price)];
        assert!(
            check_priority_fee(&ixs, 0).is_ok(),
            "the boundary itself must not be refused"
        );
    }

    #[test]
    fn one_micro_lamport_over_the_boundary_is_refused() {
        let limit = 200_000u32;
        let price =
            u64::try_from(MAX_PRIORITY_FEE_LAMPORTS * 1_000_000 / u128::from(limit)).unwrap() + 1;
        let ixs = vec![compute_unit_limit(limit), compute_unit_price(price)];
        let err =
            check_priority_fee(&ixs, 0).expect_err("one micro-lamport over the cap must refuse");
        assert!(
            matches!(err, RouteError::PriorityFeeExceeded(_)),
            "got {err}"
        );
    }

    /// `body` with its one compute-budget instruction's price replaced.
    fn with_price(body: &str, micro_lamports: u64) -> String {
        let mut v: serde_json::Value = serde_json::from_str(body).unwrap();
        v["computeBudgetInstructions"][0]["data"] =
            serde_json::Value::String(compute_unit_price(micro_lamports).data);
        v.to_string()
    }

    /// Jupiter sends a price and never a limit, so in production the fee is
    /// priced off the instruction counts `assemble` passes in. These pin that
    /// count through `assemble` itself: sol-usdc has 4 setup + swap + cleanup
    /// = 6 non-compute-budget instructions plus its 1 compute-budget price
    /// instruction, for a default limit of `200_000*6 + 3_000*1 =
    /// 1,203,000` units.
    ///
    /// The last price under the cap is the largest `price` with
    /// `price * 1,203,000 <= 1,000,000 * 1_000_000` (the cap in
    /// micro-lamports, since the fee rounds up): `1,000,000,000,000 /
    /// 1,203,000 = 831,255.19...`, so `831,255` is last under (`831,255 *
    /// 1,203,000 = 999,999,765,000`, fee `= ceil(999,999,765,000 / 1e6) =
    /// 1,000,000`, exactly at the cap) and `831,256` is first over
    /// (`831,256 * 1,203,000 = 1,000,000,968,000`, fee `= 1,000,001`, over
    /// the cap). A count one lower would let `831,256` through; one higher
    /// would refuse `831,255`.
    #[test]
    fn the_instruction_count_prices_the_fee_at_the_last_price_under_the_cap() {
        assert!(assemble(&with_price(SOL_USDC, 831_255), taker()).is_ok());
    }

    #[test]
    fn the_instruction_count_prices_the_fee_at_the_first_price_over_the_cap() {
        let err = assemble(&with_price(SOL_USDC, 831_256), taker()).unwrap_err();
        assert!(
            matches!(err, RouteError::PriorityFeeExceeded(_)),
            "got {err}"
        );
    }

    /// Neither capture has `otherInstructions`, so this adds one: usdc-sol's
    /// 1 setup + swap + cleanup + 1 other = 4 non-compute-budget instructions
    /// plus its 1 compute-budget price instruction, for a default limit of
    /// `200_000*4 + 3_000*1 = 803,000` units. At that limit, 1,250,001
    /// micro-lamports a unit is `ceil(1,250,001 * 803,000 / 1e6) =
    /// 1,003,751` lamports, over the cap. Not counting the other
    /// instruction would price it at 3 non-compute-budget instructions
    /// (`200_000*3 + 3_000*1 = 603,000` units, fee `ceil(1,250,001 *
    /// 603,000 / 1e6) = 753,751`) and let it through.
    #[test]
    fn other_instructions_count_towards_the_fee() {
        let mut v: serde_json::Value =
            serde_json::from_str(&with_price(USDC_SOL, 1_250_001)).unwrap();
        v["otherInstructions"] = serde_json::Value::Array(vec![v["cleanupInstruction"].clone()]);
        let err = assemble(&v.to_string(), taker()).unwrap_err();
        assert!(
            matches!(err, RouteError::PriorityFeeExceeded(_)),
            "got {err}"
        );
    }

    #[test]
    fn a_compute_budget_instruction_of_the_wrong_length_is_refused_not_misread() {
        let short_price = [3u8, 1, 2, 3];
        let short_limit = [2u8, 1];
        // One byte too long each: the extra byte must not be ignored.
        let long_price = [3u8, 1, 0, 0, 0, 0, 0, 0, 0, 0];
        let long_limit = [2u8, 1, 0, 0, 0, 0];
        for data in [&short_price[..], &short_limit, &long_price, &long_limit] {
            let ixs = vec![RawInstruction {
                program_id: super::COMPUTE_BUDGET_PROGRAM.to_owned(),
                accounts: Vec::new(),
                data: b64::encode(data),
            }];
            let err = check_priority_fee(&ixs, 1).expect_err("a wrong length must refuse");
            assert!(
                matches!(err, RouteError::PriorityFeeExceeded(_)),
                "{data:?}: got {err}"
            );
        }
    }

    #[test]
    fn a_price_instruction_from_another_program_is_refused() {
        let mut ix = compute_unit_price(1);
        ix.program_id = "11111111111111111111111111111111".to_owned();
        let err = check_priority_fee(&[ix], 1).expect_err("another program must refuse");
        assert!(
            matches!(err, RouteError::PriorityFeeExceeded(ref m) if m.contains("not the Compute Budget program")),
            "got {err}"
        );
    }

    #[test]
    fn an_undecodable_compute_budget_instruction_is_refused_not_passed_through() {
        let ixs = vec![RawInstruction {
            program_id: "ComputeBudget111111111111111111111111111111".to_owned(),
            accounts: Vec::new(),
            data: b64::encode(&[7u8, 1, 2, 3]),
        }];
        let err = check_priority_fee(&ixs, 6)
            .expect_err("an undecodable compute-budget instruction must refuse, not pass through");
        assert!(
            matches!(err, RouteError::PriorityFeeExceeded(_)),
            "got {err}"
        );
    }

    /// Agave refuses a transaction carrying two `SetComputeUnitPrice`
    /// instructions (`DuplicateInstruction`); a visitor should never be
    /// handed one to sign that can never land.
    #[test]
    fn a_duplicate_price_instruction_is_refused() {
        let ixs = vec![compute_unit_price(1), compute_unit_price(1)];
        let err = check_priority_fee(&ixs, 1).expect_err("a duplicate price must refuse");
        assert!(
            matches!(err, RouteError::PriorityFeeExceeded(ref m) if m.contains("duplicate")),
            "got {err}"
        );
    }

    /// Same rule for `SetComputeUnitLimit`, with two different limits so the
    /// refusal is clearly about the repeat and not about the values agreeing.
    #[test]
    fn a_duplicate_limit_instruction_is_refused() {
        let ixs = vec![compute_unit_limit(100_000), compute_unit_limit(200_000)];
        let err = check_priority_fee(&ixs, 1).expect_err("a duplicate limit must refuse");
        assert!(
            matches!(err, RouteError::PriorityFeeExceeded(ref m) if m.contains("duplicate")),
            "got {err}"
        );
    }
}
