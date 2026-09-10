// SPDX-License-Identifier: Apache-2.0
//! `radar route` — ask Jupiter what a swap would get, and describe the route.
//!
//! # Why this exists
//!
//! Everything from the decision back is exercised hourly. Everything from the
//! quote forward has barely run at all. `radar-exec` had no production caller
//! until 2026-09-01, and its pipeline traits were satisfied only by test stubs.
//!
//! That is the shape [LEARNINGS](../../../LEARNINGS.md) 10 records. A live run
//! over 41,254 candidates raised zero proposals because a hardcoded exit-probe
//! size made a proposal arithmetically impossible — every stage had passed its
//! tests against fixtures no real candidate resembled. The lesson was not "test
//! more", it was "run the thing against reality before trusting it".
//!
//! So this command runs exactly the untested part and stops.
//!
//! # What it does now, and what it used to do
//!
//! It used to print an unsigned transaction. It cannot, because as of
//! 2026-09-09 Jupiter's Router returns **raw instructions and address lookup
//! tables** and no transaction, and the deprecated endpoint that returned a
//! legacy one is gone. So this prints a **price and a route**: what would be
//! received, the floor, the impact, the venues, and whether the signer could
//! ever read a transaction assembled from it. See `radar_exec::route`.
//!
//! # What it cannot do
//!
//! **It cannot sign and it cannot send.** There is no signer here, no key for
//! the taker, no RPC endpoint, and no `--submit` flag to discover later. A
//! diagnostic that could be talked into moving money is not a diagnostic.
//!
//! It also issues no `Authorization` and consults no policy, because it produces
//! nothing anybody could act on.

use radar_exec::route::{API_KEY_VAR, BUILD_API, Credentials, QuoteRequest, Router};
use radar_types::{Address, Asset};

/// Reads an asset from an operator's argument.
///
/// `SOL` is the native balance and `USDC` the stablecoin; anything else is a
/// mint address, admitted as classic SPL. **That last part is an assumption
/// this command cannot check**: telling SPL from Token-2022 needs the mint
/// account off the chain, and this command reads no chain. A Token-2022 mint
/// named here will be quoted — Jupiter routes it either way — but the asset
/// printed beside the number will say `Spl`, and Token-2022's transfer fees and
/// hooks mean the amount received need not be the amount quoted. `radar-sim`'s
/// `MintStructure` is where that gets established, and it is not wired here.
fn parse_asset(raw: &str) -> Result<Asset, String> {
    match raw.to_ascii_uppercase().as_str() {
        "SOL" => Ok(Asset::Sol),
        "WSOL" => Ok(Asset::WrappedSol),
        "USDC" => Ok(Asset::Usdc),
        _ => raw
            .parse::<Address>()
            .map(Asset::spl)
            .map_err(|_| format!("{raw} is neither SOL, WSOL, USDC nor a base58 mint")),
    }
}

/// What to quote, from either spelling of the arguments.
///
/// Two spellings because `--mint/--wallet/--lamports` is what `radar --help`
/// documents and what anything already scripted passes. It says one thing —
/// SOL in, that mint out — which is precisely the limitation the general form
/// exists to lift, so it is kept as a translation into the general form rather
/// than as a second path through the command.
fn requested(args: &[String]) -> Result<QuoteRequest, String> {
    let flag = |name: &str| crate::flag(args, name);

    if let Some(mint) = flag("--mint") {
        let taker: Address = flag("--wallet")
            .ok_or("--wallet is required alongside --mint")?
            .parse()
            .map_err(|_| "--wallet is not a base58 address".to_owned())?;
        let lamports: u64 = flag("--lamports")
            .ok_or("--lamports is required alongside --mint")?
            .parse()
            .map_err(|_| "--lamports is not a number".to_owned())?;
        return Ok(QuoteRequest::new(
            Asset::Sol,
            parse_asset(&mint)?,
            lamports,
            taker,
        ));
    }

    let input = parse_asset(&flag("--input").ok_or(
        "--input is required (SOL, USDC or a mint), or use --mint/--wallet/--lamports for a buy",
    )?)?;
    let output =
        parse_asset(&flag("--output").ok_or("--output is required (SOL, USDC or a mint)")?)?;
    let amount: u64 = flag("--amount")
        .ok_or("--amount is required, in the input asset's base units")?
        .parse()
        .map_err(|_| "--amount is not a number".to_owned())?;
    let taker: Address = flag("--taker")
        .ok_or("--taker is required: Jupiter selects setup instructions for an account")?
        .parse()
        .map_err(|_| "--taker is not a base58 address".to_owned())?;
    Ok(QuoteRequest::new(input, output, amount, taker))
}

/// Runs the command.
///
/// # Errors
///
/// A message for the operator. Every failure here is informative rather than
/// fatal to anything: nothing has been signed or sent.
pub fn run(args: &[String]) -> Result<(), String> {
    let flag = |name: &str| crate::flag(args, name);

    let request = requested(args)?;
    let (input, output, amount, taker) =
        (request.input, request.output, request.amount, request.taker);

    // Deny by default. Jupiter answers keyless requests at a lower rate limit,
    // so falling back would *work* -- quietly, throttled, and inside whatever
    // decision the quote fed. AGENTS rule 8.
    let credentials = Credentials::from_vars(|k| std::env::var(k).ok()).ok_or_else(|| {
        format!(
            "no Jupiter API key. Set {API_KEY_VAR} and run again.\n\
             This command will not fall back to Jupiter's keyless tier: an \
             unauthenticated quote succeeds, at a lower rate limit, and nothing \
             downstream would ever see the difference."
        )
    })?;

    let endpoint = flag("--endpoint").unwrap_or_else(|| BUILD_API.to_owned());
    let router = match flag("--slippage-bps") {
        Some(raw) => Router::with_endpoint(&endpoint, credentials).with_slippage_bps(
            raw.parse()
                .map_err(|_| "--slippage-bps is not a number".to_owned())?,
        ),
        None => Router::with_endpoint(&endpoint, credentials),
    };

    println!("quoting {amount} base units of {input:?} into {output:?}");
    println!("  taker      : {taker} (no key for it is held, and none is needed)");

    let quote = router
        .quote(&request)
        .map_err(|e| format!("no quote: {e}"))?;

    println!("  expected   : {} base units out", quote.out_amount);
    match quote.worst_out {
        Some(floor) => println!("  floor      : {floor} base units at the configured slippage"),
        None => println!("  floor      : NOT STATED by Jupiter — not zero, unknown"),
    }
    if quote.impact_bps == u32::MAX {
        println!("  impact     : NOT STATED — priced as the worst case, never as 0");
    } else {
        println!("  impact     : {} bps", quote.impact_bps);
    }
    println!(
        "  mode       : {}",
        quote.swap_mode.as_deref().unwrap_or("not stated")
    );

    // Labels, not support. Radar decodes pump.fun and nothing else; reaching a
    // venue through an aggregator is not the same as understanding it.
    println!("  venues     : {}", quote.venues.join(" -> "));
    println!("               (labels from Jupiter. Radar decodes pump.fun only.)");

    // The check the signer's decoder depends on, moved from the transaction to
    // the route. A transaction naming accounts through a lookup table is one
    // the signer cannot read (ADR 0003) -- and finding that out here, with
    // nothing at stake, is the entire point of this command.
    if quote.signer_could_read() {
        println!("  lookup tbl : none — a transaction built from this route could be read");
    } else {
        println!(
            "  lookup tbl : {} — the signer could NOT read a transaction built from this route",
            quote.lookup_tables
        );
    }

    println!();
    println!("This is a price, not a transaction. Jupiter's Router returns raw");
    println!("instructions; nothing here assembles, signs or sends one, and this");
    println!("command has no key for the taker and no RPC endpoint.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_asset;
    use radar_types::Asset;

    #[test]
    fn the_named_assets_parse_to_the_variants_that_account_for_them() {
        // Not case-sensitive, because an operator types `sol`. But `SOL` and
        // `WSOL` must stay apart: they are different balances, and folding them
        // would make a quote describe a transfer that cannot happen.
        assert_eq!(parse_asset("SOL"), Ok(Asset::Sol));
        assert_eq!(parse_asset("sol"), Ok(Asset::Sol));
        assert_eq!(parse_asset("WSOL"), Ok(Asset::WrappedSol));
        assert_eq!(parse_asset("usdc"), Ok(Asset::Usdc));
        assert_ne!(parse_asset("SOL"), parse_asset("WSOL"));
    }

    #[test]
    fn the_named_mints_fold_into_their_named_variants() {
        // `Asset::spl` folds them, so an operator pasting the wrapped SOL mint
        // gets `WrappedSol` rather than a second, unequal spelling of it.
        assert_eq!(
            parse_asset("So11111111111111111111111111111111111111112"),
            Ok(Asset::WrappedSol)
        );
        assert_eq!(
            parse_asset("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
            Ok(Asset::Usdc)
        );
    }

    #[test]
    fn a_ticker_is_not_a_mint() {
        // Tickers are creator-controlled text and two mints may share one.
        // Anything that is not a known name must be a base58 address or a
        // refusal -- never a lookup.
        assert!(parse_asset("BONK").is_err());
        assert!(parse_asset("").is_err());
        assert!(parse_asset("not-base58!").is_err());
    }
}
