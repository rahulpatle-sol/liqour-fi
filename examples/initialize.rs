use std::str::FromStr;

use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

const DEVNET_RPC: &str = "https://api.devnet.solana.com";

const PROGRAM_ID: &str = "FGJS4S51o9rSvxeomGrqacdwPFnZbBuU6p9KzhRHUx3b";
const USDC_MINT: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

const INIT_DISCRIMINATOR: [u8; 8] = [175, 175, 109, 31, 13, 152, 155, 237];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let keypair_path = std::env::var("KEYPAIR_PATH")
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.config/solana/id.json", home)
        });

    let data = std::fs::read_to_string(&keypair_path)?;
    let secret: Vec<u8> = serde_json::from_str(&data)?;
    let payer = Keypair::try_from(secret.as_slice())?;

    let program_id = Pubkey::from_str(PROGRAM_ID)?;
    let usdc_mint = Pubkey::from_str(USDC_MINT)?;
    let token_prog = Pubkey::from_str(TOKEN_PROGRAM)?;
    let system_prog = Pubkey::from_str("11111111111111111111111111111111")?;
    let rent = Pubkey::from_str("SysvarRent111111111111111111111111111111111")?;

    let (vault_config, _) =
        Pubkey::find_program_address(&[b"vault_config"], &program_id);

    let vault_ta = Keypair::new();
    println!("vault_token_account: {}", vault_ta.pubkey());
    println!("vault_config PDA:     {}", vault_config);

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new(vault_ta.pubkey(), true),
            AccountMeta::new(vault_config, false),
            AccountMeta::new_readonly(token_prog, false),
            AccountMeta::new_readonly(system_prog, false),
            AccountMeta::new_readonly(rent, false),
        ],
        data: INIT_DISCRIMINATOR.to_vec(),
    };

    let blockhash = get_blockhash().await?;

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &vault_ta],
        blockhash,
    );

    let sig = send_tx(&tx).await?;
    println!("Initialize tx: https://explorer.solana.com/tx/{}?cluster=devnet", sig);
    println!("\nSet this env var in your backend:");
    println!("VAULT_TOKEN_ACCOUNT={}", vault_ta.pubkey());
    Ok(())
}

async fn get_blockhash() -> anyhow::Result<solana_sdk::hash::Hash> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "getLatestBlockhash",
        "params": [{ "commitment": "confirmed" }]
    });
    let resp = client.post(DEVNET_RPC).json(&body).send().await?
        .json::<serde_json::Value>().await?;
    let h = resp["result"]["value"]["blockhash"].as_str()
        .ok_or_else(|| anyhow::anyhow!("no blockhash"))?;
    Ok(h.parse()?)
}

async fn send_tx(tx: &Transaction) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let encoded = bs58::encode(bincode::serialize(tx)?).into_string();
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "sendTransaction",
        "params": [encoded, { "encoding": "base58", "skipPreflight": false, "commitment": "confirmed" }]
    });
    let resp = client.post(DEVNET_RPC).json(&body).send().await?
        .json::<serde_json::Value>().await?;
    resp["result"].as_str().map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("send failed: {:?}", resp["error"]))
}
