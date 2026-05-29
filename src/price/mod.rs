// src/price/mod.rs
use std::sync::Arc;
use tokio::time::{interval, Duration};
use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::{config::Config, engine::Candle, types::{EngineCmd, Market}};

#[derive(Deserialize)]
struct HermesResponse {
    parsed: Vec<ParsedPrice>,
}

#[derive(Deserialize)]
struct ParsedPrice {
    id: String,
    price: PriceInfo,
}

#[derive(Deserialize)]
struct PriceInfo {
    price: String,
    expo: i32,
}

pub async fn start_price_feed(
    config: Arc<Config>,
    engine_tx: mpsc::Sender<EngineCmd>,
    prices: Arc<DashMap<String, Decimal>>,
    candles: Arc<DashMap<String, Vec<Candle>>>,
) {
    tracing::info!("📡 Starting Pyth price feed...");

    let market_ids = vec![
        (Market::Sol, config.pyth_sol_id.trim_start_matches("0x").to_string()),
        (Market::Btc, config.pyth_btc_id.trim_start_matches("0x").to_string()),
        (Market::Eth, config.pyth_eth_id.trim_start_matches("0x").to_string()),
    ];

    let ids_param = market_ids.iter()
        .map(|(_, id)| format!("ids[]={}", id))
        .collect::<Vec<_>>()
        .join("&");

    let url = format!(
        "{}/v2/updates/price/latest?{}&encoding=hex&parsed=true",
        config.pyth_hermes_url, ids_param
    );

    let client = reqwest::Client::new();
    let mut ticker = interval(Duration::from_secs(1));

    loop {
        ticker.tick().await;

        match client.get(&url).timeout(Duration::from_secs(5)).send().await {
            Ok(resp) => {
                if let Ok(data) = resp.json::<HermesResponse>().await {
                    for parsed in &data.parsed {
                        let id_lower = parsed.id.to_lowercase();

                        let market = market_ids.iter()
                            .find(|(_, mid)| mid.to_lowercase() == id_lower)
                            .map(|(m, _)| *m);

                        if let Some(market) = market {
                            let raw: i64 = parsed.price.price.parse().unwrap_or(0);
                            let price = Decimal::from(raw)
                                * Decimal::from(10i64.pow(parsed.price.expo.unsigned_abs()))
                                    .checked_inv().unwrap_or(dec!(1));

                            if price > dec!(0) {
                                prices.insert(market.as_str().to_string(), price);
                                update_candle(&candles, market.as_str(), price);

                                let _ = engine_tx.send(EngineCmd::UpdatePrice { market, price }).await;
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::warn!("Pyth fetch error: {}", e),
        }
    }
}

fn update_candle(candles: &Arc<DashMap<String, Vec<Candle>>>, market: &str, price: Decimal) {
    let now_min = (chrono::Utc::now().timestamp() / 60) * 60;

    let mut entry = candles.entry(market.to_string()).or_insert_with(Vec::new);

    if let Some(last) = entry.last_mut() {
        if last.timestamp == now_min {
            last.high = last.high.max(price);
            last.low  = last.low.min(price);
            last.close = price;
            return;
        }
    }

    entry.push(Candle {
        open: price, high: price, low: price, close: price,
        volume: dec!(0), timestamp: now_min,
    });

    // Keep last 1000 candles
    if entry.len() > 1000 {
        entry.remove(0);
    }
}
