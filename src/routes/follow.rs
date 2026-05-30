// src/routes/follow.rs
use axum::{extract::{Path, State}, Json};
use uuid::Uuid;
use crate::{engine::AppState, error::{AppError, Result}, middleware::auth::AuthUser, types::FollowRequest};

// POST /follow
pub async fn follow(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<FollowRequest>,
) -> Result<Json<serde_json::Value>> {
    if auth.user_id == body.leader_id {
        return Err(AppError::BadRequest("Cannot copy yourself".into()));
    }
    if body.copy_amount <= rust_decimal_macros::dec!(0) {
        return Err(AppError::BadRequest("copy_amount must be positive".into()));
    }

    let leader = sqlx::query!("SELECT user_id FROM users WHERE user_id = $1", body.leader_id)
        .fetch_optional(&state.db).await?;
    if leader.is_none() {
        return Err(AppError::NotFound("Trader not found".into()));
    }

    sqlx::query!(
        r#"INSERT INTO follows (follower_id, leader_id, copy_amount, is_active)
           VALUES ($1,$2,$3,TRUE)
           ON CONFLICT (follower_id, leader_id)
           DO UPDATE SET copy_amount=$3, is_active=TRUE"#,
        auth.user_id, body.leader_id, body.copy_amount
    ).execute(&state.db).await?;

    // Update follower count
    sqlx::query!(
        r#"UPDATE trader_stats SET follower_count = (
             SELECT COUNT(*) FROM follows WHERE leader_id=$1 AND is_active=TRUE
           ) WHERE user_id=$1"#,
        body.leader_id
    ).execute(&state.db).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// DELETE /follow/:leader_id
pub async fn unfollow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(leader_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query!(
        "UPDATE follows SET is_active=FALSE WHERE follower_id=$1 AND leader_id=$2",
        auth.user_id, leader_id
    ).execute(&state.db).await?;

    sqlx::query!(
        r#"UPDATE trader_stats SET follower_count = (
             SELECT COUNT(*) FROM follows WHERE leader_id=$1 AND is_active=TRUE
           ) WHERE user_id=$1"#,
        leader_id
    ).execute(&state.db).await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// GET /follow/following
pub async fn get_following(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query!(
        r#"SELECT f.leader_id, f.copy_amount, f.is_active,
                  u.username, u.wallet_address,
                  ts.total_pnl, ts.win_count, ts.total_trades
           FROM follows f
           JOIN users u ON u.user_id = f.leader_id
           LEFT JOIN trader_stats ts ON ts.user_id = f.leader_id
           WHERE f.follower_id=$1 AND f.is_active=TRUE"#,
        auth.user_id
    ).fetch_all(&state.db).await?
    .into_iter().map(|r| serde_json::json!({
        "leader_id": r.leader_id,
        "copy_amount": r.copy_amount,
        "username": r.username,
        "wallet_address": r.wallet_address,
        "total_pnl": r.total_pnl,
        "win_count": r.win_count,
        "total_trades": r.total_trades,
    })).collect::<Vec<_>>();

    Ok(Json(serde_json::json!({ "following": rows })))
}

// GET /follow/followers
pub async fn get_followers(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query!(
        r#"SELECT f.follower_id, f.copy_amount,
                  u.username, u.wallet_address
           FROM follows f
           JOIN users u ON u.user_id = f.follower_id
           WHERE f.leader_id=$1 AND f.is_active=TRUE"#,
        auth.user_id
    ).fetch_all(&state.db).await?
    .into_iter().map(|r| serde_json::json!({
        "follower_id": r.follower_id,
        "copy_amount": r.copy_amount,
        "username": r.username,
        "wallet_address": r.wallet_address,
    })).collect::<Vec<_>>();

    Ok(Json(serde_json::json!({ "followers": rows })))
}
