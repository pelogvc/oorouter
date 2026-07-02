use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::Emitter;
use tauri_plugin_autostart::ManagerExt;
use tokio::sync::{oneshot, watch};

use crate::state::{ServerStatus, TauriAppState};

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatusDto {
    pub running: bool,
    pub port: u16,
    pub uptime_secs: u64,
    pub auth_mode: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingDto {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntryDto {
    pub id: String,
    pub timestamp: String,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub status: u16,
    pub duration_ms: u64,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenUsageDto {
    pub date: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelDto {
    pub id: String,
    pub name: String,
    pub context_length: u64,
    pub supports_vision: bool,
    pub visible: bool,
}

fn clone_proxy_state(
    proxy_state: &Arc<proxy_core::routes::AppState>,
) -> proxy_core::routes::AppState {
    (**proxy_state).clone()
}

fn auth_mode_label(auth: &proxy_core::auth::AuthInfo) -> String {
    match &auth.mode {
        proxy_core::auth::AuthMode::ChatGPT => "ChatGPT".to_string(),
        proxy_core::auth::AuthMode::ApiKey => "ApiKey".to_string(),
    }
}

struct ServerTransitionGuard {
    server_stopping: std::sync::Arc<std::sync::Mutex<bool>>,
}

impl Drop for ServerTransitionGuard {
    fn drop(&mut self) {
        if let Ok(mut stopping) = self.server_stopping.lock() {
            *stopping = false;
        }
    }
}

fn reserve_runtime_settings_update(
    server_handle: &std::sync::Arc<std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    server_stopping: &std::sync::Arc<std::sync::Mutex<bool>>,
    running_error: &str,
) -> Result<ServerTransitionGuard, String> {
    let mut stopping = server_stopping
        .lock()
        .map_err(|_| "server stopping lock poisoned".to_string())?;
    if *stopping {
        return Err("server is busy; try again in a moment".to_string());
    }
    if server_handle
        .lock()
        .map_err(|_| "server handle lock poisoned".to_string())?
        .is_some()
    {
        return Err(running_error.to_string());
    }
    *stopping = true;
    Ok(ServerTransitionGuard {
        server_stopping: server_stopping.clone(),
    })
}

pub(crate) fn spawn_server_task(
    proxy_state: Arc<proxy_core::routes::AppState>,
    port: u16,
) -> (
    tauri::async_runtime::JoinHandle<()>,
    watch::Sender<bool>,
    oneshot::Receiver<Result<(), String>>,
    oneshot::Receiver<String>,
) {
    let (tx, rx) = oneshot::channel::<Result<(), String>>();
    let (exit_tx, exit_rx) = oneshot::channel::<String>();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let handle = tauri::async_runtime::spawn(async move {
        let router = proxy_core::routes::create_router(clone_proxy_state(&proxy_state));
        let ipv4_addr = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port));
        let ipv6_addr = std::net::SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port));

        let ipv4_listener = match tokio::net::TcpListener::bind(ipv4_addr).await {
            Ok(l) => {
                let _ = tx.send(Ok(()));
                l
            }
            Err(e) => {
                let msg = format!("포트 {} 바인딩 실패: {e}", port);
                tracing::warn!("{msg}");
                let _ = tx.send(Err(msg));
                let _ = exit_tx.send(format!("Proxy server failed to bind port {port}: {e}"));
                return;
            }
        };
        let ipv6_listener = match tokio::net::TcpListener::bind(ipv6_addr).await {
            Ok(listener) => Some(listener),
            Err(error) => {
                tracing::warn!(%error, "IPv6 loopback bind failed; continuing with IPv4 only");
                None
            }
        };

        let server_result = if let Some(ipv6_listener) = ipv6_listener {
            tracing::info!("proxy listening on http://{ipv4_addr} and http://{ipv6_addr}");
            let ipv4_shutdown_rx = shutdown_rx.clone();
            let ipv6_shutdown_rx = shutdown_rx;
            tokio::try_join!(
                axum::serve(ipv4_listener, router.clone())
                    .with_graceful_shutdown(wait_for_shutdown(ipv4_shutdown_rx)),
                axum::serve(ipv6_listener, router)
                    .with_graceful_shutdown(wait_for_shutdown(ipv6_shutdown_rx)),
            )
            .map(|_| ())
        } else {
            tracing::info!("proxy listening on http://{ipv4_addr}");
            axum::serve(ipv4_listener, router)
                .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
                .await
        };

        let exit_message = match server_result {
            Ok(()) => "Proxy server stopped".to_string(),
            Err(e) => {
                tracing::error!(error = %e, "proxy server error");
                format!("Proxy server error: {e}")
            }
        };
        let _ = exit_tx.send(exit_message);
    });

    (handle, shutdown_tx, rx, exit_rx)
}

async fn wait_for_shutdown(mut shutdown_rx: watch::Receiver<bool>) {
    while !*shutdown_rx.borrow_and_update() {
        if shutdown_rx.changed().await.is_err() {
            break;
        }
    }
}

fn is_current_generation(
    server_generation: &std::sync::Arc<std::sync::Mutex<u64>>,
    startup_generation: u64,
) -> bool {
    server_generation
        .lock()
        .map(|generation| *generation == startup_generation)
        .unwrap_or(false)
}

fn spawn_start_completion_monitor(
    app_handle: tauri::AppHandle,
    server_status: std::sync::Arc<std::sync::Mutex<ServerStatus>>,
    server_handle: std::sync::Arc<std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    server_shutdown: std::sync::Arc<std::sync::Mutex<Option<watch::Sender<bool>>>>,
    server_generation: std::sync::Arc<std::sync::Mutex<u64>>,
    startup_generation: u64,
    rx: oneshot::Receiver<Result<(), String>>,
    exit_rx: oneshot::Receiver<String>,
) {
    tauri::async_runtime::spawn(async move {
        let bind_result = tokio::time::timeout(Duration::from_secs(2), rx).await;

        match bind_result {
            Ok(Ok(Ok(()))) => {
                if !is_current_generation(&server_generation, startup_generation) {
                    return;
                }
                if let Ok(mut status) = server_status.lock() {
                    status.running = true;
                    status.error = None;
                    status.started_at = Some(Instant::now());
                    status.uptime_secs = 0;
                }
                if let Some(tray) = app_handle.tray_by_id("main-tray") {
                    crate::update_tray_icon_for_state(&tray, true);
                }
                let _ = app_handle.emit("server-status-changed", ());
                spawn_server_exit_monitor(
                    app_handle,
                    server_status,
                    server_handle,
                    server_shutdown,
                    server_generation,
                    startup_generation,
                    exit_rx,
                );
            }
            Ok(Ok(Err(err_msg))) => {
                if !is_current_generation(&server_generation, startup_generation) {
                    return;
                }
                if let Ok(mut handle_guard) = server_handle.lock() {
                    if let Some(handle) = handle_guard.take() {
                        handle.abort();
                    }
                }
                if let Ok(mut shutdown_guard) = server_shutdown.lock() {
                    let _ = shutdown_guard.take();
                }
                if let Ok(mut status) = server_status.lock() {
                    status.running = false;
                    status.error = Some(err_msg.clone());
                    status.started_at = None;
                }
                let _ = app_handle.emit("port-conflict", &err_msg);
            }
            Ok(Err(_recv_err)) => {
                if !is_current_generation(&server_generation, startup_generation) {
                    return;
                }
                if let Ok(mut handle_guard) = server_handle.lock() {
                    if let Some(handle) = handle_guard.take() {
                        handle.abort();
                    }
                }
                if let Ok(mut shutdown_guard) = server_shutdown.lock() {
                    let _ = shutdown_guard.take();
                }
                let err_msg = "서버 태스크가 예기치 않게 종료됨".to_string();
                if let Ok(mut status) = server_status.lock() {
                    status.running = false;
                    status.error = Some(err_msg.clone());
                    status.started_at = None;
                }
                let _ = app_handle.emit("server-error", &err_msg);
            }
            Err(_elapsed) => {
                if !is_current_generation(&server_generation, startup_generation) {
                    return;
                }
                if let Ok(mut handle_guard) = server_handle.lock() {
                    if let Some(handle) = handle_guard.take() {
                        handle.abort();
                    }
                }
                if let Ok(mut shutdown_guard) = server_shutdown.lock() {
                    let _ = shutdown_guard.take();
                }
                let err_msg = "Timed out while starting proxy server".to_string();
                if let Ok(mut status) = server_status.lock() {
                    status.running = false;
                    status.error = Some(err_msg.clone());
                    status.started_at = None;
                }
                let _ = app_handle.emit("server-error", &err_msg);
            }
        }
    });
}

#[tauri::command]
pub async fn get_server_status(
    state: tauri::State<'_, TauriAppState>,
) -> Result<ServerStatusDto, String> {
    let status = state
        .server_status
        .lock()
        .map_err(|_| "server status lock poisoned".to_string())?;

    let uptime_secs = if status.running {
        status
            .started_at
            .map(|started_at| started_at.elapsed().as_secs())
            .unwrap_or(status.uptime_secs)
    } else {
        status.uptime_secs
    };

    Ok(ServerStatusDto {
        running: status.running,
        port: status.port,
        uptime_secs,
        auth_mode: status.auth_mode.clone(),
        error: status.error.clone(),
    })
}

#[tauri::command]
pub async fn start_server(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, TauriAppState>,
) -> Result<(), String> {
    let (startup_generation, rx, exit_rx) = {
        let stopping_guard = state
            .server_stopping
            .lock()
            .map_err(|_| "server stopping lock poisoned".to_string())?;
        if *stopping_guard {
            return Ok(());
        }
        let mut generation = state
            .server_generation
            .lock()
            .map_err(|_| "server generation lock poisoned".to_string())?;
        let mut handle_guard = state
            .server_handle
            .lock()
            .map_err(|_| "server handle lock poisoned".to_string())?;
        let mut shutdown_guard = state
            .server_shutdown
            .lock()
            .map_err(|_| "server shutdown lock poisoned".to_string())?;
        if *stopping_guard || handle_guard.is_some() {
            return Ok(());
        }
        let port = {
            let status = state
                .server_status
                .lock()
                .map_err(|_| "server status lock poisoned".to_string())?;
            status.port
        };

        *generation = generation.saturating_add(1);
        let startup_generation = *generation;
        let (handle, shutdown_tx, rx, exit_rx) = spawn_server_task(state.proxy_state.clone(), port);
        *handle_guard = Some(handle);
        *shutdown_guard = Some(shutdown_tx);
        (startup_generation, rx, exit_rx)
    };
    spawn_start_completion_monitor(
        app_handle,
        state.server_status.clone(),
        state.server_handle.clone(),
        state.server_shutdown.clone(),
        state.server_generation.clone(),
        startup_generation,
        rx,
        exit_rx,
    );
    Ok(())
}

fn spawn_server_exit_monitor(
    app_handle: tauri::AppHandle,
    server_status: std::sync::Arc<std::sync::Mutex<crate::state::ServerStatus>>,
    server_handle: std::sync::Arc<std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    server_shutdown: std::sync::Arc<std::sync::Mutex<Option<watch::Sender<bool>>>>,
    server_generation: std::sync::Arc<std::sync::Mutex<u64>>,
    startup_generation: u64,
    exit_rx: oneshot::Receiver<String>,
) {
    tauri::async_runtime::spawn(async move {
        let Ok(message) = exit_rx.await else {
            return;
        };
        let is_current = server_generation
            .lock()
            .map(|generation| *generation == startup_generation)
            .unwrap_or(false);
        if !is_current {
            return;
        }

        let was_running = server_status
            .lock()
            .map(|status| status.running)
            .unwrap_or(false);
        if !was_running {
            return;
        }

        let removed_handle = server_handle
            .lock()
            .map(|mut handle_guard| handle_guard.take().is_some())
            .unwrap_or(false);
        if !removed_handle {
            return;
        }
        if let Ok(mut shutdown_guard) = server_shutdown.lock() {
            let _ = shutdown_guard.take();
        }

        if let Ok(mut status) = server_status.lock() {
            if let Some(started_at) = status.started_at {
                status.uptime_secs = started_at.elapsed().as_secs();
            }
            status.running = false;
            status.started_at = None;
            status.error = Some(message.clone());
        }
        let _ = app_handle.emit("server-status-changed", ());
        let _ = app_handle.emit("server-error", &message);
    });
}

#[tauri::command]
pub async fn stop_server(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, TauriAppState>,
) -> Result<(), String> {
    stop_server_with_parts(
        app_handle,
        state.server_status.clone(),
        state.server_handle.clone(),
        state.server_shutdown.clone(),
        state.server_generation.clone(),
        state.server_stopping.clone(),
    )
    .await
}

pub(crate) async fn stop_server_with_parts(
    app_handle: tauri::AppHandle,
    server_status: std::sync::Arc<std::sync::Mutex<ServerStatus>>,
    server_handle: std::sync::Arc<std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    server_shutdown: std::sync::Arc<std::sync::Mutex<Option<watch::Sender<bool>>>>,
    server_generation: std::sync::Arc<std::sync::Mutex<u64>>,
    server_stopping: std::sync::Arc<std::sync::Mutex<bool>>,
) -> Result<(), String> {
    {
        let mut stopping = server_stopping
            .lock()
            .map_err(|_| "server stopping lock poisoned".to_string())?;
        if *stopping {
            return Ok(());
        }
        *stopping = true;
    }

    let prepare_result: Result<
        (
            Option<watch::Sender<bool>>,
            Option<tauri::async_runtime::JoinHandle<()>>,
        ),
        String,
    > = (|| {
        let mut generation = server_generation
            .lock()
            .map_err(|_| "server generation lock poisoned".to_string())?;
        *generation = generation.saturating_add(1);
        drop(generation);

        let shutdown_tx = {
            let mut shutdown_guard = server_shutdown
                .lock()
                .map_err(|_| "server shutdown lock poisoned".to_string())?;
            shutdown_guard.take()
        };
        let handle = {
            let mut handle_guard = server_handle
                .lock()
                .map_err(|_| "server handle lock poisoned".to_string())?;
            handle_guard.take()
        };

        Ok((shutdown_tx, handle))
    })();

    let result: Result<(), String> = async {
        let (shutdown_tx, handle) = prepare_result?;

        if let Some(shutdown_tx) = shutdown_tx {
            let _ = shutdown_tx.send(true);
        }

        if let Some(mut handle) = handle {
            tokio::select! {
                _ = &mut handle => {}
                _ = tokio::time::sleep(Duration::from_secs(2)) => {
                    handle.abort();
                    let _ = handle.await;
                }
            }
        }

        let mut status = server_status
            .lock()
            .map_err(|_| "server status lock poisoned".to_string())?;
        if let Some(started_at) = status.started_at {
            status.uptime_secs = started_at.elapsed().as_secs();
        }
        status.running = false;
        status.started_at = None;
        status.error = None;
        drop(status);

        if let Some(tray) = app_handle.tray_by_id("main-tray") {
            crate::update_tray_icon_for_stopped(&tray);
        }
        let _ = app_handle.emit("server-status-changed", ());

        Ok(())
    }
    .await;

    if let Ok(mut stopping) = server_stopping.lock() {
        *stopping = false;
    }

    result
}

#[tauri::command]
pub async fn get_settings(
    state: tauri::State<'_, TauriAppState>,
) -> Result<Vec<SettingDto>, String> {
    let rows = state
        .proxy_state
        .db
        .get_all_settings()
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|(key, value)| SettingDto { key, value })
        .collect())
}

#[tauri::command]
pub async fn update_setting(
    key: String,
    value: String,
    state: tauri::State<'_, TauriAppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    if !matches!(
        key.as_str(),
        "port" | "auth_path" | "auto_start" | "log_level"
    ) {
        return Err(format!("unsupported setting: {key}"));
    }
    let value = if key == "auth_path" {
        value.trim().to_string()
    } else {
        value
    };
    let (parsed_port, port_changed) = if key == "port" {
        let port = value
            .parse::<u16>()
            .map_err(|e| format!("invalid port: {e}"))?;
        if port == 0 {
            return Err("invalid port: must be between 1 and 65535".to_string());
        }
        let current_port = state
            .server_status
            .lock()
            .map_err(|_| "server status lock poisoned".to_string())?
            .port;
        (Some(port), port != current_port)
    } else {
        (None, false)
    };

    let auth_path_unchanged = if key == "auth_path" {
        state
            .proxy_state
            .db
            .get_setting("auth_path")
            .await
            .map_err(|e| e.to_string())?
            .as_deref()
            == Some(value.as_str())
    } else {
        false
    };
    if auth_path_unchanged {
        return Ok(());
    }

    let _runtime_settings_guard = match key.as_str() {
        "port" if port_changed => Some(reserve_runtime_settings_update(
            &state.server_handle,
            &state.server_stopping,
            "stop the server before changing the port",
        )?),
        "auth_path" => Some(reserve_runtime_settings_update(
            &state.server_handle,
            &state.server_stopping,
            "stop the server before changing the auth file",
        )?),
        _ => None,
    };

    let auth_update = if key == "auth_path" {
        if value.trim().is_empty() {
            return Err("auth_path must not be empty".to_string());
        }
        let auth_path = proxy_core::config::expand_path(value.trim())
            .map_err(|e| format!("invalid auth_path: {e}"))?;
        let new_auth = proxy_core::auth::load_auth(&auth_path)
            .map_err(|e| format!("invalid auth_path: {e}"))?;
        let new_watcher =
            proxy_core::auth_watcher::start_auth_watcher(auth_path, state.shared_auth.clone())
                .map_err(|e| format!("auth watcher update failed: {e}"))?;
        Some((new_auth, new_watcher))
    } else {
        None
    };

    let auto_start_enabled = if key == "auto_start" {
        match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => return Err("auto_start must be true or false".to_string()),
        }
    } else {
        None
    };

    if key == "log_level" && !matches!(value.as_str(), "debug" | "info" | "warn" | "error") {
        return Err("log_level must be debug, info, warn, or error".to_string());
    }
    if key == "log_level" {
        return Err("log_level updates are not supported at runtime".to_string());
    }

    let previous_auto_start = if auto_start_enabled.is_some() {
        state
            .proxy_state
            .db
            .get_setting("auto_start")
            .await
            .map_err(|e| e.to_string())?
    } else {
        None
    };

    state
        .proxy_state
        .db
        .set_setting(&key, &value)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(enabled) = auto_start_enabled {
        let autolaunch = app_handle.autolaunch();
        let result = if enabled {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if let Err(error) = result {
            let rollback_value = previous_auto_start.unwrap_or_else(|| "true".to_string());
            if let Err(rollback_error) = state
                .proxy_state
                .db
                .set_setting("auto_start", &rollback_value)
                .await
            {
                return Err(format!(
                    "autostart update failed: {error}; rollback failed: {rollback_error}"
                ));
            }
            return Err(format!("autostart update failed: {error}"));
        }
    }

    if let Some(port) = parsed_port {
        let mut status = state
            .server_status
            .lock()
            .map_err(|_| "server status lock poisoned".to_string())?;
        status.port = port;
    }

    if let Some((new_auth, new_watcher)) = auth_update {
        {
            let mut auth_guard = state
                .shared_auth
                .write()
                .map_err(|_| "auth lock poisoned".to_string())?;
            *auth_guard = Some(new_auth.clone());
        }
        {
            let mut watcher_guard = state
                .auth_watcher
                .lock()
                .map_err(|_| "auth watcher lock poisoned".to_string())?;
            *watcher_guard = Some(new_watcher);
        }
        let mut status = state
            .server_status
            .lock()
            .map_err(|_| "server status lock poisoned".to_string())?;
        status.auth_mode = auth_mode_label(&new_auth);
    }

    Ok(())
}

#[tauri::command]
pub async fn get_recent_logs(
    limit: usize,
    state: tauri::State<'_, TauriAppState>,
) -> Result<Vec<LogEntryDto>, String> {
    let logs =
        proxy_core::logger::get_recent_logs(&state.proxy_state.log_buffer, limit.clamp(1, 500));

    Ok(logs
        .into_iter()
        .map(|entry| LogEntryDto {
            id: entry.id,
            timestamp: entry.timestamp,
            method: entry.method,
            path: entry.path,
            model: entry.model,
            status: entry.status,
            duration_ms: entry.duration_ms,
            input_tokens: entry.input_tokens,
            output_tokens: entry.output_tokens,
        })
        .collect())
}

#[tauri::command]
pub async fn get_token_usage(
    days: u32,
    state: tauri::State<'_, TauriAppState>,
) -> Result<Vec<TokenUsageDto>, String> {
    if !(1..=3650).contains(&days) {
        return Err(format!("days must be between 1 and 3650: {days}"));
    }

    let usage = state
        .proxy_state
        .db
        .get_token_usage_summary(i64::from(days))
        .await
        .map_err(|e| e.to_string())?;

    Ok(usage
        .into_iter()
        .map(|row| TokenUsageDto {
            date: row.date,
            model: row.model,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            total_tokens: row.total_tokens,
            request_count: row.request_count,
        })
        .collect())
}

#[tauri::command]
pub async fn get_models(_state: tauri::State<'_, TauriAppState>) -> Result<Vec<ModelDto>, String> {
    let models = proxy_core::models::get_visible_models();

    Ok(models
        .into_iter()
        .map(|item| {
            let id = item.name.trim_end_matches(":latest").to_string();
            if let Some(def) = proxy_core::models::get_model_definition(&id) {
                ModelDto {
                    id: def.slug.to_string(),
                    name: def.name.to_string(),
                    context_length: def.context_length,
                    supports_vision: def.supports_vision,
                    visible: def.visible,
                }
            } else {
                ModelDto {
                    id: id.clone(),
                    name: id,
                    context_length: 400_000,
                    supports_vision: true,
                    visible: true,
                }
            }
        })
        .collect())
}
