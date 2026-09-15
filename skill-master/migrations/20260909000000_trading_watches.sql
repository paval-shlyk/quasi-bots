-- P1 Wave A2/A3: durable watch rules + alert outbox (Telegram delivery is a separate consumer).

CREATE TABLE IF NOT EXISTS trading_watches (
    id INTEGER PRIMARY KEY,
    symbol TEXT NULL,
    rule TEXT NOT NULL,
    threshold REAL NOT NULL,
    compare TEXT NOT NULL,
    channel TEXT NOT NULL,
    cooldown_secs INTEGER NOT NULL DEFAULT 3600,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_fired_at TEXT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_trading_watches_enabled
    ON trading_watches(enabled);

CREATE INDEX IF NOT EXISTS idx_trading_watches_symbol
    ON trading_watches(symbol);

CREATE TABLE IF NOT EXISTS alert_outbox (
    id INTEGER PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    watch_id INTEGER NOT NULL,
    channel TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    delivered_telegram_at TEXT NULL,
    acked_mcp_at TEXT NULL,
    FOREIGN KEY (watch_id) REFERENCES trading_watches(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_alert_outbox_created
    ON alert_outbox(created_at);

CREATE INDEX IF NOT EXISTS idx_alert_outbox_pending_telegram
    ON alert_outbox(delivered_telegram_at);

CREATE INDEX IF NOT EXISTS idx_alert_outbox_pending_mcp
    ON alert_outbox(acked_mcp_at);
