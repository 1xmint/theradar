// SPDX-License-Identifier: Apache-2.0
//! Bind known curve instructions to the authorized mint and signing trader.
//! The account positions are captured from accepted mainnet transactions;
//! v2 is a different layout, not an extension of the older account order.

use radar_decode::pumpfun::Instruction;
use radar_risk::Authorization;
use radar_types::{Address, Asset};

use crate::{Message, Rejection};

pub(crate) fn check(
    message: &Message,
    authorization: &Authorization,
    wallet: &Address,
) -> Vec<Rejection> {
    let mut rejections = Vec::new();
    for ix in &message.instructions {
        if ix.program_id != *radar_decode::pumpfun::PROGRAM_ID.as_bytes() {
            continue;
        }
        let Some(known) = radar_decode::decode(radar_decode::Program::PumpFun, &ix.data)
            .known()
            .copied()
            .and_then(radar_decode::Instruction::pumpfun)
        else {
            // The existing size verifier refuses unknown/truncated data.
            continue;
        };
        let (mint, trader, quote) = match known {
            Instruction::Buy | Instruction::BuyExactSolIn | Instruction::Sell => (2, 6, None),
            Instruction::BuyV2 | Instruction::BuyExactQuoteInV2 | Instruction::SellV2 => {
                (1, 13, Some(2))
            }
            _ => continue,
        };
        let mut require = |valid: bool, role: &str| {
            if !valid {
                rejections.push(Rejection::TradeAccountMismatch {
                    instruction: known.anchor_name().to_owned(),
                    role: role.to_owned(),
                });
            }
        };
        require(
            ix.accounts.get(mint) == Some(authorization.mint.as_bytes()),
            "mint",
        );
        require(
            ix.accounts.get(trader) == Some(wallet.as_bytes())
                && message
                    .accounts
                    .iter()
                    .take(usize::from(message.required_signatures))
                    .any(|key| key == wallet.as_bytes()),
            "signing wallet",
        );
        if let Some(quote) = quote {
            // V2 names its quote mint explicitly. Its quantity must not be read
            // as lamports for USDC or an arbitrary token. Other quote assets
            // need typed authority and their own independent spend verification.
            require(
                ix.accounts.get(quote) == Some(Asset::WRAPPED_SOL_MINT.as_bytes()),
                "native quote mint",
            );
        }
    }
    rejections
}
