// src/engine/mod.rs
pub mod orderbook;
pub mod snapshot;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use chrono::Utc;
use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

use crate::types::*;
use sqlx::PgPool;

// ─── Engine State ─────────────────────────────────────────────────────────────

pub struct EngineState {
    pub orderbooks: HashMap<Market, orderbook::Orderbook>,
    pub balances: HashMap<Uuid, Balance>,
    pub positions: HashMap<Uuid, Vec<Position>>,
}

impl EngineState {
    pub fn new() -> Self {
        let mut orderbooks = HashMap::new();
        for market in Market::all() {
            orderbooks.insert(market, orderbook::Orderbook::new());
        }
        Self {
            orderbooks,
            balances: HashMap::new(),
            positions: HashMap::new(),
        }
    }

    pub fn get_balance(&mut self, user_id: Uuid) -> &mut Balance {
        self.balances.entry(user_id).or_insert(Balance {
            user_id,
            available: dec!(0),
            locked: dec!(0),
        })
    }

    pub fn get_positions(&self, user_id: Uuid) -> Vec<Position> {
        self.positions.get(&user_id).cloned().unwrap_or_default()
    }

    pub fn get_index_price(&self, market: Market) -> Decimal {
        self.orderbooks.get(&market)
            .map(|ob| ob.index_price)
            .unwrap_or(dec!(0))
    }
}

// ─── App State (shared across all handlers) ───────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub engine_tx: mpsc::Sender<EngineCmd>,
    pub event_tx: broadcast::Sender<WsEvent>,
    pub engine_state: Arc<RwLock<EngineState>>,
    pub config: Arc<crate::config::Config>,
    // Latest prices - fast read without locking engine
    pub prices: Arc<DashMap<String, Decimal>>,
    // Candles cache
    pub candles: Arc<DashMap<String, Vec<Candle>>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Candle {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub timestamp: i64,
}

// ─── Engine Task ──────────────────────────────────────────────────────────────
// Runs as dedicated Tokio task - single consumer, no data races

pub async fn run_engine(
    mut cmd_rx: mpsc::Receiver<EngineCmd>,
    event_tx: broadcast::Sender<WsEvent>,
    state: Arc<RwLock<EngineState>>,
    db: PgPool,
    prices: Arc<DashMap<String, Decimal>>,
) {
    tracing::info!("⚙️  Engine task started");

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            EngineCmd::PlaceOrder(cmd) => {
                let result = handle_place_order(
                    cmd.user_id, cmd.market, cmd.side,
                    cmd.order_type, cmd.price, cmd.qty, cmd.leverage,
                    cmd.is_copy_order, cmd.copied_from_user_id,
                    &state, &db, &event_tx,
                ).await;
                let _ = cmd.resp.send(result);
            }

            EngineCmd::CancelOrder(cmd) => {
                let result = handle_cancel_order(
                    cmd.user_id, cmd.order_id, &state, &db
                ).await;
                let _ = cmd.resp.send(result);
            }

            EngineCmd::Deposit(cmd) => {
                let result = handle_deposit(cmd.user_id, cmd.amount, &state, &db).await;
                let _ = cmd.resp.send(result);
            }

            EngineCmd::UpdatePrice { market, price } => {
                prices.insert(market.as_str().to_string(), price);

                let mut st = state.write().await;
                if let Some(ob) = st.orderbooks.get_mut(&market) {
                    ob.index_price = price;
                }

                // Check liquidations
                check_liquidations(market, price, &mut st, &db, &event_tx).await;

                // Broadcast price update
                let _ = event_tx.send(WsEvent::PriceUpdate {
                    market: market.as_str().to_string(),
                    price,
                    timestamp: Utc::now().timestamp_millis(),
                });
            }

            EngineCmd::Shutdown => {
                tracing::info!("Engine shutting down");
                break;
            }
        }
    }
}

// ─── Place Order Handler ──────────────────────────────────────────────────────

async fn handle_place_order(
    user_id: Uuid,
    market: Market,
    side: Side,
    order_type: OrderType,
    price: Decimal,
    qty: Decimal,
    leverage: i32,
    is_copy_order: bool,
    copied_from_user_id: Option<Uuid>,
    state: &Arc<RwLock<EngineState>>,
    db: &PgPool,
    event_tx: &broadcast::Sender<WsEvent>,
) -> anyhow::Result<Order> {

    let mut st = state.write().await;

    let index_price = st.orderbooks.get(&market)
        .map(|ob| ob.index_price)
        .unwrap_or(dec!(0));

    let effective_price = match order_type {
        OrderType::Market => {
            if index_price <= dec!(0) {
                return Err(anyhow!("No price feed available for {}", market.as_str()));
            }
            index_price
        }
        OrderType::Limit => {
            if price <= dec!(0) {
                return Err(anyhow!("Limit orders require a positive price"));
            }
            price
        }
    };

    // Validate
    if qty <= dec!(0) { return Err(anyhow!("Quantity must be positive")); }
    if leverage < 1 || leverage > 50 { return Err(anyhow!("Leverage must be 1-50x")); }

    let margin = (effective_price * qty) / Decimal::from(leverage);
    let bal = st.get_balance(user_id);

    if bal.available < margin {
        return Err(anyhow!(
            "Insufficient balance. Need {:.2} USDC, have {:.2}",
            margin, bal.available
        ));
    }

    // Lock margin
    bal.available -= margin;
    bal.locked += margin;

    let order_id = Uuid::new_v4();
    let now = Utc::now();

    let mut order = Order {
        order_id,
        user_id,
        market,
        side,
        order_type,
        price: effective_price,
        qty,
        filled_qty: dec!(0),
        margin,
        leverage,
        status: OrderStatus::Open,
        is_copy_order,
        copied_from_user_id,
        created_at: now,
    };

    // Persist order
    sqlx::query!(
        r#"INSERT INTO orders
           (order_id, user_id, market, side, type, price, qty, filled_qty, margin, leverage,
            status, is_copy_order, copied_from_user_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        order.order_id,
        order.user_id,
        market.as_str(),
        format!("{:?}", side).to_lowercase(),
        format!("{:?}", order_type).to_lowercase(),
        order.price,
        order.qty,
        dec!(0),
        order.margin,
        order.leverage,
        "open",
        is_copy_order,
        copied_from_user_id
    ).execute(db).await?;

    // Run matching engine
    let fills = orderbook::match_order(
        &mut order,
        st.orderbooks.get_mut(&market).unwrap(),
        user_id,
    );

    // Process fills
    for fill in &fills {
        process_fill(fill, &mut st, db, event_tx).await;
    }

    // If not fully filled, add to orderbook
    if order.status == OrderStatus::Open || order.status == OrderStatus::Partial {
        orderbook::add_to_book(&order, st.orderbooks.get_mut(&market).unwrap());
    }

    // Update order in DB
    sqlx::query!(
        "UPDATE orders SET status = $1, filled_qty = $2, updated_at = NOW() WHERE order_id = $3",
        format!("{:?}", order.status).to_lowercase(),
        order.filled_qty,
        order.order_id
    ).execute(db).await?;

    // Broadcast orderbook update
    let ob_snapshot = orderbook::get_snapshot(
        st.orderbooks.get(&market).unwrap(), market
    );
    let _ = event_tx.send(WsEvent::OrderbookUpdate(ob_snapshot));

    // Trigger copy trades if original (non-copy) order was filled
    if !is_copy_order && !fills.is_empty() {
        drop(st); // Release lock before async DB calls
        trigger_copy_trades(
            user_id, market, side, order_type,
            effective_price, qty, leverage,
            state, db, event_tx
        ).await;
    }

    Ok(order)
}

// ─── Fill Processor ───────────────────────────────────────────────────────────

async fn process_fill(
    fill: &InternalFill,
    st: &mut EngineState,
    db: &PgPool,
    event_tx: &broadcast::Sender<WsEvent>,
) {
    let fill_id = Uuid::new_v4();
    let now = Utc::now();

    // Update positions for both parties
    let maker_pnl = if let Some(maker_id) = fill.maker_user_id {
        update_position(maker_id, fill.market, fill.maker_side, fill.qty, fill.price, st)
    } else {
        dec!(0)
    };

    let taker_pnl = update_position(
        fill.taker_user_id, fill.market, fill.taker_side, fill.qty, fill.price, st
    );

    let total_pnl = maker_pnl + taker_pnl;

    // Persist fill
    let _ = sqlx::query!(
        r#"INSERT INTO fills
           (fill_id, maker_user_id, taker_user_id, market, price, qty,
            maker_order_id, taker_order_id, pnl)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        fill_id,
        fill.maker_user_id,
        fill.taker_user_id,
        fill.market.as_str(),
        fill.price,
        fill.qty,
        fill.maker_order_id,
        fill.taker_order_id,
        total_pnl
    ).execute(db).await;

    // Update trader stats
    if let Some(maker_id) = fill.maker_user_id {
        let _ = update_trader_stats(maker_id, fill.qty * fill.price, maker_pnl, db).await;
    }
    let _ = update_trader_stats(fill.taker_user_id, fill.qty * fill.price, taker_pnl, db).await;

    // Broadcast fill
    let _ = event_tx.send(WsEvent::Fill(FillEvent {
        fill_id,
        market: fill.market.as_str().to_string(),
        price: fill.price,
        qty: fill.qty,
        maker_user_id: fill.maker_user_id,
        taker_user_id: Some(fill.taker_user_id),
        created_at: now,
    }));
}

fn update_position(
    user_id: Uuid,
    market: Market,
    side: Side,
    qty: Decimal,
    fill_price: Decimal,
    st: &mut EngineState,
) -> Decimal {
    let positions = st.positions.entry(user_id).or_insert_with(Vec::new);

    if let Some(pos) = positions.iter_mut().find(|p| p.market == market && p.side == side) {
        // Add to existing position (average entry)
        let new_qty = pos.qty + qty;
        pos.entry_price = (pos.entry_price * pos.qty + fill_price * qty) / new_qty;
        pos.qty = new_qty;
        pos.margin += (fill_price * qty) / Decimal::from(pos.leverage);
        pos.liquidation_price = calc_liquidation_price(pos.entry_price, side, pos.leverage);
        dec!(0)
    } else {
        // New position
        let leverage = 1; // default, actual leverage tracked from order
        let margin = (fill_price * qty) / Decimal::from(leverage);
        let liquidation_price = calc_liquidation_price(fill_price, side, leverage);

        positions.push(Position {
            position_id: Uuid::new_v4(),
            user_id,
            market,
            side,
            qty,
            entry_price: fill_price,
            margin,
            leverage,
            liquidation_price,
            opened_at: Utc::now(),
        });
        dec!(0)
    }
}

pub fn calc_liquidation_price(entry_price: Decimal, side: Side, leverage: i32) -> Decimal {
    let maintenance = dec!(0.005); // 0.5%
    let lev = Decimal::from(leverage);
    match side {
        Side::Long  => entry_price * (dec!(1) - dec!(1) / lev + maintenance),
        Side::Short => entry_price * (dec!(1) + dec!(1) / lev - maintenance),
    }
}

// ─── Liquidation Checker ──────────────────────────────────────────────────────

async fn check_liquidations(
    market: Market,
    current_price: Decimal,
    st: &mut EngineState,
    db: &PgPool,
    event_tx: &broadcast::Sender<WsEvent>,
) {
    let user_ids: Vec<Uuid> = st.positions.keys().cloned().collect();

    for user_id in user_ids {
        let to_liquidate: Vec<Position> = st.positions
            .get(&user_id)
            .map(|positions| {
                positions.iter()
                    .filter(|p| p.market == market)
                    .filter(|p| {
                        match p.side {
                            Side::Long  => current_price <= p.liquidation_price,
                            Side::Short => current_price >= p.liquidation_price,
                        }
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        for pos in to_liquidate {
            tracing::warn!("🚨 Liquidating user {} on {} @ {}", user_id, market.as_str(), current_price);

            // Remove position
            if let Some(positions) = st.positions.get_mut(&user_id) {
                positions.retain(|p| p.position_id != pos.position_id);
            }

            // Forfeit margin
            let bal = st.get_balance(user_id);
            bal.locked -= pos.margin;

            // Record liquidation
            let _ = sqlx::query!(
                "DELETE FROM positions WHERE position_id = $1",
                pos.position_id
            ).execute(db).await;

            let _ = sqlx::query!(
                r#"INSERT INTO fills (fill_id, taker_user_id, market, price, qty, pnl)
                   VALUES ($1,$2,$3,$4,$5,$6)"#,
                Uuid::new_v4(), user_id, market.as_str(),
                current_price, pos.qty, -pos.margin
            ).execute(db).await;

            let _ = update_trader_stats(user_id, dec!(0), -pos.margin, db).await;

            let _ = event_tx.send(WsEvent::PositionUpdate {
                user_id,
                market: market.as_str().to_string(),
                positions: vec![],
            });
        }
    }
}

// ─── Copy Trade ───────────────────────────────────────────────────────────────

async fn trigger_copy_trades(
    leader_id: Uuid,
    market: Market,
    side: Side,
    order_type: OrderType,
    price: Decimal,
    qty: Decimal,
    leverage: i32,
    state: &Arc<RwLock<EngineState>>,
    db: &PgPool,
    event_tx: &broadcast::Sender<WsEvent>,
) {
    let followers = sqlx::query!(
        r#"SELECT f.follower_id, f.copy_amount, b.available
           FROM follows f
           JOIN balances b ON b.user_id = f.follower_id
           WHERE f.leader_id = $1 AND f.is_active = TRUE"#,
        leader_id
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let leader_position_value = price * qty;

    for row in followers {
        let follower_id: Uuid = row.follower_id;
        let copy_amount: Decimal = row.copy_amount;
        let available: Decimal = row.available;

        if available < copy_amount * dec!(0.1) {
            tracing::warn!("Follower {} has insufficient balance, skipping", follower_id);
            continue;
        }

        // Scale qty proportionally
        let follower_qty = if leader_position_value > dec!(0) {
            (copy_amount / leader_position_value) * qty
        } else {
            continue
        };

        tracing::info!(
            "📋 Copy: {} copying {} — {} {} {} @ {}",
            follower_id, leader_id,
            format!("{:?}", side).to_lowercase(),
            follower_qty, market.as_str(), price
        );

     let _ = Box::pin(handle_place_order(
    follower_id, market, side, order_type,
    price, follower_qty, leverage,
    true, Some(leader_id),
    state, db, event_tx
)).await;
    }
}

// ─── Cancel Order ─────────────────────────────────────────────────────────────

async fn handle_cancel_order(
    user_id: Uuid,
    order_id: Uuid,
    state: &Arc<RwLock<EngineState>>,
    db: &PgPool,
) -> anyhow::Result<()> {
    let mut st = state.write().await;

    for (_market, ob) in st.orderbooks.iter_mut() {
        if let Some(remaining_qty) = ob.remove_order(order_id, user_id) {
            // Return margin
            let margin_per_unit = sqlx::query!(
                "SELECT margin, qty FROM orders WHERE order_id = $1 AND user_id = $2",
                order_id, user_id
            )
            .fetch_optional(db)
            .await?;

            if let Some(row) = margin_per_unit {
                let margin_to_return = (row.margin / row.qty) * remaining_qty;
                let bal = st.get_balance(user_id);
                bal.locked -= margin_to_return;
                bal.available += margin_to_return;
            }

            sqlx::query!(
                "UPDATE orders SET status = 'cancelled', updated_at = NOW() WHERE order_id = $1",
                order_id
            ).execute(db).await?;

            return Ok(());
        }
    }

    Err(anyhow!("Order not found or already filled"))
}

// ─── Deposit ──────────────────────────────────────────────────────────────────

async fn handle_deposit(
    user_id: Uuid,
    amount: Decimal,
    state: &Arc<RwLock<EngineState>>,
    db: &PgPool,
) -> anyhow::Result<()> {
    let mut st = state.write().await;
    let bal = st.get_balance(user_id);
    bal.available += amount;

    sqlx::query!(
        "UPDATE balances SET available = available + $1, updated_at = NOW() WHERE user_id = $2",
        amount, user_id
    ).execute(db).await?;

    Ok(())
}

// ─── Trader Stats ─────────────────────────────────────────────────────────────

async fn update_trader_stats(
    user_id: Uuid,
    volume: Decimal,
    pnl: Decimal,
    db: &PgPool,
) -> anyhow::Result<()> {
    let is_win = pnl > dec!(0);
    sqlx::query!(
        r#"INSERT INTO trader_stats (user_id, total_pnl, win_count, loss_count, total_trades, total_volume)
           VALUES ($1,$2,$3,$4,1,$5)
           ON CONFLICT (user_id) DO UPDATE SET
             total_pnl   = trader_stats.total_pnl + $2,
             win_count   = trader_stats.win_count + $3,
             loss_count  = trader_stats.loss_count + $4,
             total_trades = trader_stats.total_trades + 1,
             total_volume = trader_stats.total_volume + $5,
             updated_at  = NOW()"#,
        user_id, pnl,
        if is_win { 1_i64 } else { 0_i64 },
        if is_win { 0_i64 } else { 1_i64 },
        volume
    ).execute(db).await?;
    Ok(())
}

// ─── Load State from DB (crash recovery) ─────────────────────────────────────

pub async fn load_state_from_db(
    state: &Arc<RwLock<EngineState>>,
    db: &PgPool,
) -> anyhow::Result<()> {
    tracing::info!("Loading engine state from database...");
    let mut st = state.write().await;

    // Load balances
    let balances = sqlx::query!(
        "SELECT user_id, available, locked FROM balances"
    ).fetch_all(db).await?;

    for row in &balances {
        st.balances.insert(row.user_id, Balance {
            user_id: row.user_id,
            available: row.available,
            locked: row.locked,
        });
    }

    // Load open positions
    let positions = sqlx::query!(
        "SELECT position_id, user_id, market, side, qty, entry_price, margin, leverage, liquidation_price, opened_at FROM positions"
    ).fetch_all(db).await?;

    for row in &positions {
        let market = Market::from_str(&row.market)
            .ok_or_else(|| anyhow!("Unknown market: {}", row.market))?;
        let side: Side = if row.side == "long" { Side::Long } else { Side::Short };

        st.positions.entry(row.user_id).or_insert_with(Vec::new).push(Position {
            position_id: row.position_id,
            user_id: row.user_id,
            market,
            side,
            qty: row.qty,
            entry_price: row.entry_price,
            margin: row.margin,
            leverage: row.leverage,
            liquidation_price: row.liquidation_price,
            opened_at: row.opened_at.unwrap_or_else(|| Utc::now()),
        });
    }

    tracing::info!(
        "✓ Loaded {} balances, {} positions",
        balances.len(), positions.len()
    );

    Ok(())
}

// Internal fill struct used during matching
#[derive(Debug, Clone)]
pub struct InternalFill {
    pub maker_user_id: Option<Uuid>,
    pub taker_user_id: Uuid,
    pub market: Market,
    pub price: Decimal,
    pub qty: Decimal,
    pub maker_order_id: Option<Uuid>,
    pub taker_order_id: Uuid,
    pub maker_side: Side,
    pub taker_side: Side,
}
