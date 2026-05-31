use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [
            "C2wEmmZRPgM4UuhC6nXkkFWFivSWPX7fhqr1CFai9TSy",
            { "encoding": "base58", "commitment": "confirmed" }
        ]
    });
    let resp = client
        .post("https://api.devnet.solana.com")
        .json(&body)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
    Ok(())
}
