# Liqour 🥃 — Rust Backend

Social copy trading perps exchange. Multi-threaded Rust backend with Axum + Tokio.

## Architecture

```
Pyth Oracle (1s poll)
       │
       ▼
  Price Task ──────────────────────────────────────────────────────┐
       │                                                           │
       ▼                                                           │
Engine Task (Tokio) ← mpsc::Sender (from HTTP handlers)          │
  - in-memory orderbook (BTreeMap)                                 │
  - balance HashMap                                                 │
  - position HashMap                                                │
  - copy trade trigger                                              │
  - liquidation checker                                             │
       │                                                           │
       ▼                                                           │
broadcast::Sender<WsEvent> → WebSocket handlers (per connection)  │
       │                                                           │
       ▼                                                           │
  NeonDB (PostgreSQL)   ←──────────────────────────────────────────┘
  - persistent storage
  - crash recovery on startup
```

## Quick Start

### 1. Setup
```bash
cp .env.example .env
# Fill DATABASE_URL, REDIS_URL, JWT_SECRET
```

### 2. Run DB schema
```bash
psql $DATABASE_URL -f schema.sql
```

### 3. Run
```bash
cargo run --release
```

## ENV Variables

| Variable | Description | Required |
|---|---|---|
| `DATABASE_URL` | NeonDB connection string | ✅ |
| `JWT_SECRET` | 64+ char random string | ✅ |
| `REDIS_URL` | Redis URL (optional, future use) | ❌ |
| `PORT` | Port (default 3000) | ❌ |
| `PYTH_HERMES_URL` | Pyth API URL | ❌ |

## WebSocket Usage

Connect: `ws://localhost:3000/ws`

```javascript
const ws = new WebSocket('ws://localhost:3000/ws')

// Authenticate
ws.send(JSON.stringify({ type: 'AUTH', userId: 'your-user-id' }))

// Subscribe to channels
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'price:SOL' }))
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'orderbook:SOL' }))
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'leaderboard' }))
// For personal events (auth required):
ws.send(JSON.stringify({ type: 'SUBSCRIBE', channel: 'positions' }))
```

## GMX-Style Wallet Auth

GMX uses EVM wallets (MetaMask). Liqour uses **Solana wallets** (Phantom/Backpack).

Flow is identical conceptually:
1. Frontend requests nonce: `GET /auth/nonce?wallet=ADDRESS`
2. User signs message in wallet (Phantom `signMessage`)
3. Frontend sends signature: `POST /auth/login`
4. Server verifies ed25519 signature (Solana's curve)
5. Server returns JWT token
6. All subsequent requests use `Authorization: Bearer TOKEN`

```typescript
// Frontend wallet auth (Next.js)
const { data: wallet } = useWallet()

const login = async () => {
  const { nonce, message } = await fetch(`/auth/nonce?wallet=${wallet.publicKey}`).then(r => r.json())
  
  const messageBytes = new TextEncoder().encode(message)
  const signature = await wallet.signMessage(messageBytes)
  const sigBase58 = bs58.encode(signature)
  
  const { token } = await fetch('/auth/login', {
    method: 'POST',
    body: JSON.stringify({
      wallet_address: wallet.publicKey.toString(),
      signature: sigBase58,
      nonce,
    })
  }).then(r => r.json())
  
  localStorage.setItem('token', token)
}
```

## Postman

Import `Liqour.postman_collection.json` into Postman.

Set collection variables:
- `BASE_URL`: `http://localhost:3000`
- `TOKEN`: paste JWT after login
- `LEADER_ID`: paste any user_id from leaderboard
