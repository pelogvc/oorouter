use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex, OnceLock, Weak},
};

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tokio::sync::Mutex as AsyncMutex;

use crate::error::{ProxyError, Result};

pub struct Database {
    pool: SqlitePool,
}

type DatabaseInitializationLocks = HashMap<PathBuf, Weak<AsyncMutex<()>>>;

static DATABASE_INITIALIZATION_LOCKS: OnceLock<StdMutex<DatabaseInitializationLocks>> =
    OnceLock::new();

impl Database {
    pub async fn new(db_path: &Path) -> Result<Self> {
        prepare_database_directory(db_path).await?;
        let database_existed = prepare_database_file(db_path)?;
        restrict_database_permissions(db_path, database_existed)?;

        let canonical_db_path = std::fs::canonicalize(db_path).map_err(ProxyError::IoError)?;
        let initialization_guard = database_initialization_lock(&canonical_db_path)
            .lock_owned()
            .await;
        let migration_file_lock = acquire_migration_file_lock(&canonical_db_path).await?;

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
        drop(migration_file_lock);
        drop(initialization_guard);

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

fn database_initialization_lock(path: &Path) -> Arc<AsyncMutex<()>> {
    let registry = DATABASE_INITIALIZATION_LOCKS
        .get_or_init(|| StdMutex::new(DatabaseInitializationLocks::new()));
    let mut locks = registry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return lock;
    }

    let lock = Arc::new(AsyncMutex::new(()));
    locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
    lock
}

fn migration_lock_path(db_path: &Path) -> PathBuf {
    let mut lock_path = db_path.as_os_str().to_os_string();
    lock_path.push(".migrate.lock");
    lock_path.into()
}

async fn acquire_migration_file_lock(db_path: &Path) -> Result<File> {
    let lock_path = migration_lock_path(db_path);
    tokio::task::spawn_blocking(move || {
        let file = open_migration_lock_file(&lock_path)?;
        file.lock()?;
        Ok::<_, std::io::Error>(file)
    })
    .await
    .map_err(|_| ProxyError::ConfigError("database migration lock task failed".to_string()))?
    .map_err(ProxyError::IoError)
}

#[cfg(unix)]
fn open_migration_lock_file(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_migration_lock_file(path: &Path) -> std::io::Result<File> {
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
}

async fn prepare_database_directory(path: &Path) -> Result<()> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    if parent.exists() {
        return Ok(());
    }

    let mut missing = Vec::new();
    let mut current = Some(parent);
    while let Some(directory) = current {
        if directory.as_os_str().is_empty() || directory.exists() {
            break;
        }
        missing.push(directory.to_path_buf());
        current = directory.parent();
    }

    for directory in missing.into_iter().rev() {
        match tokio::fs::create_dir(&directory).await {
            Ok(()) => restrict_directory_permissions(&directory)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(ProxyError::IoError(error)),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(ProxyError::IoError)
}

#[cfg(not(unix))]
fn restrict_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn prepare_database_file(path: &Path) -> Result<bool> {
    use std::os::unix::fs::OpenOptionsExt;

    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .map(|_| true)
                .map_err(ProxyError::IoError)
        }
        Err(error) => Err(ProxyError::IoError(error)),
    }
}

#[cfg(not(unix))]
fn prepare_database_file(path: &Path) -> Result<bool> {
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
    {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .map(|_| true)
                .map_err(ProxyError::IoError)
        }
        Err(error) => Err(ProxyError::IoError(error)),
    }
}

#[cfg(unix)]
fn restrict_database_permissions(path: &Path, existed: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let result = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    match result {
        Ok(()) => Ok(()),
        Err(error) if existed => {
            tracing::warn!(error = %error, "could not restrict existing database permissions");
            Ok(())
        }
        Err(error) => Err(ProxyError::IoError(error)),
    }
}

#[cfg(not(unix))]
fn restrict_database_permissions(_path: &Path, _existed: bool) -> Result<()> {
    Ok(())
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
    use std::time::Duration;

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

    #[cfg(unix)]
    #[tokio::test]
    async fn test_new_database_uses_user_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("private-data");
        let db_path = data_dir.join("proxy.db");
        let _db = Database::new(&db_path).await.unwrap();

        let directory_mode = std::fs::metadata(data_dir).unwrap().permissions().mode() & 0o777;
        let database_mode = std::fs::metadata(&db_path).unwrap().permissions().mode() & 0o777;
        let canonical_db_path = std::fs::canonicalize(&db_path).unwrap();
        let lock_mode = std::fs::metadata(migration_lock_path(&canonical_db_path))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory_mode, 0o700);
        assert_eq!(database_mode, 0o600);
        assert_eq!(lock_mode, 0o600);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_existing_shared_parent_permissions_are_preserved() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let shared_dir = dir.path().join("shared");
        std::fs::create_dir(&shared_dir).unwrap();
        std::fs::set_permissions(&shared_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        let _db = Database::new(&shared_dir.join("proxy.db")).await.unwrap();

        let directory_mode = std::fs::metadata(shared_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(directory_mode, 0o755);
    }

    #[tokio::test]
    async fn test_relative_database_path_skips_empty_parent() {
        prepare_database_directory(Path::new("proxy.db"))
            .await
            .expect("empty parent is a no-op");
    }

    #[tokio::test]
    async fn test_concurrent_database_open_or_create_succeeds() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("concurrent.db");
        let mut openers = tokio::task::JoinSet::new();

        for _ in 0..8 {
            let db_path = db_path.clone();
            openers.spawn(async move { Database::new(&db_path).await });
        }

        while let Some(opener) = openers.join_next().await {
            opener
                .expect("database opener task")
                .expect("concurrent database opener");
        }
    }

    #[tokio::test]
    async fn test_initialization_locks_are_scoped_to_the_canonical_database_path() {
        let dir = TempDir::new().unwrap();
        let first_path = dir.path().join("first.db");
        let second_path = dir.path().join("second.db");
        std::fs::write(&first_path, []).unwrap();
        std::fs::write(&second_path, []).unwrap();
        let first_path = std::fs::canonicalize(first_path).unwrap();
        let second_path = std::fs::canonicalize(second_path).unwrap();

        let first_guard = database_initialization_lock(&first_path).lock_owned().await;
        let same_path_lock = database_initialization_lock(&first_path);
        let mut same_path_waiter = tokio::spawn(async move { same_path_lock.lock_owned().await });
        assert!(
            tokio::time::timeout(Duration::from_millis(25), &mut same_path_waiter)
                .await
                .is_err()
        );

        tokio::time::timeout(
            Duration::from_secs(1),
            database_initialization_lock(&second_path).lock_owned(),
        )
        .await
        .expect("different database path is not blocked");

        drop(first_guard);
        tokio::time::timeout(Duration::from_secs(1), same_path_waiter)
            .await
            .expect("same database waiter unblocked")
            .expect("same database waiter task");
    }

    #[tokio::test]
    async fn test_migration_file_lock_is_held_until_the_file_guard_is_dropped() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("file-lock.db");
        std::fs::write(&db_path, []).unwrap();
        let db_path = std::fs::canonicalize(db_path).unwrap();
        let first_guard = acquire_migration_file_lock(&db_path)
            .await
            .expect("first migration file lock");
        let mut second_waiter =
            tokio::spawn(async move { acquire_migration_file_lock(&db_path).await });

        assert!(
            tokio::time::timeout(Duration::from_millis(25), &mut second_waiter)
                .await
                .is_err()
        );
        drop(first_guard);
        tokio::time::timeout(Duration::from_secs(1), second_waiter)
            .await
            .expect("second migration file lock unblocked")
            .expect("second migration lock task")
            .expect("second migration file lock");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_concurrent_open_uses_one_lock_for_relative_and_symlink_aliases() {
        use std::os::unix::fs::symlink;

        let current_dir = std::env::current_dir().unwrap();
        let dir = tempfile::Builder::new()
            .prefix("oorouter-db-alias-")
            .tempdir_in(&current_dir)
            .unwrap();
        let target_dir = dir.path().join("target");
        let alias_dir = dir.path().join("alias");
        std::fs::create_dir(&target_dir).unwrap();
        symlink(&target_dir, &alias_dir).unwrap();
        let absolute_path = target_dir.join("aliases.db");
        let relative_path = absolute_path
            .strip_prefix(&current_dir)
            .unwrap()
            .to_path_buf();
        let symlink_path = alias_dir.join("aliases.db");
        let mut openers = tokio::task::JoinSet::new();

        for db_path in [absolute_path, relative_path, symlink_path] {
            openers.spawn(async move { Database::new(&db_path).await });
        }

        while let Some(opener) = openers.join_next().await {
            opener
                .expect("aliased database opener task")
                .expect("aliased database opener");
        }
    }
}
