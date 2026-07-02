use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::auth::{load_auth, AuthInfo};
use crate::error::{ProxyError, Result};

pub type AuthWatcher = RecommendedWatcher;
pub type SharedAuth = Arc<RwLock<Option<AuthInfo>>>;

pub fn new_shared_auth(auth: AuthInfo) -> SharedAuth {
    Arc::new(RwLock::new(Some(auth)))
}

pub fn read_shared_auth(shared_auth: &SharedAuth) -> Result<AuthInfo> {
    let guard = shared_auth
        .read()
        .map_err(|_| ProxyError::AuthError("auth lock poisoned".to_string()))?;
    guard
        .clone()
        .ok_or_else(|| ProxyError::AuthError("Authentication is not available".to_string()))
}

fn is_auth_file_event(event: &notify::Event, auth_path: &std::path::Path) -> bool {
    let Some(auth_file_name) = auth_path.file_name() else {
        return false;
    };

    event.paths.iter().any(|path| {
        path == auth_path || path.file_name().is_some_and(|name| name == auth_file_name)
    })
}

fn should_reload_auth(event: &notify::Event, auth_path: &std::path::Path) -> bool {
    if !is_auth_file_event(event, auth_path) {
        return false;
    }

    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn retry_load_auth_after_debounce(auth_path: &std::path::Path) -> Result<AuthInfo> {
    let mut last_error = None;
    for _ in 0..5 {
        std::thread::sleep(Duration::from_millis(75));
        match load_auth(auth_path) {
            Ok(auth) => return Ok(auth),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error
        .unwrap_or_else(|| ProxyError::AuthError("Failed to reload auth.json".to_string())))
}

fn auth_file_still_missing_after_debounce(auth_path: &std::path::Path) -> bool {
    for _ in 0..5 {
        if auth_path.exists() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(75));
    }

    !auth_path.exists()
}

fn watch_directory_for_auth_path(auth_path: &std::path::Path) -> Result<PathBuf> {
    let parent = auth_path.parent().ok_or_else(|| {
        ProxyError::AuthError("auth file path has no parent directory".to_string())
    })?;

    if parent.as_os_str().is_empty() {
        Ok(PathBuf::from("."))
    } else {
        Ok(parent.to_path_buf())
    }
}

/// auth.json 파일 감시 시작.
///
/// 파일 변경(`Modify` 이벤트) 감지 시 `load_auth`를 재호출하여
/// `shared_auth`를 새 `AuthInfo`로 교체한다.
///
/// - 성공 시: write lock → 새 AuthInfo로 교체 + `tracing::info!` 로그
/// - 실패 시: `tracing::warn!` 로그 + 기존 AuthInfo 유지 (교체 안 함)
/// - 진행 중인 요청은 기존 토큰으로 완료 (RwLock read lock 유지)
///
/// 반환된 `RecommendedWatcher`를 drop하면 감시가 중지된다.
pub fn start_auth_watcher(
    auth_path: PathBuf,
    shared_auth: SharedAuth,
) -> Result<RecommendedWatcher> {
    let watch_path = auth_path.clone();
    let watch_dir = watch_directory_for_auth_path(&watch_path)?;

    let mut watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                if should_reload_auth(&event, &auth_path) {
                    if matches!(event.kind, EventKind::Remove(_)) {
                        match retry_load_auth_after_debounce(&auth_path) {
                            Ok(new_auth) => match shared_auth.write() {
                                Ok(mut guard) => {
                                    *guard = Some(new_auth);
                                    tracing::info!("auth.json reloaded successfully");
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to acquire auth write lock: {}", e);
                                }
                            },
                            Err(error) if auth_file_still_missing_after_debounce(&auth_path) => {
                                match shared_auth.write() {
                                    Ok(mut guard) => {
                                        *guard = None;
                                        tracing::warn!(%error, "auth.json is unavailable after confirmed removal; authentication has been invalidated");
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to acquire auth write lock: {}", e);
                                    }
                                }
                            }
                            Err(error) => {
                                tracing::warn!(%error, "auth.json is unavailable after removal event; keeping existing auth until deletion is confirmed");
                            }
                        }
                        return;
                    }

                    match retry_load_auth_after_debounce(&auth_path) {
                        Ok(new_auth) => match shared_auth.write() {
                            Ok(mut guard) => {
                                *guard = Some(new_auth);
                                tracing::info!("auth.json reloaded successfully");
                            }
                            Err(e) => {
                                tracing::warn!("Failed to acquire auth write lock: {}", e);
                            }
                        },
                        Err(e) if !auth_path.exists()
                            && auth_file_still_missing_after_debounce(&auth_path) =>
                        {
                            match shared_auth.write() {
                            Ok(mut guard) => {
                                *guard = None;
                                tracing::warn!(
                                    "auth.json is unavailable after file event; authentication has been invalidated: {}",
                                    e
                                );
                            }
                            Err(lock_error) => {
                                tracing::warn!(
                                    "Failed to acquire auth write lock: {}",
                                    lock_error
                                );
                            }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to reload auth.json, keeping existing auth: {}",
                                e
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("File watch error: {}", e);
            }
        })
        .map_err(|e| ProxyError::AuthError(format!("Failed to create auth file watcher: {}", e)))?;

    watcher
        .watch(&watch_dir, RecursiveMode::NonRecursive)
        .map_err(|e| ProxyError::AuthError(format!("Failed to watch auth directory: {}", e)))?;

    tracing::info!("Started watching auth directory: {:?}", watch_dir);
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthInfo, AuthMode};

    #[test]
    fn test_new_shared_auth() {
        let auth = AuthInfo {
            mode: AuthMode::ChatGPT,
            access_token: "test-token".to_string(),
            account_id: Some("test-account".to_string()),
        };

        let shared = new_shared_auth(auth);
        let guard = shared.read().expect("read lock");
        let auth = guard.as_ref().expect("auth");
        assert_eq!(auth.access_token, "test-token");
        assert_eq!(auth.account_id, Some("test-account".to_string()));
        assert!(matches!(auth.mode, AuthMode::ChatGPT));
    }

    #[test]
    fn test_auth_watcher_reload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let auth_path = dir.path().join("auth.json");

        std::fs::write(
            &auth_path,
            r#"{"tokens": {"access_token": "initial-token", "account_id": "acc-1"}}"#,
        )
        .expect("write initial");

        let auth = load_auth(&auth_path).expect("load initial");
        assert_eq!(auth.access_token, "initial-token");

        let shared = new_shared_auth(auth);
        let _watcher =
            start_auth_watcher(auth_path.clone(), shared.clone()).expect("start watcher");

        std::thread::sleep(std::time::Duration::from_millis(500));

        std::fs::write(
            &auth_path,
            r#"{"tokens": {"access_token": "updated-token", "account_id": "acc-2"}}"#,
        )
        .expect("write updated");

        wait_for_auth(&shared, "updated-token", Some("acc-2".to_string()));
    }

    #[test]
    fn test_auth_watcher_invalid_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let auth_path = dir.path().join("auth.json");

        std::fs::write(
            &auth_path,
            r#"{"tokens": {"access_token": "original-token", "account_id": "acc-1"}}"#,
        )
        .expect("write initial");

        let auth = load_auth(&auth_path).expect("load initial");
        let shared = new_shared_auth(auth);
        let _watcher =
            start_auth_watcher(auth_path.clone(), shared.clone()).expect("start watcher");

        std::thread::sleep(std::time::Duration::from_millis(500));

        std::fs::write(&auth_path, "not valid json{{{").expect("write invalid");

        std::thread::sleep(std::time::Duration::from_secs(2));

        let guard = shared.read().expect("read lock");
        let auth = guard.as_ref().expect("auth");
        assert_eq!(auth.access_token, "original-token");
        assert_eq!(auth.account_id, Some("acc-1".to_string()));
    }

    #[test]
    fn test_auth_watcher_invalidates_on_remove() {
        let dir = tempfile::tempdir().expect("tempdir");
        let auth_path = dir.path().join("auth.json");

        std::fs::write(
            &auth_path,
            r#"{"tokens": {"access_token": "original-token", "account_id": "acc-1"}}"#,
        )
        .expect("write initial");

        let auth = load_auth(&auth_path).expect("load initial");
        let shared = new_shared_auth(auth);
        let _watcher =
            start_auth_watcher(auth_path.clone(), shared.clone()).expect("start watcher");

        std::thread::sleep(std::time::Duration::from_millis(500));
        std::fs::remove_file(&auth_path).expect("remove auth");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if shared.read().expect("read lock").is_none() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for auth invalidation"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    fn wait_for_auth(
        shared: &SharedAuth,
        expected_token: &str,
        expected_account_id: Option<String>,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            {
                let guard = shared.read().expect("read lock");
                if let Some(auth) = guard.as_ref() {
                    if auth.access_token == expected_token {
                        assert_eq!(auth.account_id, expected_account_id);
                        break;
                    }
                } else {
                    if expected_token.is_empty() {
                        break;
                    }
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for auth reload"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn relative_auth_file_watches_current_directory() {
        assert_eq!(
            watch_directory_for_auth_path(std::path::Path::new("auth.json")).unwrap(),
            PathBuf::from(".")
        );
    }
}
