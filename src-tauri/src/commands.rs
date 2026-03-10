use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::oneshot;
use tauri::Emitter;
use tauri_plugin_autostart::ManagerExt;

use crate::state::TauriAppState;

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

fn spawn_server_task(
    proxy_state: Arc<proxy_core::routes::AppState>,
    port: u16,
) -> (
    tauri::async_runtime::JoinHandle<()>,
    oneshot::Receiver<Result<(), String>>,
) {
    let (tx, rx) = oneshot::channel::<Result<(), String>>();

    let handle = tauri::async_runtime::spawn(async move {
        let router = proxy_core::routes::create_router(clone_proxy_state(&proxy_state));
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => {
                let _ = tx.send(Ok(()));
                l
            }
            Err(e) => {
                let msg = format!("포트 {} 바인딩 실패: {e}", port);
                eprintln!("[proxy] {msg}");
                let _ = tx.send(Err(msg));
                return;
            }
        };

        eprintln!("[proxy] Listening on http://{addr}");
        if let Err(e) = axum::serve(listener, router).await {
            eprintln!("[proxy] Server error: {e}");
        }
    });

    (handle, rx)
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
        state.start_time.elapsed().as_secs()
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
    {
        let handle_guard = state
            .server_handle
            .lock()
            .map_err(|_| "server handle lock poisoned".to_string())?;
        if handle_guard.is_some() {
            return Ok(());
        }
    }

    let port = {
        let status = state
            .server_status
            .lock()
            .map_err(|_| "server status lock poisoned".to_string())?;
        status.port
    };

    let (handle, rx) = spawn_server_task(state.proxy_state.clone(), port);

    let bind_result = tokio::time::timeout(Duration::from_secs(2), rx).await;

    match bind_result {
        Ok(Ok(Ok(()))) => {
            {
                let mut handle_guard = state
                    .server_handle
                    .lock()
                    .map_err(|_| "server handle lock poisoned".to_string())?;
                *handle_guard = Some(handle);
            }
            let mut status = state
                .server_status
                .lock()
                .map_err(|_| "server status lock poisoned".to_string())?;
            status.running = true;
            status.error = None;
            status.uptime_secs = 0;
            Ok(())
        }

        Ok(Ok(Err(err_msg))) => {
            handle.abort();
            {
                let mut status = state
                    .server_status
                    .lock()
                    .map_err(|_| "server status lock poisoned".to_string())?;
                status.running = false;
                status.error = Some(err_msg.clone());
            }
            let _ = app_handle.emit("port-conflict", &err_msg);
            Err(err_msg)
        }

        Ok(Err(_recv_err)) => {
            handle.abort();
            let err_msg = "서버 태스크가 예기치 않게 종료됨".to_string();
            {
                let mut status = state
                    .server_status
                    .lock()
                    .map_err(|_| "server status lock poisoned".to_string())?;
                status.running = false;
                status.error = Some(err_msg.clone());
            }
            let _ = app_handle.emit("port-conflict", &err_msg);
            Err(err_msg)
        }

        Err(_elapsed) => {
            {
                let mut handle_guard = state
                    .server_handle
                    .lock()
                    .map_err(|_| "server handle lock poisoned".to_string())?;
                *handle_guard = Some(handle);
            }
            let mut status = state
                .server_status
                .lock()
                .map_err(|_| "server status lock poisoned".to_string())?;
            status.running = true;
            status.error = None;
            status.uptime_secs = 0;
            Ok(())
        }
    }
}

#[tauri::command]
pub async fn stop_server(state: tauri::State<'_, TauriAppState>) -> Result<(), String> {
    {
        let mut handle_guard = state
            .server_handle
            .lock()
            .map_err(|_| "server handle lock poisoned".to_string())?;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    let mut status = state
        .server_status
        .lock()
        .map_err(|_| "server status lock poisoned".to_string())?;
    status.running = false;
    status.uptime_secs = state.start_time.elapsed().as_secs();

    Ok(())
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
    state
        .proxy_state
        .db
        .set_setting(&key, &value)
        .await
        .map_err(|e| e.to_string())?;
    if key == "port" {
        let port = value
            .parse::<u16>()
            .map_err(|e| format!("invalid port: {e}"))?;
        let mut status = state
            .server_status
            .lock()
            .map_err(|_| "server status lock poisoned".to_string())?;
        status.port = port;
    }

    if key == "auto_start" {
        let autolaunch = app_handle.autolaunch();
        if value == "true" {
            autolaunch
                .enable()
                .map_err(|e| format!("autostart enable failed: {e}"))?;
        } else {
            autolaunch
                .disable()
                .map_err(|e| format!("autostart disable failed: {e}"))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_recent_logs(
    limit: usize,
    state: tauri::State<'_, TauriAppState>,
) -> Result<Vec<LogEntryDto>, String> {
    let logs = proxy_core::logger::get_recent_logs(&state.proxy_state.log_buffer, limit);

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
pub async fn get_models(
    _state: tauri::State<'_, TauriAppState>,
) -> Result<Vec<ModelDto>, String> {
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
