// src/config.rs
use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub port: u16,
    pub pyth_hermes_url: String,
    pub pyth_sol_id: String,
    pub pyth_btc_id: String,
    pub pyth_eth_id: String,
    pub snapshot_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL not set")?,
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
            jwt_secret: std::env::var("JWT_SECRET")
                .context("JWT_SECRET not set")?,
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .context("PORT must be a number")?,
            pyth_hermes_url: std::env::var("PYTH_HERMES_URL")
                .unwrap_or_else(|_| "https://hermes.pyth.network".to_string()),
            pyth_sol_id: std::env::var("PYTH_SOL_ID")
                .unwrap_or_else(|_| "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d".to_string()),
            pyth_btc_id: std::env::var("PYTH_BTC_ID")
                .unwrap_or_else(|_| "0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43".to_string()),
            pyth_eth_id: std::env::var("PYTH_ETH_ID")
                .unwrap_or_else(|_| "0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace".to_string()),
            snapshot_interval_secs: std::env::var("SNAPSHOT_INTERVAL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .unwrap_or(300),
        })
    }
}
