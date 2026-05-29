// src/engine/snapshot.rs
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use sqlx::PgPool;
use super::EngineState;

pub async fn start_snapshot_scheduler(
    state: Arc<RwLock<EngineState>>,
    db: PgPool,
    interval_secs: u64,
) {
    let mut ticker = interval(Duration::from_secs(interval_secs));
    ticker.tick().await; // skip first immediate tick

    loop {
        ticker.tick().await;
        if let Err(e) = take_snapshot(&state, &db).await {
            tracing::error!("Snapshot error: {}", e);
        }
    }
}

async fn take_snapshot(
    state: &Arc<RwLock<EngineState>>,
    db: &PgPool,
) -> anyhow::Result<()> {
    let st = state.read().await;

    let balances: Vec<serde_json::Value> = st.balances.values().map(|b| {
        serde_json::json!({
            "user_id": b.user_id,
            "available": b.available,
            "locked": b.locked,
        })
    }).collect();

    let snapshot_data = serde_json::json!({
        "balances": balances,
        "timestamp": chrono::Utc::now().timestamp(),
    });

    sqlx::query!(
        "INSERT INTO engine_snapshots (snapshot_data) VALUES ($1)",
        snapshot_data
    ).execute(db).await?;

    // Keep only last 10
    sqlx::query!(
        r#"DELETE FROM engine_snapshots
           WHERE snapshot_id NOT IN (
             SELECT snapshot_id FROM engine_snapshots
             ORDER BY created_at DESC LIMIT 10
           )"#
    ).execute(db).await?;

    tracing::info!("📸 Snapshot saved");
    Ok(())
}
