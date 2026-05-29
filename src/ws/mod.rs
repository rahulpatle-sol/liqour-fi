// src/ws/mod.rs
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::engine::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = state.event_tx.subscribe();
    let mut subscriptions: HashSet<String> = HashSet::new();
    let mut user_id: Option<String> = None;

    // Send welcome
    let _ = sender.send(Message::Text(
        json!({ "type": "CONNECTED", "data": { "ts": chrono::Utc::now().timestamp_millis() } }).to_string()
    )).await;

    loop {
        tokio::select! {
            // Incoming WS message from client
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(val) = serde_json::from_str::<Value>(&text) {
                            match val.get("type").and_then(|t| t.as_str()) {
                                Some("AUTH") => {
                                    user_id = val.get("userId").and_then(|v| v.as_str()).map(String::from);
                                }
                                Some("SUBSCRIBE") => {
                                    if let Some(ch) = val.get("channel").and_then(|v| v.as_str()) {
                                        subscriptions.insert(ch.to_string());
                                        let _ = sender.send(Message::Text(
                                            json!({ "type": "SUBSCRIBED", "channel": ch }).to_string()
                                        )).await;
                                    }
                                }
                                Some("UNSUBSCRIBE") => {
                                    if let Some(ch) = val.get("channel").and_then(|v| v.as_str()) {
                                        subscriptions.remove(ch);
                                    }
                                }
                                Some("PING") => {
                                    let _ = sender.send(Message::Text(
                                        json!({ "type": "PONG", "ts": chrono::Utc::now().timestamp_millis() }).to_string()
                                    )).await;
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    _ => {}
                }
            }

            // Engine broadcast events
            event = event_rx.recv() => {
                match event {
                    Ok(ws_event) => {
                        let (should_send, msg) = should_forward(&ws_event, &subscriptions, &user_id);
                        if should_send {
                            if sender.send(Message::Text(msg)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WS client lagged by {} messages", n);
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

fn should_forward(
    event: &crate::types::WsEvent,
    subs: &HashSet<String>,
    user_id: &Option<String>,
) -> (bool, String) {
    use crate::types::WsEvent::*;

    let (channel, is_personal_user) = match event {
        PriceUpdate { market, .. }    => (format!("price:{}", market), None),
        OrderbookUpdate(snap)          => (format!("orderbook:{}", snap.market.as_str()), None),
        Fill(f)                        => (format!("market:{}", f.market), None),
        PositionUpdate { user_id: uid, .. } => (
            format!("positions"),
            Some(uid.to_string())
        ),
        LeaderboardUpdate              => ("leaderboard".to_string(), None),
    };

    let subscribed = subs.contains(&channel);

    // For personal events, only send if this WS connection authenticated as that user
    let user_matches = is_personal_user.map_or(true, |uid| {
        user_id.as_deref() == Some(&uid)
    });

    if subscribed && user_matches {
        let msg = serde_json::json!({
            "type": event_type_name(event),
            "data": serde_json::to_value(event).unwrap_or_default(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
        }).to_string();
        (true, msg)
    } else {
        (false, String::new())
    }
}

fn event_type_name(event: &crate::types::WsEvent) -> &'static str {
    use crate::types::WsEvent::*;
    match event {
        PriceUpdate { .. }    => "PRICE_UPDATE",
        OrderbookUpdate(_)    => "ORDERBOOK_UPDATE",
        Fill(_)               => "FILL",
        PositionUpdate { .. } => "POSITION_UPDATE",
        LeaderboardUpdate     => "LEADERBOARD_UPDATE",
    }
}
