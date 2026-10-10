// SPDX-License-Identifier: Apache-2.0
//! Output guarantees are separate from outgoing spend and token-debit bounds.

use radar_risk::Authorization;

use crate::{Message, Rejection};

pub(crate) fn check(message: &Message, authorization: &Authorization) -> Vec<Rejection> {
    let Some(required) = authorization.min_output_raw else {
        // Legacy library/local authority has no output promise. The protected
        // issuer requires an explicit positive floor for known curve trades.
        return Vec::new();
    };
    if required == 0 {
        return vec![Rejection::InvalidOutputFloor];
    }
    let mut total = Some(0_u64);
    let mut rejections = Vec::new();
    for instruction in &message.instructions {
        if instruction.program_id != *radar_decode::pumpfun::PROGRAM_ID.as_bytes() {
            continue;
        }
        let Some(known) = radar_decode::decode(radar_decode::Program::PumpFun, &instruction.data)
            .known()
            .copied()
            .and_then(radar_decode::Instruction::pumpfun)
        else {
            continue;
        };
        let Some(Ok(trade)) = radar_decode::pumpfun::trade_args(known, &instruction.data) else {
            // The size verifier independently refuses unreadable known trades.
            continue;
        };
        if !supported_arguments(known, &instruction.data) {
            rejections.push(Rejection::UnreadableOutputGuarantee(
                known.anchor_name().to_owned(),
            ));
            continue;
        }
        let output = if known.is_buy() {
            trade.exact.tokens().or_else(|| trade.limit.tokens())
        } else {
            trade.limit.lamports()
        };
        total = total.and_then(|prior| prior.checked_add(output?));
    }
    rejections.extend(match total {
        None => vec![Rejection::OutputFloorOverflow],
        Some(guaranteed) if guaranteed < required => vec![Rejection::OutputFloorNotMet {
            guaranteed,
            required,
        }],
        _ => Vec::new(),
    });
    rejections
}

fn supported_arguments(known: radar_decode::pumpfun::Instruction, data: &[u8]) -> bool {
    use radar_decode::pumpfun::Instruction;
    // Current interfaces add optional partial-fill fields. Their effect on a
    // minimum-output promise is not assumed. Admit captured quantity layouts,
    // plus only the older track-volume bool; future suffixes require review.
    data.len() == 24
        || (matches!(known, Instruction::Buy | Instruction::BuyExactSolIn)
            && data.len() == 25
            && data[24] <= 1)
}
