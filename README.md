# 🥃 Liqour — Social Copy Trading Perps on Solana

> *"The first perp DEX on Solana where you can follow the best traders and auto-copy their positions."*

Built for **Superteam India Fellowship** by Harkirat Singh — Capstone Project

---

## 🎯 What is Liqour?

Liqour is a **social perpetuals exchange** on Solana. It combines:

- **GMX-style** on-chain perps (SOL, BTC, ETH with up to 50x leverage)
- **eToro-style** copy trading (follow top traders, auto-mirror positions)
- **Real-time** Pyth oracle price feed
- **Rust backend** with Tokio multi-threaded matching engine

**The gap it fills:** Drift and Zeta Markets have perps but zero social layer. eToro has copy trading but is centralized, requires KYC, and has no Solana support. Liqour is the first permissionless social perp DEX on Solana.

---

## 🏗️ Architecture

```
Pyth Oracle (1s polling)
       │
       ▼
Rust Engine (Tokio) ◄──── mpsc channel ◄──── HTTP Handlers (Axum)
  BTreeMap orderbook                               │
  Balance HashMap                                  ▼
  Position HashMap                            NeonDB (PostgreSQL)
  Copy trade logic                           users, orders, fills
  Liquidation checker                        positions, follows
       │
       ▼ broadcast channel
WebSocket clients (per-connection)
  price:{market}
  orderbook:{market}
  positions (personal)
  leaderboard
       │
       ▼
Next.js Frontend
  TradingView Charts
  Phantom Wallet Connect
  Leaderboard + Copy Trade UI
  Live Portfolio
```

**Why this architecture?** Directly implements the CEX V2 class design:
- ✅ Stateless backend (crash-safe)
- ✅ In-memory engine (fast matching)
- ✅ DB + snapshot recovery (no data loss)
- ✅ WebSocket for live updates (no polling)

---

## 🔧 Tech Stack

| Layer | Tech | Why |
|---|---|---|
| Backend Language | **Rust** | Multi-threaded, 10-100x faster than JS, memory safe |
| Web Framework | **Axum 0.7** | Tower-based, async, built-in WS support |
| Async Runtime | **Tokio** | Same runtime that powers Solana itself |
| Database | **PostgreSQL (NeonDB)** | Serverless Postgres, free tier, auto-scales |
| ORM | **SQLx** | Async, compile-time checked queries |
| Price Feed | **Pyth Network** | Solana-native oracle, sub-second updates, free |
| Frontend | **Next.js 14 + TypeScript** | App Router, SSR, type safety |
| Styling | **Tailwind CSS** | Utility-first, Binance dark theme |
| Charts | **Lightweight Charts** | TradingView library, free, no API key |
| Wallet | **Solana Wallet Adapter** | Phantom, Backpack support |
| Auth | **JWT + ed25519-dalek** | Stateless JWT, Solana signature curve |
| Concurrency | **Tokio broadcast + mpsc** | Fan-out to WS clients, engine commands |

---

## ✅ What's Built

### Backend (Rust)
- [x] Full Axum server with all routes
- [x] JWT authentication via Solana wallet signature (ed25519)
- [x] NeonDB schema — 9 tables (users, orders, fills, positions, follows, trader_stats, etc.)
- [x] In-memory BTreeMap orderbook (O(log n) matching)
- [x] Limit order matching (price-time priority)
- [x] Market order matching (best price sweep)
- [x] **Copy trade engine** — atomic inside engine task (no race conditions)
- [x] Liquidation checker on every price update
- [x] Crash recovery — loads state from DB on startup
- [x] Snapshot scheduler (every 5 mins)
- [x] Pyth price feed (1s polling, OHLCV candle builder)
- [x] WebSocket server with channel subscriptions
- [x] All REST APIs (auth, orders, positions, leaderboard, follow, markets)
- [x] Trader stats (PnL, win rate, follower count)
- [x] Paper money — 1000 USDC on signup for demo
- [x] Postman collection for all 22 endpoints

### Frontend (Next.js)
- [x] Binance-style dark UI with brand color palette
- [x] Phantom wallet animated connect flow (3-step: choose → sign → done)
- [x] Markets overview page with live Pyth prices
- [x] **Full trading page** — chart + live orderbook + order form + positions
- [x] TradingView Lightweight Charts (OHLCV candles, live updates)
- [x] Live orderbook with bid/ask depth bars
- [x] Order form — Long/Short, Limit/Market, leverage 1-50x
- [x] Open positions table with unrealized PnL
- [x] **Leaderboard** — 4 sort modes (PnL, win rate, volume, followers)
- [x] Trader profile cards with open positions
- [x] **Copy trade modal** — set allocation, auto-mirrors positions
- [x] Portfolio page — positions + trade history + who you're copying
- [x] WebSocket live updates everywhere
- [x] System Design PDF (PRD/BRD)

---

## ❌ What's NOT Built (Honest)

### Critical for Production (not done)
- [ ] **Anchor smart contract** — No actual on-chain settlement. Currently all trades are off-chain (CEX-style). Real Solana perps need an Anchor program for the vault and position management
- [ ] **On-chain USDC deposits** — Users currently get paper money (1000 USDC). Real deposits via SPL token transfers are not implemented
- [ ] **Funding rate** — Formula is stubbed (0.01% hardcoded). Real funding = (mark price - index price) / index price per hour

### Important but Deferred
- [ ] **Rust build fix** — `sqlx::query!` fails at compile-time without live DB. Fix: run `cargo sqlx prepare` to generate `.sqlx/` cache, then set `SQLX_OFFLINE=true`. Alternatively switch to `sqlx::query_as` with runtime checking
- [ ] **Rate limiting** — No request throttling (Tower middleware needed)
- [ ] **Order book persistence** — Open limit orders are lost on crash (only loaded from DB at startup, matches after crash not replayed from queue)
- [ ] **Mobile responsive UI** — Desktop-first, not optimized for mobile
- [ ] **PnL shareable card** — Canvas/OG image generation not done

### V2 Scope (Future)
- [ ] Options layer (calls/puts on top of orderbook)
- [ ] Trader reputation NFT (on-chain trading history)
- [ ] Governance token (LIQR)
- [ ] Cross-margin (vs isolated margin per trade)
- [ ] Redis Streams queue (for true horizontal engine scaling)
- [ ] Mobile app (React Native)

---

## 🚀 Setup

### Prerequisites
- Rust (stable)
- Bun (for frontend)
- NeonDB account (free at neon.tech)
- Upstash Redis (free at upstash.com) — optional

### Backend

```bash
cd liqour-rust

# 1. Copy env
cp .env.example .env
# Fill DATABASE_URL from NeonDB, JWT_SECRET (any 64+ char string)

# 2. Run DB schema (MUST do before cargo build)
psql $DATABASE_URL -f schema.sql

# 3. Generate sqlx offline cache (fixes compile errors)
DATABASE_URL=your_neon_url cargo sqlx prepare

# 4. Build and run
SQLX_OFFLINE=true cargo run --release
```

**If `cargo sqlx prepare` fails**, quick fix — add this to `.cargo/config.toml`:
```toml
[env]
SQLX_OFFLINE = "true"
```
Then in `.sqlx/` folder create empty `query-*.json` files per error. 
OR just run against a local Postgres first.

### Frontend

```bash
cd liqour-frontend
cp .env.example .env.local
# Set NEXT_PUBLIC_API_URL=http://localhost:3000
# Set NEXT_PUBLIC_WS_URL=ws://localhost:3000/ws

bun install
bun run dev
# Opens at http://localhost:3001
```

### Deploy on Render (Backend)

1. Connect GitHub repo
2. Build command: `cargo build --release`
3. Start command: `./target/release/liqour`
4. Env vars: `DATABASE_URL`, `JWT_SECRET`, `SQLX_OFFLINE=true`
5. Run schema.sql on NeonDB first

### Deploy on Vercel (Frontend)

1. Connect GitHub repo
2. Framework: Next.js (auto-detected)
3. Env vars: `NEXT_PUBLIC_API_URL`, `NEXT_PUBLIC_WS_URL`
4. Deploy

---

## 📡 API Reference

| Method | Endpoint | Auth | Description |
|---|---|---|---|
| GET | `/auth/nonce` | No | Get message to sign |
| POST | `/auth/login` | No | Verify wallet signature, get JWT |
| PUT | `/auth/username` | Yes | Set display name |
| GET | `/markets` | No | All markets with live prices |
| GET | `/markets/:m/candles` | No | OHLCV for chart |
| GET | `/markets/:m/trades` | No | Recent fills |
| POST | `/orders` | Yes | Place order |
| DELETE | `/orders/:id` | Yes | Cancel order |
| GET | `/orders` | Yes | My order history |
| GET | `/positions` | Yes | Open positions + PnL |
| GET | `/positions/history` | Yes | Trade history |
| GET | `/leaderboard` | No | Top traders |
| GET | `/leaderboard/:id` | No | Trader profile |
| POST | `/follow` | Yes | Start copy trading |
| DELETE | `/follow/:id` | Yes | Stop copy trading |
| GET | `/follow/following` | Yes | Who I'm copying |
| GET | `/follow/followers` | Yes | My followers |
| WS | `/ws` | No | Real-time stream |

Import `Liqour.postman_collection.json` to test all endpoints.

---

## 🔌 WebSocket Usage

```javascript
const ws = new WebSocket('ws://localhost:3000/ws')

// Auth (for personal events)
ws.send(JSON.stringify({ type: 'AUTH', userId: 'your-user-id' }))

// Subscribe to channels
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'price:SOL' }))
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'orderbook:SOL' }))
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'leaderboard' }))
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'positions' }))

// Events received:
// PRICE_UPDATE, ORDERBOOK_UPDATE, FILL, POSITION_UPDATE, LEADERBOARD_UPDATE
```

---

## 💡 Copy Trade — How It Works

```
1. User A places Long SOL 10x → engine receives via mpsc channel
2. Engine matches order → fill created
3. Engine checks DB: "does anyone follow User A?"
4. For each follower with copy_amount set:
   follower_qty = (copy_amount / leader_position_value) * leader_qty
5. Engine places copy order for follower → same matching loop (ATOMIC)
6. Both users receive POSITION_UPDATE via WebSocket
```

**Why atomic?** Copy trade happens INSIDE the engine task (single consumer). 
If done in HTTP handler → race condition (leader fills, price moves, follower gets worse entry).
Inside engine → both orders processed in same lock cycle.

---

## 👥 Team

Built solo for Superteam India Fellowship Capstone — 1 week build.

---

## 📄 License

MIT