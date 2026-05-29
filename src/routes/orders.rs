// src/routes/orders.rs
use axum::{extract::{Path, Query, State}, Json};
use rust_decimal_macros::dec;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    engine::AppState,
    error::{AppError, Result},
    middleware::auth::AuthUser,
    types::*,
};

// POST /orders
pub async fn place_order(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PlaceOrderRequest>,
) -> Result<Json<serde_json::Value>> {

    let market = Market::from_str(&body.market)
        .ok_or_else(|| AppError::BadRequest("Invalid market. Use SOL, BTC, or ETH".into()))?;

    let side = match body.side.to_lowercase().as_str() {
        "long"  => Side::Long,
        "short" => Side::Short,
        _ => return Err(AppError::BadRequest("Side must be 'long' or 'short'".into())),
    };

    let order_type = match body.order_type.to_lowercase().as_str() {
        "limit"  => OrderType::Limit,
        "market" => OrderType::Market,
        _ => return Err(AppError::BadRequest("Type must be 'limit' or 'market'".into())),
    };

    if body.qty <= dec!(0) {
        return Err(AppError::BadRequest("qty must be positive".into()));
    }

    let leverage = body.leverage.unwrap_or(1).clamp(1, 50);
    let price = body.price.unwrap_or(dec!(0));

    let (tx, rx) = tokio::sync::oneshot::channel();
    state.engine_tx
        .send(EngineCmd::PlaceOrder(PlaceOrderCmd {
            user_id: auth.user_id,
            market,
            side,
            order_type,
            price,
            qty: body.qty,
            leverage,
            is_copy_order: false,
            copied_from_user_id: None,
            resp: tx,
        }))
        .await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Engine unavailable")))?;

    let order = rx.await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Engine response lost")))?
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true, "order": order })))
}

// DELETE /orders/:order_id
pub async fn cancel_order(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(order_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {

    let (tx, rx) = tokio::sync::oneshot::channel();
    state.engine_tx
        .send(EngineCmd::CancelOrder(CancelOrderCmd {
            user_id: auth.user_id,
            order_id,
            resp: tx,
        }))
        .await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Engine unavailable")))?;

    rx.await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Engine response lost")))?
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Deserialize)]
pub struct OrdersQuery {
    pub status: Option<String>,
    pub market: Option<String>,
    pub limit: Option<i64>,
}

// GET /orders
pub async fn get_orders(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<OrdersQuery>,
) -> Result<Json<serde_json::Value>> {

    let limit = params.limit.unwrap_or(50).min(100);

    let orders = if let Some(status) = &params.status {
        if let Some(market) = &params.market {
            sqlx::query!(
                "SELECT * FROM orders WHERE user_id=$1 AND status=$2 AND market=$3 ORDER BY created_at DESC LIMIT $4",
                auth.user_id, status, market, limit
            ).fetch_all(&state.db).await?
        } else {
            sqlx::query!(
                "SELECT * FROM orders WHERE user_id=$1 AND status=$2 ORDER BY created_at DESC LIMIT $3",
                auth.user_id, status, limit
            ).fetch_all(&state.db).await?
        }
    } else {
        sqlx::query!(
            "SELECT * FROM orders WHERE user_id=$1 ORDER BY created_at DESC LIMIT $2",
            auth.user_id, limit
        ).fetch_all(&state.db).await?
    };

    Ok(Json(serde_json::json!({ "orders": orders })))
}
