use anyhow::{anyhow, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub const PROGRAM_ID: &str = "FGJS4S51o9rSvxeomGrqacdwPFnZbBuU6p9KzhRHUx3b";

const DEPOSIT_DISCRIMINATOR: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];

pub fn extract_deposit_amount(tx: &serde_json::Value) -> Result<Decimal> {
    let account_keys = tx["transaction"]["message"]["accountKeys"]
        .as_array()
        .ok_or_else(|| anyhow!("No accountKeys in transaction"))?;
    let instructions = tx["transaction"]["message"]["instructions"]
        .as_array()
        .ok_or_else(|| anyhow!("No instructions in transaction"))?;

    let program_bytes = bs58::decode(PROGRAM_ID).into_vec()?;

    for inst in instructions {
        let prog_idx = inst["programIdIndex"]
            .as_u64()
            .ok_or_else(|| anyhow!("No programIdIndex"))? as usize;
        let pk = account_keys[prog_idx]["pubkey"]
            .as_str()
            .ok_or_else(|| anyhow!("No pubkey at index"))?;

        if bs58::decode(pk).into_vec()? != program_bytes {
            continue;
        }

        let data_b58 = inst["data"]
            .as_str()
            .ok_or_else(|| anyhow!("No instruction data"))?;
        let data = bs58::decode(data_b58).into_vec()?;

        if data.len() < 16 {
            continue;
        }

        let discriminator: [u8; 8] = data[..8].try_into().unwrap();
        if discriminator != DEPOSIT_DISCRIMINATOR {
            continue;
        }

        let amount_raw = u64::from_le_bytes(data[8..16].try_into().unwrap());
        return Ok(Decimal::from(amount_raw) / dec!(1_000_000));
    }

    Err(anyhow!("No liqour_defi deposit instruction found in transaction"))
}
