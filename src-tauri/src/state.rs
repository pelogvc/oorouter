use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::Instant;

use proxy_core::routes::AppState;
use tauri_plugin_updater::Update;

pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
    pub uptime_secs: u64,
    pub auth_mode: String,
    pub error: Option<String>,
    pub started_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppUpdateStatus {
    Idle,
    Checking,
    Available,
    Installing,
    Installed,
    Error,
}

impl AppUpdateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Checking => "checking",
            Self::Available => "available",
            Self::Installing => "installing",
            Self::Installed => "installed",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppUpdateState {
    pub status: AppUpdateStatus,
    pub current_version: String,
    pub version: Option<String>,
    pub date: Option<String>,
    pub body: Option<String>,
    pub downloaded_bytes: u64,
    pub content_length: Option<u64>,
    pub error: Option<String>,
    pub visible: bool,
    pub manual: bool,
}

impl AppUpdateState {
    pub fn idle(current_version: impl Into<String>) -> Self {
        Self {
            status: AppUpdateStatus::Idle,
            current_version: current_version.into(),
            version: None,
            date: None,
            body: None,
            downloaded_bytes: 0,
            content_length: None,
            error: None,
            visible: false,
            manual: false,
        }
    }
}

pub struct AppUpdateRuntimeState {
    pub pending_update: Option<Update>,
    pub state: AppUpdateState,
}

impl AppUpdateRuntimeState {
    pub fn idle(current_version: impl Into<String>) -> Self {
        Self {
            pending_update: None,
            state: AppUpdateState::idle(current_version),
        }
    }
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
    pub update_runtime: Arc<Mutex<AppUpdateRuntimeState>>,
    pub update_busy: Arc<AtomicBool>,
}
