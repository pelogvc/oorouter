use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

use crate::{
    client_auth::{generate_api_key, ClientApiKey, ClientAuthSnapshot},
    db::Database,
    error::{ProxyError, Result},
};

const CLIENT_AUTH_SINGLETON: i64 = 1;
const MAX_KEY_NAME_CHARS: usize = 64;
const MAX_GENERATION_ATTEMPTS: usize = 32;
const REDACTED_CLIENT_API_KEY: &str = "sk-••••••••••••••••";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientApiKeySummary {
    pub id: String,
    pub name: Option<String>,
    pub redacted_value: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredClientAuthConfig {
    pub enabled: bool,
    pub key_count: usize,
    snapshot: ClientAuthSnapshot,
}

impl StoredClientAuthConfig {
    fn new(enabled: bool, keys: Vec<ClientApiKey>) -> Result<Self> {
        let key_count = keys.len();
        let snapshot = if enabled {
            ClientAuthSnapshot::enabled(keys)
                .map_err(|error| ProxyError::ConfigError(error.to_string()))?
        } else {
            ClientAuthSnapshot::disabled()
        };
        Ok(Self {
            enabled,
            key_count,
            snapshot,
        })
    }

    pub fn snapshot(&self) -> ClientAuthSnapshot {
        self.snapshot.clone()
    }
}

struct StoredClientAuthRows {
    enabled: bool,
    keys: Vec<ClientApiKey>,
    summaries: Vec<ClientApiKeySummary>,
}

#[derive(Debug, Clone)]
pub struct StoredClientAuthState {
    pub auth: StoredClientAuthConfig,
    pub keys: Vec<ClientApiKeySummary>,
}

impl StoredClientAuthState {
    fn from_rows(rows: StoredClientAuthRows) -> Result<Self> {
        Ok(Self {
            auth: StoredClientAuthConfig::new(rows.enabled, rows.keys)?,
            keys: rows.summaries,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CreatedClientApiKey {
    pub key: ClientApiKeySummary,
    pub state: StoredClientAuthState,
}

#[derive(Debug, Clone)]
pub struct DeletedClientApiKey {
    pub state: StoredClientAuthState,
    pub auto_disabled: bool,
}

fn normalize_key_name(name: Option<&str>) -> Result<Option<String>> {
    let name = name.map(str::trim).filter(|name| !name.is_empty());
    if name.is_some_and(|name| name.chars().count() > MAX_KEY_NAME_CHARS) {
        return Err(ProxyError::ConfigError(
            "client API key name must be 64 characters or fewer".to_string(),
        ));
    }
    Ok(name.map(str::to_string))
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
}

async fn load_state_in_transaction(
    transaction: &mut Transaction<'_, Sqlite>,
) -> std::result::Result<StoredClientAuthRows, sqlx::Error> {
    let enabled = sqlx::query_scalar::<_, i64>(
        "SELECT enabled FROM client_api_auth_state WHERE singleton = ?",
    )
    .bind(CLIENT_AUTH_SINGLETON)
    .fetch_optional(&mut **transaction)
    .await?
    .unwrap_or(0)
        != 0;

    let stored_keys = sqlx::query_as::<_, (String, Option<String>, String, String)>(
        "SELECT id, name, token, created_at FROM client_api_keys ORDER BY sequence DESC",
    )
    .fetch_all(&mut **transaction)
    .await?;
    let mut keys = Vec::with_capacity(stored_keys.len());
    let mut summaries = Vec::with_capacity(stored_keys.len());
    for (id, name, raw, created_at) in stored_keys {
        keys.push(ClientApiKey::parse(&raw).map_err(|_| {
            sqlx::Error::Protocol("stored client API key has an invalid format".to_string())
        })?);
        summaries.push(ClientApiKeySummary {
            id,
            name,
            redacted_value: REDACTED_CLIENT_API_KEY.to_string(),
            created_at,
        });
    }

    Ok(StoredClientAuthRows {
        enabled,
        keys,
        summaries,
    })
}

impl Database {
    pub async fn load_client_auth_state(&self) -> Result<StoredClientAuthState> {
        let mut transaction = self
            .pool()
            .begin()
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        let mut rows = load_state_in_transaction(&mut transaction)
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;

        if rows.enabled && rows.keys.is_empty() {
            sqlx::query(
                "INSERT INTO client_api_auth_state (singleton, enabled, updated_at) \
                 VALUES (?, 0, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) \
                 ON CONFLICT(singleton) DO UPDATE SET enabled = 0, updated_at = excluded.updated_at",
            )
            .bind(CLIENT_AUTH_SINGLETON)
            .execute(&mut *transaction)
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
            rows.enabled = false;
        }

        let state = StoredClientAuthState::from_rows(rows)?;

        transaction
            .commit()
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        Ok(state)
    }

    pub async fn load_client_auth_config(&self) -> Result<StoredClientAuthConfig> {
        Ok(self.load_client_auth_state().await?.auth)
    }

    pub async fn list_client_api_keys(&self) -> Result<Vec<ClientApiKeySummary>> {
        let rows = sqlx::query_as::<_, (String, Option<String>, String)>(
            "SELECT id, name, created_at FROM client_api_keys ORDER BY sequence DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;

        Ok(rows
            .into_iter()
            .map(|(id, name, created_at)| ClientApiKeySummary {
                id,
                name,
                redacted_value: REDACTED_CLIENT_API_KEY.to_string(),
                created_at,
            })
            .collect())
    }

    pub async fn reveal_client_api_key(&self, id: &str) -> Result<Option<ClientApiKey>> {
        let raw = sqlx::query_scalar::<_, String>("SELECT token FROM client_api_keys WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        raw.map(|raw| {
            ClientApiKey::parse(&raw).map_err(|_| {
                ProxyError::ConfigError("stored client API key has an invalid format".to_string())
            })
        })
        .transpose()
    }

    pub async fn create_client_api_key(&self, name: Option<&str>) -> Result<CreatedClientApiKey> {
        self.create_client_api_key_with(name, generate_api_key)
            .await
    }

    async fn create_client_api_key_with<F>(
        &self,
        name: Option<&str>,
        mut generate: F,
    ) -> Result<CreatedClientApiKey>
    where
        F: FnMut() -> ClientApiKey,
    {
        let name = normalize_key_name(name)?;

        for _ in 0..MAX_GENERATION_ATTEMPTS {
            let token = generate();
            let id = Uuid::new_v4().to_string();
            let mut transaction = self
                .pool()
                .begin()
                .await
                .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
            let insert =
                sqlx::query("INSERT INTO client_api_keys (id, name, token) VALUES (?, ?, ?)")
                    .bind(&id)
                    .bind(&name)
                    .bind(token.expose_secret())
                    .execute(&mut *transaction)
                    .await;

            match insert {
                Ok(_) => {
                    let state_rows = load_state_in_transaction(&mut transaction)
                        .await
                        .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
                    let state = StoredClientAuthState::from_rows(state_rows)?;
                    let key = state
                        .keys
                        .iter()
                        .find(|key| key.id == id)
                        .cloned()
                        .ok_or_else(|| {
                            ProxyError::ConfigError(
                                "created client API key was not returned by SQLite".to_string(),
                            )
                        })?;
                    transaction
                        .commit()
                        .await
                        .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
                    return Ok(CreatedClientApiKey { key, state });
                }
                Err(error) if is_unique_violation(&error) => {
                    transaction.rollback().await.map_err(|rollback_error| {
                        ProxyError::ConfigError(format!("DB rollback error: {rollback_error}"))
                    })?;
                }
                Err(error) => {
                    return Err(ProxyError::ConfigError(format!("DB error: {error}")));
                }
            }
        }

        Err(ProxyError::ConfigError(
            "could not generate a unique client API key".to_string(),
        ))
    }

    pub async fn set_client_auth_enabled(&self, enabled: bool) -> Result<StoredClientAuthState> {
        let mut transaction = self
            .pool()
            .begin()
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        let key_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM client_api_keys")
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        if enabled && key_count == 0 {
            return Err(ProxyError::ConfigError(
                "create a client API key before enabling authentication".to_string(),
            ));
        }

        sqlx::query(
            "INSERT INTO client_api_auth_state (singleton, enabled, updated_at) \
             VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) \
             ON CONFLICT(singleton) DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
        )
        .bind(CLIENT_AUTH_SINGLETON)
        .bind(i64::from(enabled))
        .execute(&mut *transaction)
        .await
        .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        let state_rows = load_state_in_transaction(&mut transaction)
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        let state = StoredClientAuthState::from_rows(state_rows)?;
        transaction
            .commit()
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        Ok(state)
    }

    pub async fn delete_client_api_key(
        &self,
        id: &str,
        confirmed_auto_disable: bool,
    ) -> Result<Option<DeletedClientApiKey>> {
        let mut transaction = self
            .pool()
            .begin()
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        let before_rows = load_state_in_transaction(&mut transaction)
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        let key_exists = before_rows.summaries.iter().any(|key| key.id == id);
        if !key_exists {
            transaction
                .rollback()
                .await
                .map_err(|error| ProxyError::ConfigError(format!("DB rollback error: {error}")))?;
            return Ok(None);
        }

        let auto_disabled = before_rows.enabled && before_rows.keys.len() == 1;
        if auto_disabled && !confirmed_auto_disable {
            transaction
                .rollback()
                .await
                .map_err(|error| ProxyError::ConfigError(format!("DB rollback error: {error}")))?;
            return Err(ProxyError::ConfigError(
                "confirm automatic client authentication disable before deleting the last key"
                    .to_string(),
            ));
        }

        if before_rows.keys.len() == 1 {
            sqlx::query(
                "UPDATE client_api_auth_state \
                 SET enabled = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                 WHERE singleton = ? AND enabled = 1",
            )
            .bind(CLIENT_AUTH_SINGLETON)
            .execute(&mut *transaction)
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        }

        sqlx::query("DELETE FROM client_api_keys WHERE id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        let state_rows = load_state_in_transaction(&mut transaction)
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        let state = StoredClientAuthState::from_rows(state_rows)?;
        transaction
            .commit()
            .await
            .map_err(|error| ProxyError::ConfigError(format!("DB error: {error}")))?;
        Ok(Some(DeletedClientApiKey {
            state,
            auto_disabled,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use tempfile::TempDir;

    use super::*;

    async fn create_test_db() -> (Database, TempDir) {
        let dir = TempDir::new().expect("temp directory");
        let db = Database::new(&dir.path().join("client-auth.db"))
            .await
            .expect("database");
        (db, dir)
    }

    fn test_key(fill: char) -> ClientApiKey {
        ClientApiKey::parse(&format!("sk-{}", fill.to_string().repeat(64))).expect("valid key")
    }

    #[tokio::test]
    async fn starts_disabled_and_rejects_enable_without_a_key() {
        let (db, _dir) = create_test_db().await;

        let initial = db.load_client_auth_config().await.expect("initial config");
        assert!(!initial.enabled);
        assert_eq!(initial.key_count, 0);
        assert!(db.set_client_auth_enabled(true).await.is_err());
    }

    #[tokio::test]
    async fn creates_trimmed_optional_names_and_lists_only_redacted_values_newest_first() {
        let (db, _dir) = create_test_db().await;
        let unnamed = db
            .create_client_api_key_with(Some("   "), || test_key('A'))
            .await
            .expect("unnamed key");
        let named = db
            .create_client_api_key_with(Some("  desktop client  "), || test_key('B'))
            .await
            .expect("named key");
        assert_eq!(named.state.keys.len(), 2);
        assert_eq!(named.state.keys[0], named.key);
        assert!(!named.state.auth.enabled);
        assert_eq!(named.state.auth.key_count, 2);

        let keys = db.list_client_api_keys().await.expect("key list");
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].id, named.key.id);
        assert_eq!(keys[0].name.as_deref(), Some("desktop client"));
        assert_eq!(keys[0].redacted_value, REDACTED_CLIENT_API_KEY);
        assert_eq!(keys[1].id, unnamed.key.id);
        assert_eq!(keys[1].name, None);
        assert!(!keys.iter().any(|key| key.redacted_value.contains('A')));
        assert!(!keys.iter().any(|key| key.redacted_value.contains('B')));
    }

    #[tokio::test]
    async fn allows_multiple_keys_to_share_the_same_optional_name() {
        let (db, _dir) = create_test_db().await;
        db.create_client_api_key_with(Some("shared client"), || test_key('H'))
            .await
            .expect("first key");
        let second = db
            .create_client_api_key_with(Some("shared client"), || test_key('I'))
            .await
            .expect("second key");

        assert_eq!(second.state.auth.key_count, 2);
        assert_eq!(second.state.keys.len(), 2);
        assert!(second
            .state
            .keys
            .iter()
            .all(|key| key.name.as_deref() == Some("shared client")));
    }

    #[tokio::test]
    async fn reveal_uses_stable_id_and_missing_key_returns_none() {
        let (db, _dir) = create_test_db().await;
        let created = db
            .create_client_api_key_with(None, || test_key('R'))
            .await
            .expect("created key");

        let revealed = db
            .reveal_client_api_key(&created.key.id)
            .await
            .expect("reveal")
            .expect("stored key");
        assert!(revealed.expose_secret() == test_key('R').expose_secret());
        assert!(db
            .reveal_client_api_key("missing")
            .await
            .expect("missing lookup")
            .is_none());
    }

    #[tokio::test]
    async fn retries_a_generated_token_collision() {
        let (db, _dir) = create_test_db().await;
        db.create_client_api_key_with(None, || test_key('C'))
            .await
            .expect("first key");
        let mut candidates = VecDeque::from([test_key('C'), test_key('D')]);

        let created = db
            .create_client_api_key_with(None, || candidates.pop_front().expect("candidate"))
            .await
            .expect("retried key");
        let revealed = db
            .reveal_client_api_key(&created.key.id)
            .await
            .expect("reveal")
            .expect("key");
        assert!(revealed.expose_secret() == test_key('D').expose_secret());
    }

    #[tokio::test]
    async fn rejects_names_longer_than_sixty_four_characters() {
        let (db, _dir) = create_test_db().await;
        let name = "n".repeat(65);
        let error = db
            .create_client_api_key_with(Some(&name), || test_key('N'))
            .await
            .expect_err("long name must fail");
        assert!(error.to_string().contains("64"));
    }

    #[tokio::test]
    async fn enable_disable_and_last_delete_preserve_the_invariant() {
        let (db, _dir) = create_test_db().await;
        let first = db
            .create_client_api_key_with(None, || test_key('E'))
            .await
            .expect("first key");
        let second = db
            .create_client_api_key_with(None, || test_key('F'))
            .await
            .expect("second key");
        let enabled = db.set_client_auth_enabled(true).await.expect("enable");
        assert!(enabled.auth.enabled);
        assert_eq!(enabled.auth.key_count, 2);
        assert_eq!(enabled.keys.len(), 2);

        let after_first_delete = db
            .delete_client_api_key(&first.key.id, false)
            .await
            .expect("delete")
            .expect("existing");
        assert!(!after_first_delete.auto_disabled);
        assert!(after_first_delete.state.auth.enabled);
        assert_eq!(after_first_delete.state.auth.key_count, 1);
        assert_eq!(after_first_delete.state.keys.len(), 1);

        let after_last_delete = db
            .delete_client_api_key(&second.key.id, true)
            .await
            .expect("delete")
            .expect("existing");
        assert!(after_last_delete.auto_disabled);
        assert!(!after_last_delete.state.auth.enabled);
        assert_eq!(after_last_delete.state.auth.key_count, 0);
        assert!(after_last_delete.state.keys.is_empty());
        assert!(db
            .delete_client_api_key("missing", false)
            .await
            .expect("missing delete")
            .is_none());
    }

    #[tokio::test]
    async fn last_enabled_key_delete_requires_explicit_auto_disable_confirmation() {
        let (db, _dir) = create_test_db().await;
        let created = db
            .create_client_api_key_with(None, || test_key('G'))
            .await
            .expect("key");
        db.set_client_auth_enabled(true).await.expect("enable");

        let error = db
            .delete_client_api_key(&created.key.id, false)
            .await
            .expect_err("unconfirmed auto-disable must be rejected");
        assert!(error.to_string().contains("confirm"));
        let unchanged = db
            .load_client_auth_config()
            .await
            .expect("unchanged config");
        assert!(unchanged.enabled);
        assert_eq!(unchanged.key_count, 1);

        let deleted = db
            .delete_client_api_key(&created.key.id, true)
            .await
            .expect("confirmed delete")
            .expect("existing key");
        assert!(deleted.auto_disabled);
        assert!(!deleted.state.auth.enabled);
        assert!(deleted.state.keys.is_empty());
    }

    #[tokio::test]
    async fn failed_delete_rolls_back_key_and_enabled_state() {
        let (db, _dir) = create_test_db().await;
        let created = db
            .create_client_api_key_with(None, || test_key('T'))
            .await
            .expect("key");
        db.set_client_auth_enabled(true).await.expect("enable");
        sqlx::query(
            "CREATE TRIGGER fail_client_key_delete BEFORE DELETE ON client_api_keys \
             BEGIN SELECT RAISE(ABORT, 'forced delete failure'); END",
        )
        .execute(db.pool())
        .await
        .expect("failure trigger");

        assert!(db
            .delete_client_api_key(&created.key.id, true)
            .await
            .is_err());
        let config = db.load_client_auth_config().await.expect("config");
        assert!(config.enabled);
        assert_eq!(config.key_count, 1);
    }

    #[tokio::test]
    async fn startup_normalizes_a_corrupted_enabled_state_without_keys() {
        let (db, _dir) = create_test_db().await;
        sqlx::query("DROP TRIGGER client_api_auth_requires_key_insert")
            .execute(db.pool())
            .await
            .expect("drop invariant trigger");
        sqlx::query("INSERT INTO client_api_auth_state (singleton, enabled) VALUES (1, 1)")
            .execute(db.pool())
            .await
            .expect("corrupted state");

        let normalized = db
            .load_client_auth_config()
            .await
            .expect("normalized state");
        assert!(!normalized.enabled);
        assert_eq!(normalized.key_count, 0);
        let persisted = sqlx::query_scalar::<_, i64>(
            "SELECT enabled FROM client_api_auth_state WHERE singleton = 1",
        )
        .fetch_one(db.pool())
        .await
        .expect("persisted state");
        assert_eq!(persisted, 0);
    }

    #[tokio::test]
    async fn migration_preserves_existing_settings_and_usage() {
        let dir = TempDir::new().expect("temp directory");
        let path = dir.path().join("upgrade.db");
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let pool = sqlx::SqlitePool::connect(&url).await.expect("legacy db");
        for statement in include_str!("../../../migrations/001_initial.sql")
            .split(';')
            .map(str::trim)
            .filter(|statement| !statement.is_empty())
        {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("legacy statement");
        }
        sqlx::query("INSERT INTO settings (key, value) VALUES ('port', '12345')")
            .execute(&pool)
            .await
            .expect("legacy setting");
        sqlx::query(
            "INSERT INTO token_usage (model, input_tokens, output_tokens) VALUES ('gpt-test', 2, 3)",
        )
        .execute(&pool)
        .await
        .expect("legacy usage");
        pool.close().await;

        let db = Database::new(&path).await.expect("migrated database");
        assert_eq!(
            db.get_setting("port").await.expect("setting"),
            Some("12345".to_string())
        );
        let usage = db.get_token_usage_summary(7).await.expect("usage");
        assert_eq!(usage[0].total_tokens, 5);
        assert!(db.list_client_api_keys().await.expect("keys").is_empty());
    }
}
