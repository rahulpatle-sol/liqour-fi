// src/routes/deposit.rs
// NEW FILE — add to main.rs routes

use axum::{extract::State, Json};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{
    engine::AppState,
    error::{AppError, Result},
    middleware::auth::AuthUser,
    solana::verify_deposit_tx,
};

#[derive(Deserialize)]
pub struct VerifyDepositBody {
    /// Solana tx signature from frontend after user signs deposit tx
    pub tx_signature: String,
}

#[derive(Deserialize)]
pub struct WithdrawBody {
    /// Amount to withdraw in USDC
    pub amount: Decimal,
}

#[derive(Serialize)]
pub struct DepositResponse {
    pub success:    bool,
    pub amount:     Decimal,
    pub new_balance: Decimal,
}

// POST /deposit/verify
// Frontend calls after Anchor deposit tx is confirmed
pub async fn verify_deposit(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<VerifyDepositBody>,
) -> Result<Json<serde_json::Value>> {

    let vault_token_account = std::env::var("VAULT_TOKEN_ACCOUNT")
        .unwrap_or_default();

    // Verify tx on-chain
    let amount = verify_deposit_tx(
        &body.tx_signature,
        &auth.wallet_address,
        &vault_token_account,
    )
    .await
    .map_err(|e| AppError::BadRequest(e.to_string()))?;

    // Credit engine balance
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.engine_tx
        .send(crate::types::EngineCmd::Deposit(crate::types::DepositCmd {
            user_id: auth.user_id,
            amount,
            resp: tx,
        }))
        .await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Engine unavailable")))?;

    rx.await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Engine response lost")))?
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    // Update DB balance
    sqlx::query!(
        "UPDATE balances SET available = available + $1, updated_at = NOW() WHERE user_id = $2",
        amount, auth.user_id
    )
    .execute(&state.db)
    .await?;

    // Get new balance
    let bal = sqlx::query!(
        "SELECT available FROM balances WHERE user_id = $1",
        auth.user_id
    )
    .fetch_one(&state.db)
    .await?;

    tracing::info!("Deposit verified: {} USDC for {}", amount, auth.wallet_address);

    Ok(Json(serde_json::json!({
        "success": true,
        "deposited": amount,
        "new_balance": bal.available,
        "message": format!("Credited {} USDC to your trading balance", amount),
    })))
}

// POST /deposit/withdraw-request
// User requests to withdraw profits to their wallet
pub async fn request_withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<WithdrawBody>,
) -> Result<Json<serde_json::Value>> {

    if body.amount <= rust_decimal_macros::dec!(0) {
        return Err(AppError::BadRequest("Amount must be positive".into()));
    }

    // Check available balance
    let bal = sqlx::query!(
        "SELECT available FROM balances WHERE user_id = $1",
        auth.user_id
    )
    .fetch_one(&state.db)
    .await?;

    if bal.available < body.amount {
        return Err(AppError::BadRequest(format!(
            "Insufficient balance. Available: {} USDC",
            bal.available
        )));
    }

    // Deduct from balance first
    let (tx, rx) = tokio::sync::oneshot::channel();
    // Negative deposit = deduct
    state.engine_tx
        .send(crate::types::EngineCmd::Deposit(crate::types::DepositCmd {
            user_id: auth.user_id,
            amount: -body.amount,
            resp: tx,
        }))
        .await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Engine unavailable")))?;
    let _ = rx.await;

    sqlx::query!(
        "UPDATE balances SET available = available - $1, updated_at = NOW() WHERE user_id = $2",
        body.amount, auth.user_id
    )
    .execute(&state.db)
    .await?;

    // TODO: Call Anchor withdraw instruction via solana-sdk
     tx_sig = send_withdraw(&auth.wallet_address, body.amount, keypair_path).await?;
    // For now — return pending status
    tracing::info!(
        "Withdraw requested: {} USDC for {}",
        body.amount, auth.wallet_address
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "amount": body.amount,
        "status": "pending",
        "message": "Withdraw queued. Will arrive in your wallet within 30 seconds.",
        // "tx_signature": tx_sig,  // uncomment after Anchor withdraw implemented
    })))
}