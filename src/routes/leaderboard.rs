// src/routes/leaderboard.rs
use axum::{extract::{Path, Query, State}, Json};
use rust_decimal_macros::dec;
use serde::Deserialize;
use uuid::Uuid;

use crate::{engine::AppState, error::Result, types::{Market, PositionWithPnl, Side}};

#[derive(Deserialize)]
pub struct LeaderboardQuery {
    pub sort: Option<String>,
    pub limit: Option<i64>,
}

// GET /leaderboard
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Query(q): Query<LeaderboardQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = q.limit.unwrap_or(20).min(50);
    let sort = q.sort.as_deref().unwrap_or("pnl");

    let order_by = match sort {
        "volume"    => "ts.total_volume DESC",
        "winrate"   => "CASE WHEN ts.total_trades > 0 THEN ts.win_count::float/ts.total_trades ELSE 0 END DESC",
        "followers" => "ts.follower_count DESC",
        _           => "ts.total_pnl DESC",
    };

    let sql = format!(
        r#"SELECT u.user_id, u.wallet_address, u.username,
            ts.total_pnl, ts.win_count, ts.loss_count,
            ts.total_trades, ts.total_volume, ts.follower_count,
            CASE WHEN ts.total_trades > 0
              THEN ROUND((ts.win_count::float / ts.total_trades * 100)::numeric, 1)
              ELSE 0 END as win_rate
           FROM trader_stats ts
           JOIN users u ON u.user_id = ts.user_id
           WHERE ts.total_trades > 0
           ORDER BY {order_by}
           LIMIT $1"#
    );

    let rows = sqlx::query(&sql)
        .bind(limit)
        .fetch_all(&state.db)
        .await?;

    let engine_st = state.engine_state.read().await;

    let traders: Vec<serde_json::Value> = rows.iter().map(|row| {
        use sqlx::Row;
        let user_id: Uuid = row.get("user_id");
        let positions_with_pnl: Vec<PositionWithPnl> = engine_st
            .get_positions(user_id)
            .into_iter()
            .map(|pos| {
                let current_price = state.prices
                    .get(pos.market.as_str())
                    .map(|p| *p)
                    .unwrap_or(dec!(0));
                let diff = match pos.side {
                    Side::Long  => current_price - pos.entry_price,
                    Side::Short => pos.entry_price - current_price,
                };
                PositionWithPnl {
                    unrealized_pnl: diff * pos.qty,
                    current_price,
                    position: pos,
                }
            }).collect();

        serde_json::json!({
            "user_id":        user_id,
            "wallet_address": row.get::<String, _>("wallet_address"),
            "username":       row.get::<Option<String>, _>("username"),
            "total_pnl":      row.get::<rust_decimal::Decimal, _>("total_pnl"),
            "win_count":      row.get::<i64, _>("win_count"),
            "total_trades":   row.get::<i64, _>("total_trades"),
            "total_volume":   row.get::<rust_decimal::Decimal, _>("total_volume"),
            "follower_count": row.get::<i64, _>("follower_count"),
            "win_rate":       row.get::<rust_decimal::Decimal, _>("win_rate"),
            "open_positions": positions_with_pnl,
        })
    }).collect();

    Ok(Json(serde_json::json!({ "traders": traders, "sort": sort })))
}

// GET /leaderboard/:user_id
pub async fn get_trader(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let trader = sqlx::query!(
        r#"SELECT u.user_id, u.wallet_address, u.username,
            ts.total_pnl, ts.win_count, ts.loss_count, ts.total_trades,
            ts.total_volume, ts.follower_count,
            CASE WHEN ts.total_trades > 0
              THEN ROUND((ts.win_count::float/ts.total_trades*100)::numeric,1)
              ELSE 0 END as win_rate
           FROM users u
           LEFT JOIN trader_stats ts ON ts.user_id = u.user_id
           WHERE u.user_id = $1"#,
        user_id
    ).fetch_optional(&state.db).await?;

    let trader = trader.ok_or_else(|| crate::error::AppError::NotFound("Trader not found".into()))?;

    let fills = sqlx::query!(
        "SELECT fill_id, maker_user_id, taker_user_id, market, price, qty, pnl, created_at FROM fills WHERE maker_user_id=$1 OR taker_user_id=$1 ORDER BY created_at DESC LIMIT 20",
        user_id
    ).fetch_all(&state.db).await?
    .into_iter().map(|r| serde_json::json!({
        "fill_id": r.fill_id,
        "maker_user_id": r.maker_user_id,
        "taker_user_id": r.taker_user_id,
        "market": r.market,
        "price": r.price,
        "qty": r.qty,
        "pnl": r.pnl,
        "created_at": r.created_at,
    })).collect::<Vec<_>>();

    let st = state.engine_state.read().await;
    let positions_with_pnl: Vec<PositionWithPnl> = st.get_positions(user_id).into_iter().map(|pos| {
        let current_price = state.prices.get(pos.market.as_str()).map(|p| *p).unwrap_or(dec!(0));
        let diff = match pos.side {
            Side::Long  => current_price - pos.entry_price,
            Side::Short => pos.entry_price - current_price,
        };
        PositionWithPnl { unrealized_pnl: diff * pos.qty, current_price, position: pos }
    }).collect();

    Ok(Json(serde_json::json!({
        "trader": {
            "user_id": trader.user_id,
            "wallet_address": trader.wallet_address,
            "username": trader.username,
            "total_pnl": trader.total_pnl,
            "win_count": trader.win_count,
            "total_trades": trader.total_trades,
            "total_volume": trader.total_volume,
            "follower_count": trader.follower_count,
            "win_rate": trader.win_rate,
            "open_positions": positions_with_pnl,
        },
        "recent_fills": fills,
    })))
}