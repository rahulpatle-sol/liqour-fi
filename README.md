# 🥃 Liqour — Social Copy Trading Perps on Solana

> **The first permissionless social perpetuals exchange on Solana.** Copy the best traders, auto-mirror their positions, trade with up to 50× leverage — no KYC, no intermediaries.

[![Backend](https://img.shields.io/badge/backend-live-1D9E75?style=flat-square)](https://liqour-fi.onrender.com)
[![Frontend](https://img.shields.io/badge/frontend-live-1D9E75?style=flat-square)](https://liqour-fi.vercel.app)
[![License](https://img.shields.io/badge/license-MIT-EF9F27?style=flat-square)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-CE7B1B?style=flat-square)](https://www.rust-lang.org)
[![Superteam Fellowship](https://img.shields.io/badge/Superteam-India%20Fellowship%20Capstone-9D4EDD?style=flat-square)](https://superteam.fun)

**Live:** [liqour-fi.vercel.app](https://liqour-fi.vercel.app) &nbsp;|&nbsp; **API:** [liqour-fi.onrender.com](https://liqour-fi.onrender.com)

---

## What is Liqour?

| Existing Product | What it does | What it lacks |
|---|---|---|
| Drift / Zeta Markets | On-chain perps on Solana | Zero social layer |
| eToro | Copy trading | Centralized, KYC, no Solana |
| **Liqour** | **Perps + Copy trading** | **Nothing — this is the gap** |

Liqour combines GMX-style perpetual trading with eToro-style social copy trading, built on a Rust backend with a Tokio multi-threaded matching engine. Traders can go long or short on SOL, BTC, and ETH at up to 50× leverage, follow top performers on the leaderboard, and auto-mirror their positions atomically.

---

## Architecture

```
Pyth Oracle (1s polling)
       │
       ▼
Rust Engine (Tokio) ◄──── mpsc channel ◄──── HTTP Handlers (Axum)
  BTreeMap orderbook                               │
  Balance HashMap                                  ▼
  Position HashMap                          NeonDB (PostgreSQL)
  Copy trade logic                   users, orders, fills, positions,
  Liquidation checker                       follows, trader_stats
       │
       ▼ broadcast channel
WebSocket clients (per-connection)
  price:{market} · orderbook:{market}
  positions · leaderboard
       │
       ▼
Next.js 14 Frontend
  TradingView Lightweight Charts · Phantom Wallet Connect
  Leaderboard + Copy Trade UI · Live Portfolio
```

**Design decisions:**
- **Stateless backend** — crash-safe, horizontally scalable
- **In-memory engine** — sub-millisecond order matching
- **DB + snapshot recovery** — no data loss on restart
- **WebSocket fan-out** — zero polling on the frontend

---

## Tech Stack

| Layer | Tech | Why |
|---|---|---|
| Backend language | **Rust** | 10–100× faster than JS, memory safe, zero GC pauses |
| Web framework | **Axum 0.7** | Tower-based, async, built-in WebSocket support |
| Async runtime | **Tokio** | Same runtime powering Solana itself |
| Database | **PostgreSQL (NeonDB)** | Serverless Postgres, free tier, auto-scales |
| ORM | **SQLx** | Async, compile-time checked queries |
| Price feed | **Pyth Network** | Solana-native oracle, sub-second updates, free |
| Frontend | **Next.js 14 + TypeScript** | App Router, SSR, full type safety |
| Styling | **Tailwind CSS** | Binance dark theme, utility-first |
| Charts | **Lightweight Charts** | TradingView library, free, no API key |
| Wallet | **Solana Wallet Adapter** | Phantom + Backpack support |
| Auth | **JWT + ed25519-dalek** | Stateless JWT on Solana's signature curve — no passwords |
| Concurrency | **Tokio broadcast + mpsc** | Fan-out to WS clients + engine commands |

---

## What's Built

### Backend (Rust)
- [x] Full Axum server — all routes implemented
- [x] JWT auth via Solana wallet signature (ed25519) — no passwords, no KYC
- [x] NeonDB schema — 9 tables (users, orders, fills, positions, follows, trader_stats, markets, candles, snapshots)
- [x] In-memory BTreeMap orderbook — O(log n) price-time priority matching
- [x] Limit order + market order matching with best-price sweep
- [x] **Copy trade engine** — atomic inside engine task (zero race conditions)
- [x] Liquidation checker fires on every Pyth price update
- [x] Crash recovery — full state loaded from DB on startup
- [x] Snapshot scheduler every 5 minutes
- [x] Pyth price feed — 1s polling, OHLCV candle builder
- [x] WebSocket server with per-channel subscriptions
- [x] Trader stats (PnL, win rate, follower count, volume)
- [x] Paper money — 1000 USDC on signup for instant demo
- [x] Postman collection for all 22 endpoints

### Frontend (Next.js)
- [x] Binance-style dark UI with brand color system
- [x] Phantom wallet animated connect flow (3 steps: choose → sign → done)
- [x] Markets overview — live Pyth prices
- [x] Full trading page — chart + live orderbook + order form + open positions
- [x] TradingView Lightweight Charts — OHLCV candles, live updates
- [x] Live orderbook with bid/ask depth visualization
- [x] Order form — Long/Short, Limit/Market, 1–50× leverage slider
- [x] Open positions table with unrealized PnL
- [x] **Leaderboard** — 4 sort modes (PnL, win rate, volume, followers)
- [x] Trader profile cards with open positions visible
- [x] **Copy trade modal** — set allocation, auto-mirrors all positions
- [x] Portfolio page — positions + trade history + who you're copying
- [x] WebSocket live updates across all pages

---

## Copy Trade Engine — How It Works

```
1. User A places Long SOL 10× → engine receives via mpsc channel
2. Engine matches order → fill created
3. Engine queries DB: "does anyone follow User A?"
4. For each follower with copy_amount set:
       follower_qty = (copy_amount / leader_position_value) × leader_qty
5. Engine places copy order for follower → same matching loop (ATOMIC)
6. Both users receive POSITION_UPDATE via WebSocket
```

**Why atomic?** Copy trade runs inside the engine task — the single mpsc consumer. If it ran in the HTTP handler, there's a race: leader fills, price moves, follower gets a worse entry. Inside the engine, both orders are processed in the same tick — same price, same moment.

---

## API Reference

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

Import `Liqour.postman_collection.json` to test all endpoints locally.

### WebSocket Usage

```javascript
const ws = new WebSocket('wss://liqour-fi.onrender.com/ws')

// Auth for personal events
ws.send(JSON.stringify({ type: 'AUTH', userId: 'your-user-id' }))

// Subscribe to channels
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'price:SOL' }))
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'orderbook:SOL' }))
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'leaderboard' }))
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'positions' }))

// Events you'll receive:
// PRICE_UPDATE, ORDERBOOK_UPDATE, FILL, POSITION_UPDATE, LEADERBOARD_UPDATE
```

---

## What's NOT Built (Honest)

This is a 1-week solo build. The following limitations are documented transparently.

### Production Blockers
- [ ] **Anchor smart contract** — No on-chain settlement. All trades are off-chain (CEX-style). Real Solana perps need an Anchor program for vault + position management
- [ ] **On-chain USDC deposits** — Users get paper money (1000 USDC on signup). SPL token deposits not implemented
- [ ] **Real funding rate** — Currently hardcoded at 0.01%. Real formula: `(mark_price - index_price) / index_price` per hour

### Known Issues
- [ ] **SQLx build** — `sqlx::query!` fails at compile-time without a live DB connection. Fix: run `cargo sqlx prepare` to generate `.sqlx/` cache, then set `SQLX_OFFLINE=true`
- [ ] **Rate limiting** — No request throttling (Tower middleware needed)
- [ ] **Orderbook persistence** — Open limit orders lost on crash; only DB-recovered orders are replayed at startup

### Deferred
- [ ] Mobile responsive UI (desktop-first)
- [ ] PnL shareable card (OG image generation)

### V2 Scope
- [ ] Options layer (calls/puts)
- [ ] Trader reputation NFT (on-chain history)
- [ ] Governance token (LIQR)
- [ ] Cross-margin (vs current isolated margin per position)
- [ ] Redis Streams for horizontal engine scaling
- [ ] Mobile app (React Native)

---

## Setup

### Prerequisites
- Rust (stable) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Bun — `curl -fsSL https://bun.sh/install | bash`
- [NeonDB](https://neon.tech) account (free tier works)

### Backend

```bash
cd liqour-rust

# 1. Configure environment
cp .env.example .env
# Fill in:
#   DATABASE_URL=postgres://... (from NeonDB dashboard)
#   JWT_SECRET=<any 64+ character random string>

# 2. Run DB schema
psql $DATABASE_URL -f schema.sql

# 3. Generate SQLx offline cache (required for cargo build)
DATABASE_URL=your_neon_url cargo sqlx prepare

# 4. Build and run
SQLX_OFFLINE=true cargo run --release
# Server starts at http://localhost:3000
```

> **If `cargo sqlx prepare` fails:** add `SQLX_OFFLINE = "true"` to `.cargo/config.toml` under `[env]`, or run against a local Postgres instance first.

### Frontend

```bash
cd liqour-frontend

cp .env.example .env.local
# NEXT_PUBLIC_API_URL=http://localhost:3000
# NEXT_PUBLIC_WS_URL=ws://localhost:3000/ws

bun install
bun run dev
# Opens at http://localhost:3001
```

### Deploy

**Backend on Render:**
1. Connect GitHub repo
2. Build command: `cargo build --release`
3. Start command: `./target/release/liqour`
4. Environment variables: `DATABASE_URL`, `JWT_SECRET`, `SQLX_OFFLINE=true`
5. Run `schema.sql` on NeonDB before first deploy

**Frontend on Vercel:**
1. Connect GitHub repo — Next.js auto-detected
2. Environment variables: `NEXT_PUBLIC_API_URL`, `NEXT_PUBLIC_WS_URL`
3. Deploy

---

## Team

Built solo for **Superteam India Fellowship Capstone** — 1 week build.

---

## License

MIT
