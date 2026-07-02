use std::sync::{Arc, Mutex};
use std::time::Instant;

use proxy_core::routes::AppState;

pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
    pub uptime_secs: u64,
    pub auth_mode: String,
    pub error: Option<String>,
    pub started_at: Option<Instant>,
}

pub struct TauriAppState {
    pub proxy_state: Arc<AppState>,
    pub server_status: Arc<Mutex<ServerStatus>>,
    pub server_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub server_shutdown: Arc<Mutex<Option<tokio::sync::watch::Sender<bool>>>>,
    pub server_generation: Arc<Mutex<u64>>,
    pub server_stopping: Arc<Mutex<bool>>,
    pub shared_auth: proxy_core::auth_watcher::SharedAuth,
    pub auth_watcher: Arc<Mutex<Option<proxy_core::auth_watcher::AuthWatcher>>>,
}
