// src/routes/markets.rs
use axum::{extract::{Path, State}, Json};
use rust_decimal_macros::dec;
use crate::{engine::AppState, engine::orderbook::get_snapshot, error::{AppError, Result}, types::Market};

// GET /markets
pub async fn get_markets(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    let st = state.engine_state.read().await;

    let markets: Vec<serde_json::Value> = Market::all().iter().map(|&market| {
        let ob = st.orderbooks.get(&market).unwrap();
        let price = state.prices.get(market.as_str()).map(|p| *p).unwrap_or(dec!(0));

        let candles = state.candles.get(market.as_str())
            .map(|c| c.clone()).unwrap_or_default();
        let last_24 = candles.iter().rev().take(24);
        let h24_high = last_24.clone().map(|c| c.high).fold(dec!(0), |a, b| a.max(b));
        let h24_low  = last_24.clone().map(|c| c.low).fold(price, |a, b| if b > dec!(0) { a.min(b) } else { a });
        let h24_change = if candles.len() >= 2 {
            let first = candles[candles.len().saturating_sub(24)].open;
            if first > dec!(0) { ((price - first) / first) * dec!(100) } else { dec!(0) }
        } else { dec!(0) };

        serde_json::json!({
            "market": market.as_str(),
            "price": price,
            "index_price": ob.index_price,
            "last_traded_price": ob.last_traded_price,
            "funding_rate": ob.funding_rate,
            "h24_high": h24_high,
            "h24_low": h24_low,
            "h24_change": h24_change.round_dp(2),
            "best_bid": ob.bids.keys().next_back().copied(),
            "best_ask": ob.asks.keys().next().copied(),
        })
    }).collect();

    Ok(Json(serde_json::json!({ "markets": markets })))
}

// GET /markets/:market
pub async fn get_market(
    State(state): State<AppState>,
    Path(market_str): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let market = Market::from_str(&market_str)
        .ok_or_else(|| AppError::BadRequest("Invalid market".into()))?;

    let st = state.engine_state.read().await;
    let ob = st.orderbooks.get(&market).unwrap();
    let snapshot = get_snapshot(ob, market);
    let price = state.prices.get(market.as_str()).map(|p| *p).unwrap_or(dec!(0));

    Ok(Json(serde_json::json!({ "market": market.as_str(), "price": price, "orderbook": snapshot })))
}

// GET /markets/:market/candles
pub async fn get_candles(
    State(state): State<AppState>,
    Path(market_str): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let market = market_str.to_uppercase();
    let candles = state.candles.get(&market)
        .map(|c| c.clone()).unwrap_or_default();

    Ok(Json(serde_json::json!({ "market": market, "candles": candles })))
}

// GET /markets/:market/trades
pub async fn get_trades(
    State(state): State<AppState>,
    Path(market_str): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let market = market_str.to_uppercase();
    let rows = sqlx::query!(
        "SELECT fill_id, market, price, qty, created_at FROM fills WHERE market=$1 ORDER BY created_at DESC LIMIT 50",
        market
    ).fetch_all(&state.db).await?
    .into_iter().map(|r| serde_json::json!({
        "fill_id": r.fill_id,
        "market": r.market,
        "price": r.price,
        "qty": r.qty,
        "created_at": r.created_at,
    })).collect::<Vec<_>>();

    Ok(Json(serde_json::json!({ "trades": rows })))
}