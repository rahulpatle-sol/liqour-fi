// src/main.rs
mod config;
mod engine;
mod error;
mod middleware;
mod price;
mod routes;
mod types;
mod ws;
mod solana;

use std::sync::Arc;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use dashmap::DashMap;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::{broadcast, mpsc, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::{
    config::Config,
    engine::{run_engine, load_state_from_db, AppState, EngineState},
    types::WsEvent,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env
    dotenv::dotenv().ok();

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("liqour=info".parse()?)
        )
        .init();

    tracing::info!("\n🥃 Starting Liqour Backend (Rust)\n");

    let config = Arc::new(Config::from_env()?);

    // ── Database ──────────────────────────────────────────────────────────────
    tracing::info!("Connecting to NeonDB...");
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    tracing::info!("✓ Database connected");

    // ── Engine State ──────────────────────────────────────────────────────────
    let engine_state = Arc::new(RwLock::new(EngineState::new()));
    load_state_from_db(&engine_state, &db).await?;

    // ── Channels ──────────────────────────────────────────────────────────────
    let (engine_tx, engine_rx) = mpsc::channel::<types::EngineCmd>(1024);
    let (event_tx, _)          = broadcast::channel::<WsEvent>(4096);

    // ── Shared State ──────────────────────────────────────────────────────────
    let prices: Arc<DashMap<String, rust_decimal::Decimal>> = Arc::new(DashMap::new());
    let candles: Arc<DashMap<String, Vec<engine::Candle>>>  = Arc::new(DashMap::new());

    let app_state = AppState {
        db: db.clone(),
        engine_tx: engine_tx.clone(),
        event_tx: event_tx.clone(),
        engine_state: engine_state.clone(),
        config: config.clone(),
        prices: prices.clone(),
        candles: candles.clone(),
    };

    // ── Engine Task ───────────────────────────────────────────────────────────
    {
        let st  = engine_state.clone();
        let db2 = db.clone();
        let tx2 = event_tx.clone();
        let p2  = prices.clone();
        tokio::spawn(async move {
            run_engine(engine_rx, tx2, st, db2, p2).await;
        });
    }

    // ── Price Feed Task ───────────────────────────────────────────────────────
    {
        let cfg = config.clone();
        let etx = engine_tx.clone();
        let p   = prices.clone();
        let c   = candles.clone();
        tokio::spawn(async move {
            price::start_price_feed(cfg, etx, p, c).await;
        });
    }

    // ── Snapshot Task ─────────────────────────────────────────────────────────
    {
        let st  = engine_state.clone();
        let db2 = db.clone();
        let interval = config.snapshot_interval_secs;
        tokio::spawn(async move {
            engine::snapshot::start_snapshot_scheduler(st, db2, interval).await;
        });
    }

    // ── Router ────────────────────────────────────────────────────────────────
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Health
        .route("/health", get(health))
        .route("/", get(api_index))

        // Auth
        .route("/auth/nonce",    get(routes::auth::get_nonce))
        .route("/auth/login",    post(routes::auth::login))
        .route("/auth/username", put(routes::auth::set_username))

        // Markets (public)
        .route("/markets",                  get(routes::markets::get_markets))
        .route("/markets/:market",          get(routes::markets::get_market))
        .route("/markets/:market/candles",  get(routes::markets::get_candles))
        .route("/markets/:market/trades",   get(routes::markets::get_trades))

        // Leaderboard (public)
        .route("/leaderboard",          get(routes::leaderboard::get_leaderboard))
        .route("/leaderboard/:user_id", get(routes::leaderboard::get_trader))

        // Orders (auth required)
        .route("/orders",          post(routes::orders::place_order))
        .route("/orders",          get(routes::orders::get_orders))
        .route("/orders/:id",      delete(routes::orders::cancel_order))

        // Positions (auth required)
        .route("/positions",              get(routes::positions::get_positions))
        .route("/positions/history",      get(routes::positions::get_history))
        .route("/positions/candles/:market", get(routes::positions::get_candles))

        // Follow / Copy trade (auth required)
        .route("/follow",              post(routes::follow::follow))
        .route("/follow/:leader_id",   delete(routes::follow::unfollow))
        .route("/follow/following",    get(routes::follow::get_following))
        .route("/follow/followers",    get(routes::follow::get_followers))

        // WebSocket
        .route("/ws", get(ws::ws_handler))
       //  new solana routes
       .route("/deposit/verify",           post(routes::deposit::verify_deposit))
        .route("/deposit/withdraw-request", post(routes::deposit::request_withdraw))
        // close the trade also 
        .route("/positions/close", post(routes::positions::close_position))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);
       
    // ── Listen ────────────────────────────────────────────────────────────────
    let port = config.port;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    tracing::info!("🚀 Liqour (Rust) running on port {}", port);
    tracing::info!("📡 WebSocket at ws://localhost:{}/ws", port);
    tracing::info!("📖 API at http://localhost:{}/\n", port);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "liqour-rust",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

async fn api_index() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "Liqour 🥃 Social Perps — Rust Backend",
        "version": "1.0.0",
        "engine": "Tokio multi-threaded + in-memory orderbook",
        "endpoints": {
            "GET  /health": "Health check",
            "GET  /auth/nonce": "Get nonce to sign",
            "POST /auth/login": "Login with Solana wallet",
            "PUT  /auth/username": "Set username",
            "GET  /markets": "All markets",
            "GET  /markets/:m/candles": "OHLCV candles",
            "GET  /markets/:m/trades": "Recent trades",
            "POST /orders": "Place order",
            "DELETE /orders/:id": "Cancel order",
            "GET  /orders": "My orders",
            "GET  /positions": "My open positions",
            "GET  /positions/history": "My trade history",
            "GET  /leaderboard": "Top traders",
            "GET  /leaderboard/:id": "Trader profile",
            "POST /follow": "Start copy trading",
            "DELETE /follow/:id": "Stop copy trading",
            "GET  /follow/following": "Who I copy",
            "GET  /follow/followers": "My followers",
            "WS   /ws": "Real-time WebSocket",
        }
    }))
}
