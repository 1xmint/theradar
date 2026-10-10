// SPDX-License-Identifier: Apache-2.0
//! Protected reviewed floor, not a live quote or a model-controlled amount.

pub(super) fn review(bytes: &[u8], floor: Option<u64>) -> Result<Option<u64>, String> {
    let message = radar_signer::decode(bytes).map_err(|_| "transaction refused")?;
    let has_trade = message.instructions.iter().any(|instruction| {
        instruction.program_id == *radar_decode::pumpfun::PROGRAM_ID.as_bytes()
            && radar_decode::decode(radar_decode::Program::PumpFun, &instruction.data)
                .known()
                .copied()
                .and_then(radar_decode::Instruction::pumpfun)
                .is_some_and(|known| known.is_trade())
    });
    if has_trade && !floor.is_some_and(|raw| raw > 0) {
        return Err("curve trade requires a positive protected output floor".into());
    }
    Ok(floor)
}
