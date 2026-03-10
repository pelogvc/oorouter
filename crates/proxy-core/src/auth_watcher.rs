use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::auth::{load_auth, AuthInfo};
use crate::error::{ProxyError, Result};


pub type SharedAuth = Arc<RwLock<AuthInfo>>;


pub fn new_shared_auth(auth: AuthInfo) -> SharedAuth {
    Arc::new(RwLock::new(auth))
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

    let mut watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                if matches!(event.kind, EventKind::Modify(_)) {
                    match load_auth(&auth_path) {
                        Ok(new_auth) => match shared_auth.write() {
                            Ok(mut guard) => {
                                *guard = new_auth;
                                tracing::info!("auth.json reloaded successfully");
                            }
                            Err(e) => {
                                tracing::warn!("Failed to acquire auth write lock: {}", e);
                            }
                        },
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
        .watch(&watch_path, RecursiveMode::NonRecursive)
        .map_err(|e| ProxyError::AuthError(format!("Failed to watch auth file: {}", e)))?;

    tracing::info!("Started watching auth file: {:?}", watch_path);
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
        assert_eq!(guard.access_token, "test-token");
        assert_eq!(guard.account_id, Some("test-account".to_string()));
        assert!(matches!(guard.mode, AuthMode::ChatGPT));
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


        std::thread::sleep(std::time::Duration::from_secs(2));

        let guard = shared.read().expect("read lock");
        assert_eq!(guard.access_token, "updated-token");
        assert_eq!(guard.account_id, Some("acc-2".to_string()));
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
        assert_eq!(guard.access_token, "original-token");
        assert_eq!(guard.account_id, Some("acc-1".to_string()));
    }
}
