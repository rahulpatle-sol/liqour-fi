// src/routes/positions.rs
use axum::{extract::{Path, Query, State}, Json};
use rust_decimal_macros::dec;
use serde::Deserialize;

use crate::{
    engine::AppState,
    error::Result,
    middleware::auth::AuthUser,
    types::{Market, PositionWithPnl, Side},
};

// GET /positions
pub async fn get_positions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>> {
    let st = state.engine_state.read().await;
    let raw = st.get_positions(auth.user_id);
    let balance = st.balances.get(&auth.user_id).cloned();
    drop(st);

    let positions_with_pnl: Vec<PositionWithPnl> = raw.into_iter().map(|pos| {
        let current_price = state.prices
            .get(pos.market.as_str())
            .map(|p| *p)
            .unwrap_or(dec!(0));

        let pnl = if current_price > dec!(0) {
            let diff = match pos.side {
                Side::Long  => current_price - pos.entry_price,
                Side::Short => pos.entry_price - current_price,
            };
            diff * pos.qty
        } else {
            dec!(0)
        };

        PositionWithPnl { position: pos, unrealized_pnl: pnl, current_price }
    }).collect();

    Ok(Json(serde_json::json!({
        "positions": positions_with_pnl,
        "balance": balance,
    })))
}

// GET /positions/history
pub async fn get_history(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>> {
    let fills = sqlx::query!(
        r#"SELECT f.*, u1.username as maker_username, u2.username as taker_username
           FROM fills f
           LEFT JOIN users u1 ON u1.user_id = f.maker_user_id
           LEFT JOIN users u2 ON u2.user_id = f.taker_user_id
           WHERE f.maker_user_id = $1 OR f.taker_user_id = $1
           ORDER BY f.created_at DESC LIMIT 50"#,
        auth.user_id
    ).fetch_all(&state.db).await?;

    Ok(Json(serde_json::json!({ "history": fills })))
}

// GET /positions/candles/:market
#[derive(Deserialize)]
pub struct CandlesQuery { pub limit: Option<usize> }

pub async fn get_candles(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(market): Path<String>,
    Query(q): Query<CandlesQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = q.limit.unwrap_or(100).min(500);
    let key = market.to_uppercase();
    let candles = state.candles.get(&key)
        .map(|c| c.iter().rev().take(limit).rev().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    Ok(Json(serde_json::json!({ "market": key, "candles": candles })))
}
