// src/engine/orderbook.rs
use std::collections::BTreeMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::types::*;
use super::InternalFill;

// BTreeMap gives us sorted order automatically
// bids: highest price first (we iterate in reverse)
// asks: lowest price first (normal iteration)

pub struct Orderbook {
    pub bids: BTreeMap<Decimal, BookLevel>,  // price -> level
    pub asks: BTreeMap<Decimal, BookLevel>,
    pub last_traded_price: Decimal,
    pub index_price: Decimal,
    pub funding_rate: Decimal,
}

pub struct BookLevel {
    pub orders: Vec<LevelOrder>,
    pub total_qty: Decimal,
}

#[derive(Clone)]
pub struct LevelOrder {
    pub order_id: Uuid,
    pub user_id: Uuid,
    pub qty: Decimal,
    pub filled_qty: Decimal,
    pub leverage: i32,
    pub margin: Decimal,
}

impl Orderbook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_traded_price: dec!(0),
            index_price: dec!(0),
            funding_rate: dec!(0),
        }
    }

    pub fn remove_order(&mut self, order_id: Uuid, user_id: Uuid) -> Option<Decimal> {
        for book in [&mut self.bids, &mut self.asks] {
            let mut found_price: Option<Decimal> = None;
            let mut remaining = dec!(0);

            for (price, level) in book.iter_mut() {
                if let Some(idx) = level.orders.iter().position(|o| o.order_id == order_id && o.user_id == user_id) {
                    let order = &level.orders[idx];
                    remaining = order.qty - order.filled_qty;
                    level.total_qty -= remaining;
                    level.orders.remove(idx);
                    found_price = Some(*price);
                    break;
                }
            }

            if let Some(price) = found_price {
                if book.get(&price).map_or(false, |l| l.orders.is_empty()) {
                    book.remove(&price);
                }
                return Some(remaining);
            }
        }
        None
    }
}

pub fn add_to_book(order: &Order, ob: &mut Orderbook) {
    let remaining = order.qty - order.filled_qty;
    if remaining <= dec!(0) { return; }

    let level_order = LevelOrder {
        order_id: order.order_id,
        user_id: order.user_id,
        qty: order.qty,
        filled_qty: order.filled_qty,
        leverage: order.leverage,
        margin: order.margin,
    };

    let book = if order.side == Side::Long { &mut ob.bids } else { &mut ob.asks };
    let level = book.entry(order.price).or_insert(BookLevel {
        orders: Vec::new(),
        total_qty: dec!(0),
    });
    level.orders.push(level_order);
    level.total_qty += remaining;
}

pub fn match_order(order: &mut Order, ob: &mut Orderbook, user_id: Uuid) -> Vec<InternalFill> {
    let mut fills = Vec::new();

    match order.order_type {
        OrderType::Market => match_market(order, ob, user_id, &mut fills),
        OrderType::Limit  => match_limit(order, ob, user_id, &mut fills),
    }

    fills
}

fn match_market(
    taker: &mut Order,
    ob: &mut Orderbook,
    taker_user_id: Uuid,
    fills: &mut Vec<InternalFill>,
) {
    let book = if taker.side == Side::Long { &mut ob.asks } else { &mut ob.bids };

    // Collect prices to match against
    let prices: Vec<Decimal> = if taker.side == Side::Long {
        book.keys().cloned().collect()              // asks: ascending
    } else {
        book.keys().rev().cloned().collect()        // bids: descending
    };

    let mut remaining = taker.qty - taker.filled_qty;

    'outer: for price in prices {
        if remaining <= dec!(0) { break; }

        let level = match book.get_mut(&price) {
            Some(l) => l,
            None => continue,
        };

        let mut orders_to_remove = Vec::new();

        for (i, maker) in level.orders.iter_mut().enumerate() {
            if remaining <= dec!(0) { break; }
            if maker.user_id == taker_user_id { continue; } // no self-fill

            let fill_qty = remaining.min(maker.qty - maker.filled_qty);

            // Create fill
            fills.push(InternalFill {
                maker_user_id: Some(maker.user_id),
                taker_user_id,
                market: taker.market,
                price,
                qty: fill_qty,
                maker_order_id: Some(maker.order_id),
                taker_order_id: taker.order_id,
                maker_side: opposite_side(taker.side),
                taker_side: taker.side,
            });

            maker.filled_qty += fill_qty;
            taker.filled_qty += fill_qty;
            level.total_qty -= fill_qty;
            remaining -= fill_qty;

            ob.last_traded_price = price;

            if maker.filled_qty >= maker.qty {
                orders_to_remove.push(i);
            }
        }

        // Remove filled orders (reverse order to preserve indices)
        for i in orders_to_remove.into_iter().rev() {
            level.orders.remove(i);
        }
    }

    // Remove empty levels
    book.retain(|_, level| !level.orders.is_empty());

    taker.status = if taker.filled_qty >= taker.qty {
        OrderStatus::Filled
    } else if taker.filled_qty > dec!(0) {
        OrderStatus::Partial
    } else {
        OrderStatus::Open
    };
}

fn match_limit(
    taker: &mut Order,
    ob: &mut Orderbook,
    taker_user_id: Uuid,
    fills: &mut Vec<InternalFill>,
) {
    let book = if taker.side == Side::Long { &mut ob.asks } else { &mut ob.bids };

    let prices: Vec<Decimal> = if taker.side == Side::Long {
        book.range(..=taker.price).map(|(&p, _)| p).collect()
    } else {
        book.range(taker.price..).map(|(&p, _)| p).rev().collect()
    };

    let mut remaining = taker.qty - taker.filled_qty;

    for price in prices {
        if remaining <= dec!(0) { break; }

        let level = match book.get_mut(&price) {
            Some(l) => l,
            None => continue,
        };

        let mut orders_to_remove = Vec::new();

        for (i, maker) in level.orders.iter_mut().enumerate() {
            if remaining <= dec!(0) { break; }
            if maker.user_id == taker_user_id { continue; }

            let fill_qty = remaining.min(maker.qty - maker.filled_qty);

            fills.push(InternalFill {
                maker_user_id: Some(maker.user_id),
                taker_user_id,
                market: taker.market,
                price,
                qty: fill_qty,
                maker_order_id: Some(maker.order_id),
                taker_order_id: taker.order_id,
                maker_side: opposite_side(taker.side),
                taker_side: taker.side,
            });

            maker.filled_qty += fill_qty;
            taker.filled_qty += fill_qty;
            level.total_qty -= fill_qty;
            remaining -= fill_qty;

            ob.last_traded_price = price;

            if maker.filled_qty >= maker.qty {
                orders_to_remove.push(i);
            }
        }

        for i in orders_to_remove.into_iter().rev() {
            level.orders.remove(i);
        }
    }

    book.retain(|_, level| !level.orders.is_empty());

    taker.status = if taker.filled_qty >= taker.qty {
        OrderStatus::Filled
    } else if taker.filled_qty > dec!(0) {
        OrderStatus::Partial
    } else {
        OrderStatus::Open
    };
}

pub fn get_snapshot(ob: &Orderbook, market: Market) -> OrderbookSnapshot {
    let bids: Vec<OrderbookLevel> = ob.bids.iter().rev().take(20)
        .map(|(price, level)| OrderbookLevel { price: *price, qty: level.total_qty })
        .collect();

    let asks: Vec<OrderbookLevel> = ob.asks.iter().take(20)
        .map(|(price, level)| OrderbookLevel { price: *price, qty: level.total_qty })
        .collect();

    OrderbookSnapshot {
        market,
        bids,
        asks,
        last_traded_price: ob.last_traded_price,
        index_price: ob.index_price,
        funding_rate: ob.funding_rate,
    }
}

fn opposite_side(side: Side) -> Side {
    match side {
        Side::Long  => Side::Short,
        Side::Short => Side::Long,
    }
}
