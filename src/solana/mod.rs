use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

mod verify;
pub use verify::{extract_deposit_amount, PROGRAM_ID};

const DEVNET_RPC: &str = "https://api.devnet.solana.com";

const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xr25ix9sJ5qwTtK6";
const USDC_MINT: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";

const WITHDRAW_DISCRIMINATOR: [u8; 8] = [183, 18, 70, 156, 148, 109, 161, 34];

pub async fn verify_deposit_tx(
    tx_signature: &str,
    _expected_wallet: &str,
    _vault_token_account: &str,
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

    let tx = &resp["result"];
    if !tx.is_object() {
        return Err(anyhow!("Transaction not found or not confirmed yet"));
    }

    let err = &tx["meta"]["err"];
    if !err.is_null() {
        return Err(anyhow!("Transaction failed on-chain: {:?}", err));
    }

    extract_deposit_amount(tx)
}

#[allow(dead_code)]
pub async fn send_withdraw(
    user_wallet: &str,
    amount_usdc: Decimal,
    backend_keypair_path: &str,
) -> Result<String> {
    let keypair_data = tokio::fs::read_to_string(backend_keypair_path).await?;
    let secret_bytes: Vec<u8> = serde_json::from_str(&keypair_data)?;
    let keypair = Keypair::try_from(secret_bytes.as_slice())
        .map_err(|e| anyhow!("Invalid keypair: {e}"))?;

    let program_id: Pubkey = PROGRAM_ID.parse()?;
    let user_pk: Pubkey = user_wallet.parse()?;
    let usdc_mint: Pubkey = USDC_MINT.parse()?;
    let token_prog: Pubkey = TOKEN_PROGRAM_ID.parse()?;
    let ata_prog: Pubkey = ASSOCIATED_TOKEN_PROGRAM_ID.parse()?;

    let user_usdc_ata =
        Pubkey::find_program_address(
            &[user_pk.as_ref(), token_prog.as_ref(), usdc_mint.as_ref()],
            &ata_prog,
        )
        .0;

    let (vault_config, _) =
        Pubkey::find_program_address(&[b"vault_config"], &program_id);
    let (user_vault, _) =
        Pubkey::find_program_address(&[b"user_vault", user_pk.as_ref()], &program_id);

    let vault_ta: Pubkey = std::env::var("VAULT_TOKEN_ACCOUNT")
        .map_err(|_| anyhow!("VAULT_TOKEN_ACCOUNT not set"))?
        .parse()?;

    let raw_str = (amount_usdc * dec!(1_000_000))
        .round_dp(0)
        .to_string();
    let amount_raw: u64 = raw_str
        .parse()
        .map_err(|e| anyhow!("Invalid amount: {e}"))?;

    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&WITHDRAW_DISCRIMINATOR);
    data.extend_from_slice(&amount_raw.to_le_bytes());

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(keypair.pubkey(), true),
            AccountMeta::new(user_usdc_ata, false),
            AccountMeta::new(vault_ta, false),
            AccountMeta::new(vault_config, false),
            AccountMeta::new(user_vault, false),
            AccountMeta::new_readonly(token_prog, false),
        ],
        data,
    };

    let blockhash = rpc_get_latest_blockhash().await?;

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&keypair.pubkey()),
        &[&keypair],
        blockhash,
    );

    let tx_sig = rpc_send_transaction(&tx).await?;
    Ok(tx_sig)
}

#[allow(dead_code)]
async fn rpc_get_latest_blockhash() -> Result<solana_sdk::hash::Hash> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestBlockhash",
        "params": [{ "commitment": "confirmed" }]
    });

    let resp = client
        .post(DEVNET_RPC)
        .json(&body)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let hash_str = resp["result"]["value"]["blockhash"]
        .as_str()
        .ok_or_else(|| anyhow!("Failed to get blockhash"))?;

    hash_str
        .parse()
        .map_err(|e| anyhow!("Invalid blockhash: {e}"))
}

#[allow(dead_code)]
async fn rpc_send_transaction(tx: &Transaction) -> Result<String> {
    let client = reqwest::Client::new();
    let encoded = bs58::encode(bincode::serialize(tx)?).into_string();

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [
            encoded,
            { "encoding": "base58", "skipPreflight": false, "commitment": "confirmed" }
        ]
    });

    let resp = client
        .post(DEVNET_RPC)
        .json(&body)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let sig = resp["result"]
        .as_str()
        .ok_or_else(|| {
            let err = &resp["error"];
            anyhow!("Send transaction failed: {:?}", err)
        })?
        .to_string();

    Ok(sig)
}
