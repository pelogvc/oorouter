use std::sync::{Arc, Mutex};
use std::time::Instant;

use proxy_core::routes::AppState;

pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
    pub uptime_secs: u64,
    pub auth_mode: String,
    pub error: Option<String>,
}

pub struct TauriAppState {
    pub proxy_state: Arc<AppState>,
    pub server_status: Arc<Mutex<ServerStatus>>,
    pub server_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub start_time: Instant,
}
