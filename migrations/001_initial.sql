-- Settings table: key-value store for app configuration
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Token usage table: tracks API token consumption
CREATE TABLE IF NOT EXISTS token_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    model TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT 'codex',
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    request_path TEXT NOT NULL DEFAULT ''
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_usage_timestamp ON token_usage(timestamp);
CREATE INDEX IF NOT EXISTS idx_usage_model ON token_usage(model);
