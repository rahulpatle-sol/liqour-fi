// src/routes/auth.rs
use axum::{extract::State, Json};
use bs58;
use ed25519_dalek::{Verifier, VerifyingKey, Signature};
use serde::{Deserialize, Serialize};
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::{
    engine::AppState,
    error::{AppError, Result},
    middleware::auth::create_token,
    types::LoginRequest,
};

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: Uuid,
    pub wallet_address: String,
    pub username: Option<String>,
    pub is_new_user: bool,
}

#[derive(Serialize)]
pub struct NonceResponse {
    pub nonce: String,
    pub message: String,
}

#[derive(Deserialize)]
pub struct SetUsernameRequest {
    pub username: String,
}

// GET /auth/nonce?wallet=ADDRESS
pub async fn get_nonce(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<NonceResponse>> {
    let _wallet = params.get("wallet")
        .ok_or_else(|| AppError::BadRequest("wallet param required".into()))?;

    let nonce = format!("{:x}{:x}",
        rand::random::<u64>(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    );

    let message = sign_message(&nonce);
    Ok(Json(NonceResponse { nonce, message }))
}

// POST /auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>> {

    // Verify Solana ed25519 signature
    let pubkey_bytes = bs58::decode(&body.wallet_address)
        .into_vec()
        .map_err(|_| AppError::BadRequest("Invalid wallet address".into()))?;

    let sig_bytes = bs58::decode(&body.signature)
        .into_vec()
        .map_err(|_| AppError::BadRequest("Invalid signature".into()))?;

    let verifying_key = VerifyingKey::from_bytes(
        pubkey_bytes.as_slice().try_into()
            .map_err(|_| AppError::BadRequest("Invalid public key length".into()))?
    ).map_err(|_| AppError::BadRequest("Invalid public key".into()))?;

    let signature = Signature::from_bytes(
        sig_bytes.as_slice().try_into()
            .map_err(|_| AppError::BadRequest("Invalid signature length".into()))?
    );

    let message = sign_message(&body.nonce);
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| AppError::Unauthorized("Invalid signature".into()))?;

    // Get or create user
    let existing = sqlx::query!(
        "SELECT user_id, wallet_address, username FROM users WHERE wallet_address = $1",
        body.wallet_address
    )
    .fetch_optional(&state.db)
    .await?;

    let (user_id, is_new_user) = if let Some(row) = existing {
        (row.user_id, false)
    } else {
        // Create new user
        let uid = sqlx::query!(
            "INSERT INTO users (wallet_address) VALUES ($1) RETURNING user_id",
            body.wallet_address
        )
        .fetch_one(&state.db)
        .await?
        .user_id;

        // Create balance
        sqlx::query!("INSERT INTO balances (user_id) VALUES ($1)", uid)
            .execute(&state.db)
            .await?;

        // Create trader stats
        sqlx::query!("INSERT INTO trader_stats (user_id) VALUES ($1)", uid)
            .execute(&state.db)
            .await?;

        // Give 1000 USDC paper money for demo
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = state.engine_tx.send(crate::types::EngineCmd::Deposit(
            crate::types::DepositCmd { user_id: uid, amount: dec!(1000), resp: tx }
        )).await;
        let _ = rx.await;

        sqlx::query!(
            "UPDATE balances SET available = 1000 WHERE user_id = $1", uid
        ).execute(&state.db).await?;

        (uid, true)
    };

    let user = sqlx::query!(
        "SELECT user_id, wallet_address, username FROM users WHERE user_id = $1",
        user_id
    ).fetch_one(&state.db).await?;

    let token = create_token(user_id, &body.wallet_address, &state.config.jwt_secret)
        .map_err(|e| AppError::Internal(e))?;

    Ok(Json(LoginResponse {
        token,
        user_id,
        wallet_address: user.wallet_address,
        username: user.username,
        is_new_user,
    }))
}

// PUT /auth/username
pub async fn set_username(
    State(state): State<AppState>,
    auth: crate::middleware::auth::AuthUser,
    Json(body): Json<SetUsernameRequest>,
) -> Result<Json<serde_json::Value>> {
    if body.username.len() < 3 || body.username.len() > 20 {
        return Err(AppError::BadRequest("Username must be 3-20 characters".into()));
    }

    sqlx::query!(
        "UPDATE users SET username = $1 WHERE user_id = $2",
        body.username, auth.user_id
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("unique") {
            AppError::BadRequest("Username already taken".into())
        } else {
            AppError::Database(e)
        }
    })?;

    Ok(Json(serde_json::json!({ "success": true, "username": body.username })))
}

fn sign_message(nonce: &str) -> String {
    format!("Welcome to Liqour 🥃\n\nSign to authenticate.\n\nNonce: {nonce}")
}
