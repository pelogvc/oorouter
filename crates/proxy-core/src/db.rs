use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::Path;

use crate::error::{ProxyError, Result};

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(ProxyError::IoError)?;
        }

        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .map_err(|e| ProxyError::ConfigError(format!("DB connect error: {}", e)))?;

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .map_err(|e| ProxyError::ConfigError(format!("Migration error: {}", e)))?;

        Ok(Database { pool })
    }

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query!("SELECT value FROM settings WHERE key = ?", key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ProxyError::ConfigError(format!("DB error: {}", e)))?;

        Ok(row.map(|r| r.value))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query!(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))",
            key, value
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ProxyError::ConfigError(format!("DB error: {}", e)))?;

        Ok(())
    }

    pub async fn get_all_settings(&self) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query!("SELECT key, value FROM settings ORDER BY key")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ProxyError::ConfigError(format!("DB error: {}", e)))?;

        Ok(rows.into_iter().map(|r| (r.key, r.value)).collect())
    }

    pub async fn insert_token_usage(
        &self,
        model: &str,
        provider: &str,
        input_tokens: i64,
        output_tokens: i64,
        request_path: &str,
    ) -> Result<()> {
        if input_tokens < 0 || output_tokens < 0 {
            return Err(ProxyError::ConfigError(
                "token usage values must be non-negative".to_string(),
            ));
        }

        sqlx::query!(
            "INSERT INTO token_usage (model, provider, input_tokens, output_tokens, request_path) VALUES (?, ?, ?, ?, ?)",
            model, provider, input_tokens, output_tokens, request_path
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ProxyError::ConfigError(format!("DB error: {}", e)))?;

        Ok(())
    }

    pub async fn get_token_usage_summary(&self, days: i64) -> Result<Vec<TokenUsageSummary>> {
        if !(1..=3650).contains(&days) {
            return Err(ProxyError::ConfigError(format!(
                "days must be between 1 and 3650: {}",
                days
            )));
        }

        let rows = sqlx::query!(
            r#"
            SELECT 
                date(timestamp) as date,
                model,
                SUM(input_tokens) as input_tokens,
                SUM(output_tokens) as output_tokens,
                SUM(input_tokens + output_tokens) as total_tokens,
                COUNT(*) as request_count
            FROM token_usage
            WHERE timestamp >= datetime('now', '-' || ? || ' days')
            GROUP BY date(timestamp), model
            ORDER BY date DESC, model
            "#,
            days
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ProxyError::ConfigError(format!("DB error: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| TokenUsageSummary {
                date: r.date.unwrap_or_default(),
                model: r.model,
                input_tokens: r.input_tokens,
                output_tokens: r.output_tokens,
                total_tokens: r.total_tokens,
                request_count: r.request_count,
            })
            .collect())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenUsageSummary {
    pub date: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub request_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_db() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(&db_path).await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn test_settings_crud() {
        let (db, _dir) = create_test_db().await;

        db.set_setting("port", "11434").await.unwrap();
        let value = db.get_setting("port").await.unwrap();
        assert_eq!(value, Some("11434".to_string()));

        db.set_setting("port", "8080").await.unwrap();
        let value = db.get_setting("port").await.unwrap();
        assert_eq!(value, Some("8080".to_string()));

        let missing = db.get_setting("nonexistent").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_token_usage_insert() {
        let (db, _dir) = create_test_db().await;

        db.insert_token_usage("gpt-5.3-codex", "codex", 100, 50, "/api/chat")
            .await
            .unwrap();

        let summary = db.get_token_usage_summary(7).await.unwrap();
        assert!(!summary.is_empty());
        assert_eq!(summary[0].model, "gpt-5.3-codex");
        assert_eq!(summary[0].input_tokens, 100);
        assert_eq!(summary[0].output_tokens, 50);
    }

    #[tokio::test]
    async fn test_token_usage_rejects_negative_values() {
        let (db, _dir) = create_test_db().await;
        let result = db
            .insert_token_usage("gpt-5.3-codex", "codex", -1, 50, "/api/chat")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_token_usage_summary_rejects_invalid_days() {
        let (db, _dir) = create_test_db().await;
        assert!(db.get_token_usage_summary(0).await.is_err());
        assert!(db.get_token_usage_summary(3651).await.is_err());
    }

    #[tokio::test]
    async fn test_no_request_logs_table() {
        let (db, _dir) = create_test_db().await;

        let result = sqlx::query("SELECT * FROM request_logs LIMIT 1")
            .fetch_optional(db.pool())
            .await;

        assert!(result.is_err(), "request_logs table should not exist");
    }
}
