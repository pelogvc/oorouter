use std::sync::Arc;

use proxy_core::{
    client_auth::ClientAuthRuntime,
    client_auth_store::{ClientApiKeySummary, StoredClientAuthState},
    db::Database,
};
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopClientAuthState {
    pub enabled: bool,
    pub keys: Vec<ClientApiKeySummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteClientApiKeyResult {
    pub state: DesktopClientAuthState,
    pub auto_disabled: bool,
}

#[derive(Clone)]
pub struct DesktopClientAuthController {
    db: Arc<Database>,
    runtime: ClientAuthRuntime,
    mutations: Arc<Mutex<()>>,
}

impl DesktopClientAuthController {
    pub fn new(db: Arc<Database>, runtime: ClientAuthRuntime) -> Self {
        Self {
            db,
            runtime,
            mutations: Arc::new(Mutex::new(())),
        }
    }

    pub async fn hydrate(db: Arc<Database>) -> Result<Self, String> {
        let config = db
            .load_client_auth_config()
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self::new(db, ClientAuthRuntime::new(config.snapshot())))
    }

    pub fn runtime(&self) -> ClientAuthRuntime {
        self.runtime.clone()
    }

    pub async fn state(&self) -> Result<DesktopClientAuthState, String> {
        let db = Arc::clone(&self.db);
        let runtime = self.runtime.clone();
        let mutations = Arc::clone(&self.mutations);
        await_mutation(tokio::spawn(async move {
            let _guard = mutations.lock().await;
            let policy_update = runtime.begin_update().await;
            let stored = db
                .load_client_auth_state()
                .await
                .map_err(|error| error.to_string())?;
            policy_update.replace(stored.auth.snapshot());
            Ok(desktop_state(stored))
        }))
        .await
    }

    pub async fn create(&self, name: Option<&str>) -> Result<DesktopClientAuthState, String> {
        let db = Arc::clone(&self.db);
        let runtime = self.runtime.clone();
        let mutations = Arc::clone(&self.mutations);
        let name = name.map(str::to_owned);
        await_mutation(tokio::spawn(async move {
            let _guard = mutations.lock().await;
            let policy_update = runtime.begin_update().await;
            let created = db
                .create_client_api_key(name.as_deref())
                .await
                .map_err(|error| error.to_string())?;
            policy_update.replace(created.state.auth.snapshot());
            Ok(desktop_state(created.state))
        }))
        .await
    }

    pub async fn reveal(&self, id: &str) -> Result<String, String> {
        let _guard = self.mutations.lock().await;
        let key = self
            .db
            .reveal_client_api_key(id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Client API key not found".to_string())?;
        Ok(key.expose_secret().to_string())
    }

    pub async fn delete(
        &self,
        id: &str,
        confirmed_auto_disable: bool,
    ) -> Result<DeleteClientApiKeyResult, String> {
        let db = Arc::clone(&self.db);
        let runtime = self.runtime.clone();
        let mutations = Arc::clone(&self.mutations);
        let id = id.to_owned();
        await_mutation(tokio::spawn(async move {
            let _guard = mutations.lock().await;
            let policy_update = runtime.begin_update().await;
            let deleted = db
                .delete_client_api_key(&id, confirmed_auto_disable)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Client API key not found".to_string())?;
            policy_update.replace(deleted.state.auth.snapshot());
            Ok(DeleteClientApiKeyResult {
                state: desktop_state(deleted.state),
                auto_disabled: deleted.auto_disabled,
            })
        }))
        .await
    }

    pub async fn set_enabled(&self, enabled: bool) -> Result<DesktopClientAuthState, String> {
        let db = Arc::clone(&self.db);
        let runtime = self.runtime.clone();
        let mutations = Arc::clone(&self.mutations);
        await_mutation(tokio::spawn(async move {
            let _guard = mutations.lock().await;
            let policy_update = runtime.begin_update().await;
            let stored = db
                .set_client_auth_enabled(enabled)
                .await
                .map_err(|error| error.to_string())?;
            policy_update.replace(stored.auth.snapshot());
            Ok(desktop_state(stored))
        }))
        .await
    }
}

async fn await_mutation<T>(task: tokio::task::JoinHandle<Result<T, String>>) -> Result<T, String> {
    task.await
        .map_err(|_| "Client authentication mutation task failed".to_string())?
}

fn desktop_state(stored: StoredClientAuthState) -> DesktopClientAuthState {
    DesktopClientAuthState {
        enabled: stored.auth.enabled,
        keys: stored.keys,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;

    async fn create_controller() -> (DesktopClientAuthController, TempDir) {
        let dir = TempDir::new().expect("temp directory");
        let db = Arc::new(
            Database::new(&dir.path().join("desktop-client-auth.db"))
                .await
                .expect("database"),
        );
        let controller = DesktopClientAuthController::hydrate(db)
            .await
            .expect("controller");
        (controller, dir)
    }

    async fn wait_for_policy_update(runtime: &ClientAuthRuntime) {
        for _ in 0..100 {
            let probe_runtime = runtime.clone();
            let mut probe = tokio::spawn(async move { probe_runtime.policy_snapshot().await });
            if tokio::time::timeout(Duration::from_millis(10), &mut probe)
                .await
                .is_err()
            {
                probe.abort();
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("policy update did not acquire the runtime gate");
    }

    #[tokio::test]
    async fn create_and_toggle_replace_the_shared_runtime() {
        let (controller, _dir) = create_controller().await;
        let shared_runtime = controller.runtime();

        let created = controller
            .create(Some("  desktop  "))
            .await
            .expect("create");
        assert!(!created.enabled);
        assert_eq!(created.keys[0].name.as_deref(), Some("desktop"));
        assert!(!shared_runtime.is_enabled());

        let enabled = controller.set_enabled(true).await.expect("enable");
        assert!(enabled.enabled);
        assert!(shared_runtime.is_enabled());
        assert_eq!(shared_runtime.snapshot().key_count(), 1);
    }

    #[tokio::test]
    async fn last_delete_disables_database_and_runtime_together() {
        let (controller, _dir) = create_controller().await;
        let created = controller.create(None).await.expect("create");
        controller.set_enabled(true).await.expect("enable");

        let deleted = controller
            .delete(&created.keys[0].id, true)
            .await
            .expect("delete");
        assert!(deleted.auto_disabled);
        assert!(!deleted.state.enabled);
        assert!(deleted.state.keys.is_empty());
        assert!(!controller.runtime().is_enabled());
    }

    #[tokio::test]
    async fn last_enabled_key_delete_requires_auto_disable_confirmation() {
        let (controller, _dir) = create_controller().await;
        let created = controller.create(None).await.expect("create");
        controller.set_enabled(true).await.expect("enable");

        let error = controller
            .delete(&created.keys[0].id, false)
            .await
            .expect_err("confirmation required");
        assert!(error.contains("confirm"));
        let unchanged = controller.state().await.expect("unchanged state");
        assert!(unchanged.enabled);
        assert_eq!(unchanged.keys.len(), 1);
        assert!(controller.runtime().is_enabled());
    }

    #[tokio::test]
    async fn policy_reads_wait_for_database_commit_and_runtime_replacement() {
        let (controller, _dir) = create_controller().await;
        controller.create(None).await.expect("create");
        let runtime = controller.runtime();
        let mut database_lock = controller.db.pool().acquire().await.expect("DB connection");
        sqlx::query("BEGIN EXCLUSIVE")
            .execute(&mut *database_lock)
            .await
            .expect("exclusive DB transaction");

        let update_controller = controller.clone();
        let update = tokio::spawn(async move { update_controller.set_enabled(true).await });
        wait_for_policy_update(&runtime).await;

        let reader_runtime = runtime.clone();
        let mut reader = tokio::spawn(async move { reader_runtime.policy_snapshot().await });
        assert!(tokio::time::timeout(Duration::from_millis(25), &mut reader)
            .await
            .is_err());

        sqlx::query("ROLLBACK")
            .execute(&mut *database_lock)
            .await
            .expect("release DB transaction");
        update
            .await
            .expect("update task")
            .expect("enable after DB commit");
        let policy = tokio::time::timeout(Duration::from_secs(1), reader)
            .await
            .expect("policy read unblocked")
            .expect("policy reader task");
        assert!(policy.is_enabled());
        assert_eq!(policy.key_count(), 1);
    }

    #[tokio::test]
    async fn caller_cancellation_does_not_interrupt_commit_and_runtime_replacement() {
        let (controller, _dir) = create_controller().await;
        controller.create(None).await.expect("create");
        let runtime = controller.runtime();
        let mut database_lock = controller.db.pool().acquire().await.expect("DB connection");
        sqlx::query("BEGIN EXCLUSIVE")
            .execute(&mut *database_lock)
            .await
            .expect("exclusive DB transaction");

        let update_controller = controller.clone();
        let caller = tokio::spawn(async move { update_controller.set_enabled(true).await });
        wait_for_policy_update(&runtime).await;
        caller.abort();
        assert!(caller.await.expect_err("cancelled caller").is_cancelled());

        sqlx::query("ROLLBACK")
            .execute(&mut *database_lock)
            .await
            .expect("release DB transaction");
        let policy = tokio::time::timeout(Duration::from_secs(1), runtime.policy_snapshot())
            .await
            .expect("detached update completed");
        assert!(policy.is_enabled());
        assert_eq!(policy.key_count(), 1);
        let state = controller.state().await.expect("committed state");
        assert!(state.enabled);
        assert_eq!(state.keys.len(), 1);
    }

    #[tokio::test]
    async fn failed_database_delete_keeps_runtime_and_persisted_state() {
        let (controller, _dir) = create_controller().await;
        let created = controller.create(None).await.expect("create");
        controller.set_enabled(true).await.expect("enable");
        sqlx::query(
            "CREATE TRIGGER fail_desktop_key_delete BEFORE DELETE ON client_api_keys \
             BEGIN SELECT RAISE(ABORT, 'forced delete failure'); END",
        )
        .execute(controller.db.pool())
        .await
        .expect("failure trigger");

        assert!(controller.delete(&created.keys[0].id, true).await.is_err());
        assert!(controller.runtime().is_enabled());
        assert_eq!(controller.runtime().snapshot().key_count(), 1);
        let state = controller.state().await.expect("state");
        assert!(state.enabled);
        assert_eq!(state.keys.len(), 1);
    }

    #[tokio::test]
    async fn concurrent_enable_and_delete_never_leave_enabled_without_keys() {
        let (controller, _dir) = create_controller().await;
        let created = controller.create(None).await.expect("create");
        let id = created.keys[0].id.clone();
        let enable_controller = controller.clone();
        let delete_controller = controller.clone();

        let (_enable_result, delete_result) = tokio::join!(
            enable_controller.set_enabled(true),
            delete_controller.delete(&id, true)
        );
        assert!(delete_result.is_ok());
        let final_state = controller.state().await.expect("final state");
        assert!(!final_state.enabled);
        assert!(final_state.keys.is_empty());
        assert!(!controller.runtime().is_enabled());
    }

    #[tokio::test]
    async fn reveal_after_delete_returns_not_found_without_a_secret() {
        let (controller, _dir) = create_controller().await;
        let created = controller.create(None).await.expect("create");
        let id = created.keys[0].id.clone();
        controller.delete(&id, false).await.expect("delete");

        let error = controller.reveal(&id).await.expect_err("deleted key");
        assert_eq!(error, "Client API key not found");
    }

    #[tokio::test]
    async fn hydration_restores_keys_and_enabled_state_after_reopen() {
        let dir = TempDir::new().expect("temp directory");
        let path = dir.path().join("restart.db");
        let first_db = Arc::new(Database::new(&path).await.expect("first database"));
        let first = DesktopClientAuthController::hydrate(first_db)
            .await
            .expect("first controller");
        let created = first.create(Some("restart key")).await.expect("create");
        let id = created.keys[0].id.clone();
        let secret = first.reveal(&id).await.expect("first reveal");
        first.set_enabled(true).await.expect("enable");
        drop(first);

        let reopened_db = Arc::new(Database::new(&path).await.expect("reopened database"));
        let reopened = DesktopClientAuthController::hydrate(reopened_db)
            .await
            .expect("reopened controller");
        let restored = reopened.state().await.expect("restored state");
        assert!(restored.enabled);
        assert_eq!(restored.keys.len(), 1);
        assert_eq!(restored.keys[0].name.as_deref(), Some("restart key"));
        assert!(reopened.reveal(&id).await.expect("restored reveal") == secret);
        assert!(reopened.runtime().is_enabled());
    }

    #[tokio::test]
    async fn state_refresh_keeps_runtime_in_sync_when_it_normalizes_corrupted_storage() {
        let (controller, _dir) = create_controller().await;
        controller.create(None).await.expect("create");
        controller.set_enabled(true).await.expect("enable");
        sqlx::query("DROP TRIGGER client_api_auth_prevents_last_key_delete")
            .execute(controller.db.pool())
            .await
            .expect("drop invariant trigger");
        sqlx::query("DELETE FROM client_api_keys")
            .execute(controller.db.pool())
            .await
            .expect("create corrupted state");
        assert!(controller.runtime().is_enabled());

        let normalized = controller.state().await.expect("normalized state");

        assert!(!normalized.enabled);
        assert!(normalized.keys.is_empty());
        assert!(!controller.runtime().is_enabled());
    }
}
