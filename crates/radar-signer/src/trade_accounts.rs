// SPDX-License-Identifier: Apache-2.0
//! Bind known curve instructions to the mint, derived reserves and trader.
//! The account positions are captured from accepted mainnet transactions;
//! v2 is a different layout, not an extension of the older account order.

use radar_decode::pumpfun::Instruction;
use radar_risk::Authorization;
use radar_types::{Address, Asset};

use crate::{Message, Rejection};

static SPL_TOKEN: std::sync::LazyLock<Address> = std::sync::LazyLock::new(|| {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .expect("fixed token program")
});
static TOKEN_2022: std::sync::LazyLock<Address> = std::sync::LazyLock::new(|| {
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        .parse()
        .expect("fixed token program")
});

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
        let (mint, trader, quote, curve, curve_token, token_program) = match known {
            Instruction::Buy | Instruction::BuyExactSolIn => (2, 6, None, 3, 4, 8),
            Instruction::Sell => (2, 6, None, 3, 4, 9),
            Instruction::BuyV2 | Instruction::BuyExactQuoteInV2 | Instruction::SellV2 => {
                (1, 13, Some(2), 10, 11, 3)
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
        let expected_curve = radar_pda::find(
            &[b"bonding-curve", authorization.mint.as_bytes()],
            &radar_decode::pumpfun::PROGRAM_ID,
        );
        require(
            expected_curve
                .is_some_and(|(address, _)| ix.accounts.get(curve) == Some(address.as_bytes())),
            "derived bonding curve",
        );
        let token_program = ix
            .accounts
            .get(token_program)
            .map(|key| Address::new(*key))
            .filter(|program| program == &*SPL_TOKEN || program == &*TOKEN_2022);
        require(token_program.is_some(), "base token program");
        let expected_token = expected_curve.and_then(|(curve, _)| {
            radar_pda::associated_token_account(&curve, &authorization.mint, &token_program?)
        });
        require(
            expected_token
                .is_some_and(|address| ix.accounts.get(curve_token) == Some(address.as_bytes())),
            "derived curve token account",
        );
        require(
            ix.accounts.get(trader) == Some(wallet.as_bytes())
                && message
                    .accounts
                    .iter()
                    .position(|key| key == wallet.as_bytes())
                    .and_then(|index| message.privileges(index))
                    .is_some_and(|flags| flags.signer),
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
