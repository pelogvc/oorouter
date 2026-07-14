CREATE TABLE IF NOT EXISTS client_api_keys (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    name TEXT CHECK(name IS NULL OR length(name) <= 64),
    token TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_client_api_keys_created
    ON client_api_keys(sequence DESC);

CREATE TABLE IF NOT EXISTS client_api_auth_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TRIGGER IF NOT EXISTS client_api_auth_requires_key_insert
BEFORE INSERT ON client_api_auth_state
WHEN NEW.enabled = 1 AND NOT EXISTS (SELECT 1 FROM client_api_keys)
BEGIN
    SELECT RAISE(ABORT, 'client API authentication requires at least one key');
END;

CREATE TRIGGER IF NOT EXISTS client_api_auth_requires_key_update
BEFORE UPDATE OF enabled ON client_api_auth_state
WHEN NEW.enabled = 1 AND NOT EXISTS (SELECT 1 FROM client_api_keys)
BEGIN
    SELECT RAISE(ABORT, 'client API authentication requires at least one key');
END;

CREATE TRIGGER IF NOT EXISTS client_api_auth_prevents_last_key_delete
BEFORE DELETE ON client_api_keys
WHEN (SELECT enabled FROM client_api_auth_state WHERE singleton = 1) = 1
  AND (SELECT COUNT(*) FROM client_api_keys) = 1
BEGIN
    SELECT RAISE(ABORT, 'disable client API authentication before deleting the last key');
END;
