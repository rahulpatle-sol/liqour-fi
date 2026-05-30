// src/solana/mod.rs
// Backend verifies on-chain tx + sends withdraw instructions

use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;

const DEVNET_RPC: &str = "https://api.devnet.solana.com";
// Devnet USDC mint
pub const USDC_MINT_DEVNET: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";

// ── Verify a deposit transaction ──────────────────────────────────────────────
// Returns amount deposited (in USDC, 6 decimals) if tx is valid

pub async fn verify_deposit_tx(
    tx_signature: &str,
    expected_wallet: &str,
    vault_token_account: &str,
) -> Result<Decimal> {
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTransaction",
        "params": [
            tx_signature,
            { "encoding": "jsonParsed", "commitment": "confirmed" }
        ]
    });

    let resp = client
        .post(DEVNET_RPC)
        .json(&body)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let tx = resp["result"]
        .as_object()
        .ok_or_else(|| anyhow!("Transaction not found or not confirmed yet"))?;

    // Check tx succeeded
    let err = &tx["meta"]["err"];
    if !err.is_null() {
        return Err(anyhow!("Transaction failed on-chain: {:?}", err));
    }

    // Find the token transfer to vault_token_account
    let pre_balances = &tx["meta"]["preTokenBalances"];
    let post_balances = &tx["meta"]["postTokenBalances"];

    let pre_arr  = pre_balances.as_array().ok_or_else(|| anyhow!("No pre token balances"))?;
    let post_arr = post_balances.as_array().ok_or_else(|| anyhow!("No post token balances"))?;

    // Find vault account in post balances
    let vault_post = post_arr.iter().find(|b| {
        b["owner"].as_str() == Some(vault_token_account)
            || b["accountIndex"].as_u64().map_or(false, |_| {
                // Check account keys
                tx["transaction"]["message"]["accountKeys"]
                    .as_array()
                    .and_then(|keys| {
                        let idx = b["accountIndex"].as_u64()? as usize;
                        Some(keys[idx]["pubkey"].as_str() == Some(vault_token_account))
                    })
                    .unwrap_or(false)
            })
    });

    let vault_pre = pre_arr.iter().find(|b| {
        b["owner"].as_str() == Some(vault_token_account)
    });

    let post_amount: u64 = vault_post
        .and_then(|b| b["uiTokenAmount"]["amount"].as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let pre_amount: u64 = vault_pre
        .and_then(|b| b["uiTokenAmount"]["amount"].as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let deposited_lamports = post_amount.saturating_sub(pre_amount);

    if deposited_lamports == 0 {
        return Err(anyhow!("No USDC transfer to vault found in transaction"));
    }

    // USDC has 6 decimals
    let amount = Decimal::from(deposited_lamports) / dec!(1_000_000);
    Ok(amount)
}

// ── Send withdraw instruction ─────────────────────────────────────────────────
// Backend calls Anchor program to send USDC to user

pub async fn send_withdraw(
    user_wallet: &str,
    amount_usdc: Decimal,
    backend_keypair_path: &str,
) -> Result<String> {
    // In production: use solana-sdk to build + sign + send the withdraw instruction
    // For now: return placeholder — implement with solana-sdk crate
    // 
    // anchor_client::Client::new_with_options(cluster, payer, opts)
    //   .program(program_id)
    //   .request()
    //   .instruction(withdraw { amount: amount_lamports })
    //   .send()?
    
    tracing::info!(
        "TODO: Send withdraw {} USDC to {} via Anchor program",
        amount_usdc, user_wallet
    );
    
    Ok("tx_signature_placeholder".to_string())
}

// ── Devnet USDC airdrop helper ────────────────────────────────────────────────
// For testing — get devnet USDC from faucet

pub fn get_devnet_usdc_faucet_url(wallet: &str) -> String {
    format!(
        "https://spl-token-faucet.com/?token-name=USDC-Dev&quantity=100&receiver={}",
        wallet
    )
}