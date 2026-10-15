// SPDX-License-Identifier: Apache-2.0
//! Configured private collector evidence, not a model-selected price or path.

use super::{Config, Snapshot, expired, private_read, read_evidence_expiry};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    context_slot: u64,
    posted_slot: u64,
    published_at_unix_secs: u64,
    lower_micro_usd_per_sol: u64,
    upper_micro_usd_per_sol: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Packet {
    version: u8,
    authority: String,
    commitment: String,
    asset: String,
    quote: String,
    source: String,
    account: String,
    read_started_at_unix_secs: u64,
    read_completed_at_unix_secs: u64,
    observation: Observation,
    wallet_inventory_complete: bool,
    execution_guaranteed: bool,
}

pub(super) fn read(
    path: &Path,
    snapshot: &Snapshot,
    config: &Config,
    now: u64,
) -> Result<(u64, u64), String> {
    let packet: Packet = serde_json::from_slice(&private_read(path)?)
        .map_err(|_| "invalid protected SOL price evidence")?;
    let price = &packet.observation;
    if packet.version != 1
        || packet.authority != "read_only"
        || packet.commitment != "finalized"
        || packet.asset != "SOL"
        || packet.quote != "USD"
        || packet.source != "pyth_receiver_full"
        || packet.account != "7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE"
        || packet.wallet_inventory_complete
        || packet.execution_guaranteed
        || price.lower_micro_usd_per_sol == 0
        || price.upper_micro_usd_per_sol < price.lower_micro_usd_per_sol
        || price.context_slot < snapshot.proposal.oldest_input_slot.get()
        || price.context_slot > snapshot.state.now.get()
        || price.posted_slot > price.context_slot
        || price.published_at_unix_secs > packet.read_completed_at_unix_secs
    {
        return Err("protected SOL price identity, bounds or context refused".into());
    }
    let read_expiry = read_evidence_expiry(
        &serde_json::json!({
            "read_started_at_unix_secs":packet.read_started_at_unix_secs,
            "read_completed_at_unix_secs":packet.read_completed_at_unix_secs,
        }),
        config,
        now,
    )?;
    let expiry = price
        .published_at_unix_secs
        .checked_add(config.max_snapshot_age_secs)
        .ok_or("protected SOL price expiry overflow")?
        .min(read_expiry);
    if expired(now, expiry) {
        return Err("protected SOL price has no remaining validity".into());
    }
    Ok((price.upper_micro_usd_per_sol, expiry))
}
