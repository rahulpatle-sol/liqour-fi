// src/types.rs
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Market {
    Sol,
    Btc,
    Eth,
}

impl Market {
    pub fn as_str(&self) -> &'static str {
        match self {
            Market::Sol => "SOL",
            Market::Btc => "BTC",
            Market::Eth => "ETH",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "SOL" => Some(Market::Sol),
            "BTC" => Some(Market::Btc),
            "ETH" => Some(Market::Eth),
            _ => None,
        }
    }
    pub fn all() -> Vec<Market> {
        vec![Market::Sol, Market::Btc, Market::Eth]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Long,
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    Limit,
    Market,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Open,
    Filled,
    Partial,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: Uuid,
    pub user_id: Uuid,
    pub market: Market,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Decimal,
    pub qty: Decimal,
    pub filled_qty: Decimal,
    pub margin: Decimal,
    pub leverage: i32,
    pub status: OrderStatus,
    pub is_copy_order: bool,
    pub copied_from_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub position_id: Uuid,
    pub user_id: Uuid,
    pub market: Market,
    pub side: Side,
    pub qty: Decimal,
    pub entry_price: Decimal,
    pub margin: Decimal,
    pub leverage: i32,
    pub liquidation_price: Decimal,
    pub opened_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub user_id: Uuid,
    pub available: Decimal,
    pub locked: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub fill_id: Uuid,
    pub maker_user_id: Option<Uuid>,
    pub taker_user_id: Option<Uuid>,
    pub market: Market,
    pub price: Decimal,
    pub qty: Decimal,
    pub maker_order_id: Option<Uuid>,
    pub taker_order_id: Option<Uuid>,
    pub pnl: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookLevel {
    pub price: Decimal,
    pub qty: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookSnapshot {
    pub market: Market,
    pub bids: Vec<OrderbookLevel>,
    pub asks: Vec<OrderbookLevel>,
    pub last_traded_price: Decimal,
    pub index_price: Decimal,
    pub funding_rate: Decimal,
}

// ─── Engine Commands (HTTP handlers → Engine) ─────────────────────────────────

#[derive(Debug)]
pub struct PlaceOrderCmd {
    pub user_id: Uuid,
    pub market: Market,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Decimal,
    pub qty: Decimal,
    pub leverage: i32,
    pub is_copy_order: bool,
    pub copied_from_user_id: Option<Uuid>,
    pub resp: tokio::sync::oneshot::Sender<anyhow::Result<Order>>,
}

#[derive(Debug)]
pub struct CancelOrderCmd {
    pub user_id: Uuid,
    pub order_id: Uuid,
    pub resp: tokio::sync::oneshot::Sender<anyhow::Result<()>>,
}

#[derive(Debug)]
pub struct DepositCmd {
    pub user_id: Uuid,
    pub amount: Decimal,
    pub resp: tokio::sync::oneshot::Sender<anyhow::Result<()>>,
}

#[derive(Debug)]
pub enum EngineCmd {
    PlaceOrder(PlaceOrderCmd),
    CancelOrder(CancelOrderCmd),
    Deposit(DepositCmd),
    UpdatePrice { market: Market, price: Decimal },
    Shutdown,
}

// ─── Engine Events (Engine → WebSocket broadcast) ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsEvent {
    PriceUpdate { market: String, price: Decimal, timestamp: i64 },
    OrderbookUpdate(OrderbookSnapshot),
    Fill(FillEvent),
    PositionUpdate { user_id: Uuid, market: String, positions: Vec<PositionWithPnl> },
    LeaderboardUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillEvent {
    pub fill_id: Uuid,
    pub market: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub maker_user_id: Option<Uuid>,
    pub taker_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionWithPnl {
    #[serde(flatten)]
    pub position: Position,
    pub unrealized_pnl: Decimal,
    pub current_price: Decimal,
}

// ─── API Request/Response types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PlaceOrderRequest {
    pub market: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub price: Option<Decimal>,
    pub qty: Decimal,
    pub leverage: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub wallet_address: String,
    pub signature: String,
    pub nonce: String,
}

#[derive(Debug, Deserialize)]
pub struct FollowRequest {
    pub leader_id: Uuid,
    pub copy_amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self { success: true, data: Some(data), error: None }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Self { success: false, data: None, error: Some(msg.into()) }
    }
}
