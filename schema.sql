-- schema.sql — Run once on NeonDB
-- psql $DATABASE_URL -f schema.sql

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE IF NOT EXISTS users (
  user_id       UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  wallet_address VARCHAR(64) UNIQUE NOT NULL,
  username       VARCHAR(32) UNIQUE,
  created_at     TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS balances (
  user_id    UUID PRIMARY KEY REFERENCES users(user_id),
  available  DECIMAL(18,6) NOT NULL DEFAULT 0,
  locked     DECIMAL(18,6) NOT NULL DEFAULT 0,
  updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS markets (
  market_id         SERIAL PRIMARY KEY,
  market_slug       VARCHAR(10) UNIQUE NOT NULL,
  index_price       DECIMAL(18,6) DEFAULT 0,
  last_traded_price DECIMAL(18,6) DEFAULT 0,
  funding_rate      DECIMAL(10,8) DEFAULT 0,
  updated_at        TIMESTAMPTZ DEFAULT NOW()
);
INSERT INTO markets (market_slug) VALUES ('SOL'),('BTC'),('ETH') ON CONFLICT DO NOTHING;

CREATE TABLE IF NOT EXISTS orders (
  order_id             UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  user_id              UUID NOT NULL REFERENCES users(user_id),
  market               VARCHAR(10) NOT NULL,
  side                 VARCHAR(10) NOT NULL,
  type                 VARCHAR(10) NOT NULL,
  price                DECIMAL(18,6),
  qty                  DECIMAL(18,6) NOT NULL,
  filled_qty           DECIMAL(18,6) NOT NULL DEFAULT 0,
  margin               DECIMAL(18,6) NOT NULL,
  leverage             INTEGER NOT NULL DEFAULT 1,
  status               VARCHAR(20) NOT NULL DEFAULT 'open',
  is_copy_order        BOOLEAN NOT NULL DEFAULT FALSE,
  copied_from_user_id  UUID,
  created_at           TIMESTAMPTZ DEFAULT NOW(),
  updated_at           TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_orders_user   ON orders(user_id);
CREATE INDEX IF NOT EXISTS idx_orders_market ON orders(market, status);

CREATE TABLE IF NOT EXISTS positions (
  position_id       UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  user_id           UUID NOT NULL REFERENCES users(user_id),
  market            VARCHAR(10) NOT NULL,
  side              VARCHAR(10) NOT NULL,
  qty               DECIMAL(18,6) NOT NULL,
  entry_price       DECIMAL(18,6) NOT NULL,
  margin            DECIMAL(18,6) NOT NULL,
  leverage          INTEGER NOT NULL DEFAULT 1,
  liquidation_price DECIMAL(18,6) NOT NULL,
  opened_at         TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_positions_user ON positions(user_id);

CREATE TABLE IF NOT EXISTS fills (
  fill_id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  maker_user_id   UUID REFERENCES users(user_id),
  taker_user_id   UUID REFERENCES users(user_id),
  market          VARCHAR(10) NOT NULL,
  price           DECIMAL(18,6) NOT NULL,
  qty             DECIMAL(18,6) NOT NULL,
  maker_order_id  UUID,
  taker_order_id  UUID,
  pnl             DECIMAL(18,6) DEFAULT 0,
  created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_fills_maker  ON fills(maker_user_id);
CREATE INDEX IF NOT EXISTS idx_fills_taker  ON fills(taker_user_id);
CREATE INDEX IF NOT EXISTS idx_fills_market ON fills(market);

CREATE TABLE IF NOT EXISTS follows (
  follow_id   UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  follower_id UUID NOT NULL REFERENCES users(user_id),
  leader_id   UUID NOT NULL REFERENCES users(user_id),
  copy_amount DECIMAL(18,6) NOT NULL,
  is_active   BOOLEAN NOT NULL DEFAULT TRUE,
  created_at  TIMESTAMPTZ DEFAULT NOW(),
  UNIQUE(follower_id, leader_id)
);
CREATE INDEX IF NOT EXISTS idx_follows_follower ON follows(follower_id);
CREATE INDEX IF NOT EXISTS idx_follows_leader   ON follows(leader_id);

CREATE TABLE IF NOT EXISTS trader_stats (
  user_id       UUID PRIMARY KEY REFERENCES users(user_id),
  total_pnl     DECIMAL(18,6) NOT NULL DEFAULT 0,
  win_count     BIGINT NOT NULL DEFAULT 0,
  loss_count    BIGINT NOT NULL DEFAULT 0,
  total_trades  BIGINT NOT NULL DEFAULT 0,
  total_volume  DECIMAL(18,6) NOT NULL DEFAULT 0,
  follower_count BIGINT NOT NULL DEFAULT 0,
  updated_at    TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS engine_snapshots (
  snapshot_id   SERIAL PRIMARY KEY,
  snapshot_data JSONB NOT NULL,
  created_at    TIMESTAMPTZ DEFAULT NOW()
);
