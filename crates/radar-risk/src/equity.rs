// SPDX-License-Identifier: Apache-2.0
//! Pure long-only paper equity decisions. Actual caller: the CLI paper preflight.
//! Inputs are protected caller facts, never model-provided account evidence.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Direction in the cash-funded, long-only equity lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Purchase with reconciled cash.
    Buy,
    /// Reduce owned, unreserved shares.
    Sell,
}

/// An inert simple day limit order; extended hours, margin and shorts are absent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    /// Pinned paper account identity.
    pub account_id: String,
    /// Durable client identity, never regenerated on retry.
    pub client_order_id: String,
    /// US equity ticker.
    pub symbol: String,
    /// Buy or owned-share reduction.
    pub side: Side,
    /// Billionths of one share.
    pub quantity_nanoshares: u64,
    /// Worst permitted price, micro-USD per share.
    pub limit_microusd_per_share: u64,
    /// Reserved fee ceiling in micro-USD.
    pub fee_bound_microusd: u64,
}

/// Effective owner policy. Missing configuration resolves to the closed default.
/// Website/manual/adaptive choices must resolve to these bounds before evaluation.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// Explicit revocable paper handover.
    pub enabled: bool,
    /// Account to which handover applies.
    pub account_id: String,
    /// Owner-permitted tickers.
    pub symbols: BTreeSet<String>,
    /// Maximum total committed exposure, micro-USD.
    pub max_capital_microusd: u64,
    /// Maximum one-order limit notional, micro-USD.
    pub max_order_microusd: u64,
    /// Maximum one-symbol committed exposure, micro-USD.
    pub max_position_microusd: u64,
    /// Optional entry stop; absent loss measurement refuses when enabled.
    pub max_daily_loss_microusd: Option<u64>,
    /// Maximum age of the oldest input in Unix milliseconds.
    pub max_input_age_ms: u64,
    /// Maximum lifetime of an in-process permission, milliseconds.
    pub authorization_lifetime_ms: u64,
    /// Maximum bid/ask spread relative to bid, basis points.
    pub max_spread_bps: u64,
    /// Maximum adverse limit distance from the executable quote, basis points.
    pub max_limit_distance_bps: u64,
    /// Maximum measured conservative round-trip cost relative to limit notional.
    pub max_round_trip_cost_bps: u64,
}

/// Selected, reconciled facts supplied by the private provider caller.
#[derive(Clone, Debug)]
pub struct State {
    /// Broker identity to which the selected facts belong.
    pub account_id: String,
    /// Decision watermark, Unix milliseconds; no Solana slots.
    pub now_unix_ms: u64,
    /// Oldest host or validated exchange input, Unix milliseconds.
    pub oldest_input_unix_ms: u64,
    /// Ceiled host completion of all selected reads, Unix milliseconds.
    pub available_at_unix_ms: u64,
    /// Pause/revoke state supplied outside model reasoning.
    pub halted: bool,
    /// Broker regular-hours session is known open.
    pub market_open: bool,
    /// Validated broker close; permissions expire strictly before this boundary.
    pub session_close_unix_ms: Option<u64>,
    /// Cash after all local liabilities, independently bounded by broker funds.
    pub available_cash_microusd: Option<u64>,
    /// Reconciled shares after all pending local sell claims.
    pub available_nanoshares: Option<u64>,
    /// Existing positions and retained cash commitments, micro-USD.
    pub committed_microusd: Option<u64>,
    /// Existing symbol position and retained commitments, micro-USD.
    pub symbol_committed_microusd: Option<u64>,
    /// Realised loss for the owner's selected day, micro-USD; unknown is absent.
    pub daily_loss_microusd: Option<u64>,
    /// Validated bid, micro-USD per share.
    pub bid_microusd_per_share: u64,
    /// Validated ask, micro-USD per share.
    pub ask_microusd_per_share: u64,
    /// Verified executable/exit capacity in shares, not ambiguous feed size units.
    pub capacity_nanoshares: Option<u64>,
    /// Independently established required fee reserve, micro-USD.
    pub required_fee_microusd: Option<u64>,
    /// Conservative round-trip cost, micro-USD; never model confidence.
    pub round_trip_cost_microusd: Option<u64>,
}

/// Reproducible first refusal, independent of ambient state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// Handover is closed, paused or invalid.
    Closed,
    /// Identity, ticker, quantity or price is invalid.
    InvalidOrder,
    /// Inputs are unavailable, future-dated, expired or overflow time bounds.
    Time,
    /// Regular-hours session is closed.
    Session,
    /// Quote, limit distance or capacity is unsupported.
    Liquidity,
    /// Costs/fee adequacy are unknown or excessive.
    Costs,
    /// Cash or owned shares are unknown or insufficient.
    Funds,
    /// An order, symbol or portfolio ceiling would be exceeded.
    Limit,
    /// Optional loss stop is reached or its required measurement is unknown.
    DailyLoss,
}

/// Opaque in-process permission. It cannot be deserialized from a model/file.
/// A separate executor still rechecks payload, time, revocation and durable reuse.
#[derive(Debug)]
pub struct Authorization {
    order: Order,
    issued_at: u64,
    valid_until: u64,
}

impl Authorization {
    /// Check the exact immutable payload and inclusive validity interval.
    #[must_use]
    pub fn permits(&self, order: &Order, now_unix_ms: u64) -> bool {
        self.order == *order && now_unix_ms >= self.issued_at && now_unix_ms <= self.valid_until
    }
}

fn notional(order: &Order) -> Result<u64, Refusal> {
    u64::try_from(
        (u128::from(order.quantity_nanoshares) * u128::from(order.limit_microusd_per_share))
            .div_ceil(1_000_000_000),
    )
    .map_err(|_| Refusal::InvalidOrder)
}

/// Judge an inert proposal against protected caller facts and effective policy.
///
/// # Errors
/// A deterministic refusal; no partial authority escapes.
pub fn evaluate(order: &Order, state: &State, policy: &Policy) -> Result<Authorization, Refusal> {
    if !policy.enabled || state.halted || policy.authorization_lifetime_ms == 0 {
        return Err(Refusal::Closed);
    }
    if order.account_id.trim().is_empty()
        || order.account_id != policy.account_id
        || order.account_id != state.account_id
        || order.client_order_id.is_empty()
        || !order
            .client_order_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !policy.symbols.contains(&order.symbol)
        || order.quantity_nanoshares == 0
        || order.limit_microusd_per_share == 0
    {
        return Err(Refusal::InvalidOrder);
    }
    let expiry = expires(state, policy)?;
    liquidity(order, state, policy)?;
    let value = notional(order)?;
    if value > policy.max_order_microusd {
        return Err(Refusal::Limit);
    }
    let fee = state.required_fee_microusd.ok_or(Refusal::Costs)?;
    let cost = state.round_trip_cost_microusd.ok_or(Refusal::Costs)?;
    if fee > order.fee_bound_microusd
        || cost < fee
        || u128::from(cost) * 10_000
            > u128::from(value) * u128::from(policy.max_round_trip_cost_bps)
    {
        return Err(Refusal::Costs);
    }
    let claim = match order.side {
        Side::Buy => value
            .checked_add(order.fee_bound_microusd)
            .ok_or(Refusal::Funds)?,
        Side::Sell => order.fee_bound_microusd,
    };
    allocation(order, state, policy, claim)?;
    Ok(Authorization {
        order: order.clone(),
        issued_at: state.now_unix_ms,
        valid_until: expiry,
    })
}

fn expires(state: &State, policy: &Policy) -> Result<u64, Refusal> {
    let input_expiry = state
        .oldest_input_unix_ms
        .checked_add(policy.max_input_age_ms)
        .ok_or(Refusal::Time)?;
    let expiry = state
        .now_unix_ms
        .checked_add(policy.authorization_lifetime_ms)
        .ok_or(Refusal::Time)?
        .min(input_expiry);
    if state.oldest_input_unix_ms > state.available_at_unix_ms
        || state.available_at_unix_ms > state.now_unix_ms
        || state.now_unix_ms > input_expiry
    {
        return Err(Refusal::Time);
    }
    if !state.market_open {
        return Err(Refusal::Session);
    }
    let close = state.session_close_unix_ms.ok_or(Refusal::Session)?;
    if state.now_unix_ms >= close {
        return Err(Refusal::Session);
    }
    Ok(expiry.min(close - 1))
}

fn liquidity(order: &Order, state: &State, policy: &Policy) -> Result<(), Refusal> {
    let bid = state.bid_microusd_per_share;
    let ask = state.ask_microusd_per_share;
    let spread = ask.checked_sub(bid).ok_or(Refusal::Liquidity)?;
    // With ask >= bid already established, a positive bid also proves ask > 0.
    if bid == 0
        || u128::from(spread) * 10_000 > u128::from(bid) * u128::from(policy.max_spread_bps)
        || state
            .capacity_nanoshares
            .is_none_or(|capacity| capacity < order.quantity_nanoshares)
    {
        return Err(Refusal::Liquidity);
    }
    let (distance, reference) = match order.side {
        Side::Buy => (order.limit_microusd_per_share.checked_sub(ask), ask),
        Side::Sell => (bid.checked_sub(order.limit_microusd_per_share), bid),
    };
    if distance.is_none_or(|distance| {
        u128::from(distance) * 10_000
            > u128::from(reference) * u128::from(policy.max_limit_distance_bps)
    }) {
        return Err(Refusal::Liquidity);
    }
    Ok(())
}

fn allocation(order: &Order, state: &State, policy: &Policy, claim: u64) -> Result<(), Refusal> {
    if state
        .available_cash_microusd
        .is_none_or(|cash| cash < claim)
    {
        return Err(Refusal::Funds);
    }
    match order.side {
        Side::Buy => {
            if state
                .committed_microusd
                .and_then(|held| held.checked_add(claim))
                .is_none_or(|after| after > policy.max_capital_microusd)
                || state
                    .symbol_committed_microusd
                    .and_then(|held| held.checked_add(claim))
                    .is_none_or(|after| after > policy.max_position_microusd)
            {
                return Err(Refusal::Limit);
            }
            if policy
                .max_daily_loss_microusd
                .is_some_and(|cap| state.daily_loss_microusd.is_none_or(|loss| loss >= cap))
            {
                return Err(Refusal::DailyLoss);
            }
        }
        Side::Sell => {
            if state
                .available_nanoshares
                .is_none_or(|held| held < order.quantity_nanoshares)
            {
                return Err(Refusal::Funds);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Order, State, Policy) {
        let order = Order {
            account_id: "paper".into(),
            client_order_id: "radar_1".into(),
            symbol: "SPY".into(),
            side: Side::Buy,
            quantity_nanoshares: 1_000_000_000,
            limit_microusd_per_share: 100_000_000,
            fee_bound_microusd: 1,
        };
        let state = State {
            account_id: "paper".into(),
            now_unix_ms: 100,
            oldest_input_unix_ms: 90,
            available_at_unix_ms: 95,
            halted: false,
            market_open: true,
            session_close_unix_ms: Some(1000),
            available_cash_microusd: Some(100_000_001),
            available_nanoshares: Some(1_000_000_000),
            committed_microusd: Some(0),
            symbol_committed_microusd: Some(0),
            daily_loss_microusd: None,
            bid_microusd_per_share: 100_000_000,
            ask_microusd_per_share: 100_000_000,
            capacity_nanoshares: Some(1_000_000_000),
            required_fee_microusd: Some(1),
            round_trip_cost_microusd: Some(10_000),
        };
        let policy = Policy {
            enabled: true,
            account_id: "paper".into(),
            symbols: ["SPY".into()].into(),
            max_capital_microusd: 100_000_001,
            max_order_microusd: 100_000_000,
            max_position_microusd: 100_000_001,
            max_daily_loss_microusd: None,
            max_input_age_ms: 10,
            authorization_lifetime_ms: 10,
            max_spread_bps: 0,
            max_limit_distance_bps: 0,
            max_round_trip_cost_bps: 1,
        };
        (order, state, policy)
    }

    #[test]
    fn session_close_caps_permission_and_refuses_unknown_or_elapsed_session() {
        let (order, mut state, policy) = fixture();
        state.oldest_input_unix_ms = 100;
        state.available_at_unix_ms = 100;
        state.session_close_unix_ms = Some(103);
        let permission = evaluate(&order, &state, &policy).unwrap();
        assert!(permission.permits(&order, 102));
        assert!(!permission.permits(&order, 103));
        for close in [None, Some(0), Some(99), Some(100)] {
            state.session_close_unix_ms = close;
            assert!(matches!(
                evaluate(&order, &state, &policy),
                Err(Refusal::Session)
            ));
        }
        state.session_close_unix_ms = Some(101);
        assert!(
            evaluate(&order, &state, &policy)
                .unwrap()
                .permits(&order, 100)
        );
    }

    #[test]
    fn exact_boundaries_authorize_only_the_bound_payload_and_valid_time() {
        let (order, mut state, mut policy) = fixture();
        let auth = evaluate(&order, &state, &policy).unwrap();
        assert!(auth.permits(&order, 100));
        assert!(!auth.permits(&order, 99));
        assert!(!auth.permits(&order, 101));
        let mut changed = order.clone();
        changed.fee_bound_microusd += 1;
        assert!(!auth.permits(&changed, 100));
        state.oldest_input_unix_ms = 95;
        let auth = evaluate(&order, &state, &policy).unwrap();
        assert!(auth.permits(&order, 105));
        assert!(!auth.permits(&order, 106));
        policy.authorization_lifetime_ms = 1;
        let auth = evaluate(&order, &state, &policy).unwrap();
        assert!(auth.permits(&order, 101));
        assert!(!auth.permits(&order, 102));
        assert!(evaluate(&order, &state, &Policy::default()).is_err());
        state.available_at_unix_ms = 100;
        assert!(evaluate(&order, &state, &policy).is_ok());
        state.oldest_input_unix_ms = 100;
        assert!(evaluate(&order, &state, &policy).is_ok());
    }

    #[test]
    fn every_missing_measurement_and_exceeded_boundary_refuses() {
        for case in 0..35 {
            let (mut order, mut state, mut policy) = fixture();
            let reason = if case < 17 {
                invalid_configuration(case, &mut order, &mut state, &mut policy)
            } else {
                invalid_measurement(case, &mut order, &mut state, &mut policy)
            };
            assert_eq!(
                evaluate(&order, &state, &policy).unwrap_err(),
                reason,
                "case {case}"
            );
        }
    }

    fn invalid_configuration(
        case: u8,
        order: &mut Order,
        state: &mut State,
        policy: &mut Policy,
    ) -> Refusal {
        match case {
            0 => {
                policy.enabled = false;
                Refusal::Closed
            }
            1 => {
                state.halted = true;
                Refusal::Closed
            }
            2 => {
                policy.authorization_lifetime_ms = 0;
                Refusal::Closed
            }
            3 => {
                order.account_id = " ".into();
                Refusal::InvalidOrder
            }
            4 => {
                policy.account_id = "other".into();
                Refusal::InvalidOrder
            }
            5 => {
                state.account_id = "other".into();
                Refusal::InvalidOrder
            }
            6 => {
                order.client_order_id = String::new();
                Refusal::InvalidOrder
            }
            7 => {
                order.client_order_id = "x&y".into();
                Refusal::InvalidOrder
            }
            8 => {
                policy.symbols.clear();
                Refusal::InvalidOrder
            }
            9 => {
                order.quantity_nanoshares = 0;
                Refusal::InvalidOrder
            }
            10 => {
                order.limit_microusd_per_share = 0;
                Refusal::InvalidOrder
            }
            11 => {
                state.oldest_input_unix_ms = 96;
                Refusal::Time
            }
            12 => {
                state.available_at_unix_ms = 101;
                Refusal::Time
            }
            13 => {
                state.now_unix_ms = 101;
                Refusal::Time
            }
            14 => {
                policy.max_input_age_ms = u64::MAX;
                Refusal::Time
            }
            15 => {
                policy.authorization_lifetime_ms = u64::MAX;
                Refusal::Time
            }
            16 => {
                state.market_open = false;
                Refusal::Session
            }
            _ => unreachable!(),
        }
    }
    fn invalid_measurement(
        case: u8,
        order: &mut Order,
        state: &mut State,
        policy: &mut Policy,
    ) -> Refusal {
        match case {
            17 => {
                state.bid_microusd_per_share = 0;
                Refusal::Liquidity
            }
            18 => {
                state.ask_microusd_per_share = 0;
                Refusal::Liquidity
            }
            19 => {
                state.ask_microusd_per_share = 99_999_999;
                Refusal::Liquidity
            }
            20 => {
                state.bid_microusd_per_share = 99_999_999;
                Refusal::Liquidity
            }
            21 => {
                state.capacity_nanoshares = None;
                Refusal::Liquidity
            }
            22 => {
                state.capacity_nanoshares = Some(999_999_999);
                Refusal::Liquidity
            }
            23 => {
                order.limit_microusd_per_share = 99_999_999;
                Refusal::Liquidity
            }
            24 => {
                order.limit_microusd_per_share = 100_000_001;
                Refusal::Liquidity
            }
            25 => {
                policy.max_order_microusd = 99_999_999;
                Refusal::Limit
            }
            26 => {
                state.required_fee_microusd = None;
                Refusal::Costs
            }
            27 => {
                state.required_fee_microusd = Some(2);
                Refusal::Costs
            }
            28 => {
                state.round_trip_cost_microusd = None;
                Refusal::Costs
            }
            29 => {
                state.round_trip_cost_microusd = Some(10_001);
                Refusal::Costs
            }
            30 => {
                state.available_cash_microusd = None;
                Refusal::Funds
            }
            31 => {
                state.available_cash_microusd = Some(100_000_000);
                Refusal::Funds
            }
            32 => {
                state.committed_microusd = None;
                Refusal::Limit
            }
            33 => {
                state.symbol_committed_microusd = None;
                Refusal::Limit
            }
            _ => {
                state.committed_microusd = Some(u64::MAX);
                Refusal::Limit
            }
        }
    }

    #[test]
    fn budgets_loss_stops_and_owned_share_reductions_are_independent() {
        let (mut order, mut state, mut policy) = fixture();
        for symbol in [false, true] {
            let mut limited = policy.clone();
            if symbol {
                limited.max_position_microusd -= 1;
            } else {
                limited.max_capital_microusd -= 1;
            }
            assert_eq!(
                evaluate(&order, &state, &limited).unwrap_err(),
                Refusal::Limit
            );
        }
        policy.max_daily_loss_microusd = Some(1);
        for loss in [None, Some(1), Some(2)] {
            state.daily_loss_microusd = loss;
            assert_eq!(
                evaluate(&order, &state, &policy).unwrap_err(),
                Refusal::DailyLoss
            );
        }
        state.daily_loss_microusd = Some(0);
        assert!(evaluate(&order, &state, &policy).is_ok());
        order.side = Side::Sell;
        state.available_cash_microusd = Some(1);
        state.committed_microusd = None;
        state.symbol_committed_microusd = None;
        state.daily_loss_microusd = None;
        policy.max_capital_microusd = 0;
        policy.max_position_microusd = 0;
        assert!(evaluate(&order, &state, &policy).is_ok());
        for shares in [None, Some(999_999_999)] {
            state.available_nanoshares = shares;
            assert_eq!(
                evaluate(&order, &state, &policy).unwrap_err(),
                Refusal::Funds
            );
        }
        state.available_nanoshares = Some(1_000_000_000);
        state.available_cash_microusd = Some(0);
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Funds
        );
    }

    #[test]
    fn fractional_notional_rounds_up_and_overflow_never_saturates_into_permission() {
        let (mut order, mut state, mut policy) = fixture();
        order.quantity_nanoshares = 1;
        order.limit_microusd_per_share = 1;
        assert_eq!(notional(&order).unwrap(), 1);
        order.quantity_nanoshares = 1_000_000_001;
        assert_eq!(notional(&order).unwrap(), 2);
        order.quantity_nanoshares = u64::MAX;
        order.limit_microusd_per_share = u64::MAX;
        assert_eq!(notional(&order), Err(Refusal::InvalidOrder));
        let (mut order, _, _) = fixture();
        order.fee_bound_microusd = u64::MAX;
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Funds
        );
        order.fee_bound_microusd = 1;
        state.symbol_committed_microusd = Some(u64::MAX);
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Limit
        );
        state.symbol_committed_microusd = Some(0);
        state.round_trip_cost_microusd = Some(1);
        policy.max_limit_distance_bps = 1;
        policy.max_order_microusd = u64::MAX;
        policy.max_capital_microusd = u64::MAX;
        policy.max_position_microusd = u64::MAX;
        state.available_cash_microusd = Some(u64::MAX);
        order.limit_microusd_per_share += 10_000;
        assert!(evaluate(&order, &state, &policy).is_ok());
        order.limit_microusd_per_share += 1;
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Liquidity
        );
        order.side = Side::Sell;
        order.limit_microusd_per_share = 99_990_000;
        assert!(evaluate(&order, &state, &policy).is_ok());
        order.limit_microusd_per_share -= 1;
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Liquidity
        );
        order.limit_microusd_per_share = 100_000_001;
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Liquidity
        );
    }

    #[test]
    fn cost_and_spread_ratios_hold_at_nonunit_basis_point_boundaries() {
        let (mut order, mut state, mut policy) = fixture();
        state.round_trip_cost_microusd = Some(0);
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Costs
        );
        policy.max_round_trip_cost_bps = 2;
        state.round_trip_cost_microusd = Some(20_000);
        assert!(evaluate(&order, &state, &policy).is_ok());
        state.round_trip_cost_microusd = Some(20_001);
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Costs
        );
        state.round_trip_cost_microusd = Some(1);
        policy.max_spread_bps = 100;
        state.ask_microusd_per_share = 101_000_000;
        order.limit_microusd_per_share = 101_000_000;
        policy.max_order_microusd = u64::MAX;
        policy.max_capital_microusd = u64::MAX;
        policy.max_position_microusd = u64::MAX;
        state.available_cash_microusd = Some(u64::MAX);
        assert!(evaluate(&order, &state, &policy).is_ok());
        state.ask_microusd_per_share += 1;
        order.limit_microusd_per_share += 1;
        assert_eq!(
            evaluate(&order, &state, &policy).unwrap_err(),
            Refusal::Liquidity
        );
    }
}
