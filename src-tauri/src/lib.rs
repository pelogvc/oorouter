mod commands;
mod state;

use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Listener, Manager, RunEvent, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_positioner::{Position, WindowExt};

use crate::state::{ServerStatus, TauriAppState};

static ERROR_ICON_RGBA: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for chunk in rgba.chunks_exact_mut(4) {
        chunk[0] = 220;
        chunk[1] = 50;
        chunk[2] = 50;
        chunk[3] = 200;
    }
    rgba
});

fn create_proxy_state() -> Result<
    (
        Arc<proxy_core::routes::AppState>,
        u16,
        String,
        std::path::PathBuf,
        proxy_core::auth_watcher::SharedAuth,
    ),
    String,
> {
    let mut config =
        proxy_core::config::load_config().map_err(|e| format!("load config failed: {e}"))?;

    let db_path = proxy_core::config::default_db_path()
        .map_err(|e| format!("resolve database path failed: {e}"))?;
    let runtime =
        tokio::runtime::Runtime::new().map_err(|e| format!("create tokio runtime failed: {e}"))?;
    let db = runtime
        .block_on(proxy_core::db::Database::new(&db_path))
        .map_err(|e| format!("init database failed: {e}"))?;

    if let Some(port) = runtime
        .block_on(db.get_setting("port"))
        .map_err(|e| format!("load port setting failed: {e}"))?
    {
        config.port = port
            .parse::<u16>()
            .map_err(|e| format!("invalid saved port setting: {e}"))?;
        if config.port == 0 {
            return Err("invalid saved port setting: must be between 1 and 65535".to_string());
        }
    }

    if let Some(auth_path) = runtime
        .block_on(db.get_setting("auth_path"))
        .map_err(|e| format!("load auth_path setting failed: {e}"))?
    {
        config.auth_path = proxy_core::config::expand_path(&auth_path)
            .map_err(|e| format!("invalid saved auth_path setting: {e}"))?;
    }

    let auth = proxy_core::auth::load_auth(&config.auth_path)
        .map_err(|e| format!("load auth failed: {e}"))?;

    let auth_mode = match &auth.mode {
        proxy_core::auth::AuthMode::ChatGPT => "ChatGPT".to_string(),
        proxy_core::auth::AuthMode::ApiKey => "ApiKey".to_string(),
    };

    let shared_auth = proxy_core::auth_watcher::new_shared_auth(auth);
    let client = Arc::new(proxy_core::client::CodexClient::new_with_shared_auth(
        shared_auth.clone(),
        config.chatgpt_api_url.clone(),
    ));

    let state = proxy_core::routes::AppState {
        client,
        db: Arc::new(db),
        log_buffer: proxy_core::logger::new_log_buffer(),
    };

    Ok((
        Arc::new(state),
        config.port,
        auth_mode,
        config.auth_path,
        shared_auth,
    ))
}

fn start_proxy_server_with_status(
    app_handle: tauri::AppHandle,
    proxy_state: Arc<proxy_core::routes::AppState>,
    server_status: Arc<Mutex<ServerStatus>>,
    server_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    server_shutdown: Arc<Mutex<Option<tokio::sync::watch::Sender<bool>>>>,
    server_generation: Arc<Mutex<u64>>,
    server_stopping: Arc<Mutex<bool>>,
) {
    let (startup_generation, rx, exit_rx) = {
        let Ok(stopping_guard) = server_stopping.lock() else {
            tracing::warn!("server stopping lock poisoned");
            return;
        };
        if *stopping_guard {
            return;
        }

        let Ok(mut generation) = server_generation.lock() else {
            tracing::warn!("server generation lock poisoned");
            return;
        };
        let Ok(mut handle_guard) = server_handle.lock() else {
            tracing::warn!("server handle lock poisoned");
            return;
        };
        let Ok(mut shutdown_guard) = server_shutdown.lock() else {
            tracing::warn!("server shutdown lock poisoned");
            return;
        };
        if *stopping_guard || handle_guard.is_some() {
            return;
        }
        let Ok(status) = server_status.lock() else {
            tracing::warn!("server status lock poisoned");
            return;
        };
        let port = status.port;
        drop(status);

        *generation = generation.saturating_add(1);
        let startup_generation = *generation;
        let (handle, shutdown_tx, rx, exit_rx) = commands::spawn_server_task(proxy_state, port);
        *handle_guard = Some(handle);
        *shutdown_guard = Some(shutdown_tx);
        (startup_generation, rx, exit_rx)
    };

    let app_handle_for_bind = app_handle.clone();
    let server_status_for_bind = server_status.clone();
    let server_handle_for_bind = server_handle.clone();
    let server_shutdown_for_bind = server_shutdown.clone();
    let server_generation_for_bind = server_generation.clone();
    tauri::async_runtime::spawn(async move {
        let bind_result = tokio::time::timeout(Duration::from_secs(2), rx).await;
        match bind_result {
            Ok(Ok(Ok(()))) => {
                if !is_current_server_generation(&server_generation_for_bind, startup_generation) {
                    return;
                }
                if let Ok(mut status) = server_status_for_bind.lock() {
                    status.running = true;
                    status.error = None;
                    status.started_at = Some(Instant::now());
                    status.uptime_secs = 0;
                }
                if let Some(tray) = app_handle_for_bind.tray_by_id("main-tray") {
                    update_tray_icon_for_state(&tray, true);
                }
                let _ = app_handle_for_bind.emit("server-status-changed", ());
                spawn_server_exit_monitor(
                    app_handle_for_bind.clone(),
                    server_status_for_bind.clone(),
                    server_handle_for_bind.clone(),
                    server_shutdown_for_bind.clone(),
                    server_generation_for_bind.clone(),
                    startup_generation,
                    exit_rx,
                );
            }
            Ok(Ok(Err(err_msg))) => {
                if !is_current_server_generation(&server_generation_for_bind, startup_generation) {
                    return;
                }
                if let Ok(mut handle_guard) = server_handle_for_bind.lock() {
                    if let Some(handle) = handle_guard.take() {
                        handle.abort();
                    }
                }
                if let Ok(mut shutdown_guard) = server_shutdown_for_bind.lock() {
                    let _ = shutdown_guard.take();
                }
                if let Ok(mut status) = server_status_for_bind.lock() {
                    status.running = false;
                    status.error = Some(err_msg.clone());
                    status.started_at = None;
                }
                let _ = app_handle_for_bind.emit("port-conflict", &err_msg);
            }
            Ok(Err(_recv_err)) => {
                if !is_current_server_generation(&server_generation_for_bind, startup_generation) {
                    return;
                }
                if let Ok(mut handle_guard) = server_handle_for_bind.lock() {
                    if let Some(handle) = handle_guard.take() {
                        handle.abort();
                    }
                }
                if let Ok(mut shutdown_guard) = server_shutdown_for_bind.lock() {
                    let _ = shutdown_guard.take();
                }
                let err_msg = "서버 태스크가 예기치 않게 종료됨".to_string();
                if let Ok(mut status) = server_status_for_bind.lock() {
                    status.running = false;
                    status.error = Some(err_msg.clone());
                    status.started_at = None;
                }
                let _ = app_handle_for_bind.emit("server-error", &err_msg);
            }
            Err(_) => {
                if !is_current_server_generation(&server_generation_for_bind, startup_generation) {
                    return;
                }
                if let Ok(mut handle_guard) = server_handle_for_bind.lock() {
                    if let Some(handle) = handle_guard.take() {
                        handle.abort();
                    }
                }
                if let Ok(mut shutdown_guard) = server_shutdown_for_bind.lock() {
                    let _ = shutdown_guard.take();
                }
                let err_msg = "Timed out while starting proxy server".to_string();
                if let Ok(mut status) = server_status_for_bind.lock() {
                    status.running = false;
                    status.error = Some(err_msg.clone());
                    status.started_at = None;
                }
                let _ = app_handle_for_bind.emit("server-error", &err_msg);
            }
        }
    });
}

fn is_current_server_generation(
    server_generation: &Arc<Mutex<u64>>,
    startup_generation: u64,
) -> bool {
    server_generation
        .lock()
        .map(|generation| *generation == startup_generation)
        .unwrap_or(false)
}

fn spawn_server_exit_monitor(
    app_handle: tauri::AppHandle,
    server_status: Arc<Mutex<ServerStatus>>,
    server_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    server_shutdown: Arc<Mutex<Option<tokio::sync::watch::Sender<bool>>>>,
    server_generation: Arc<Mutex<u64>>,
    startup_generation: u64,
    exit_rx: tokio::sync::oneshot::Receiver<String>,
) {
    tauri::async_runtime::spawn(async move {
        let Ok(message) = exit_rx.await else {
            return;
        };
        if !is_current_server_generation(&server_generation, startup_generation) {
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
        if let Some(tray) = app_handle.tray_by_id("main-tray") {
            update_tray_icon_for_state(&tray, false);
        }
        let _ = app_handle.emit("server-status-changed", ());
        let _ = app_handle.emit("server-error", &message);
    });
}

fn spawn_log_event_bridge(
    app_handle: tauri::AppHandle,
    proxy_state: Arc<proxy_core::routes::AppState>,
) {
    tauri::async_runtime::spawn(async move {
        let mut last_seen_log_id: Option<String> = None;
        loop {
            let mut ordered = proxy_core::logger::get_recent_logs(&proxy_state.log_buffer, 200);
            ordered.reverse();

            let mut emit_queue = Vec::new();
            if let Some(last_seen) = &last_seen_log_id {
                if let Some(index) = ordered.iter().position(|entry| &entry.id == last_seen) {
                    emit_queue.extend(ordered.into_iter().skip(index + 1));
                } else {
                    emit_queue.extend(ordered.into_iter());
                }
            } else if let Some(last) = ordered.last() {
                last_seen_log_id = Some(last.id.clone());
            }

            for log in emit_queue {
                last_seen_log_id = Some(log.id.clone());
                if let Err(e) = app_handle.emit("log-entry", &log) {
                    tracing::warn!(error = %e, "failed to emit log-entry");
                }
            }

            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    });
}

pub(crate) fn update_tray_icon_for_state(
    tray: &tauri::tray::TrayIcon<impl tauri::Runtime>,
    running: bool,
) {
    if running {
        if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
        {
            let _ = tray.set_icon(Some(icon));
        }
    } else {
        let icon = tauri::image::Image::new(&ERROR_ICON_RGBA, 32, 32);
        let _ = tray.set_icon(Some(icon));
    }
    tracing::debug!(
        state = if running { "running" } else { "error" },
        "tray icon state updated"
    );
}

pub(crate) fn update_tray_icon_for_stopped(tray: &tauri::tray::TrayIcon<impl tauri::Runtime>) {
    if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png")) {
        let _ = tray.set_icon(Some(icon));
    }
    tracing::debug!(state = "stopped", "tray icon state updated");
}

fn show_popover(window: &tauri::WebviewWindow) {
    let _ = window.move_window(Position::TrayCenter);
    let _ = window.show();
    let _ = window.set_focus();
}

fn toggle_popover(window: &tauri::WebviewWindow) {
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        show_popover(window);
    }
}

pub fn run() {
    let builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_server_status,
            commands::start_server,
            commands::stop_server,
            commands::get_settings,
            commands::update_setting,
            commands::get_recent_logs,
            commands::get_token_usage,
            commands::get_models
        ])
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_shell::init());

    #[cfg(debug_assertions)]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let is_wdio_embedded = std::env::var("WDIO_EMBEDDED_SERVER").as_deref() == Ok("true");

            let show_dashboard =
                MenuItem::with_id(app, "show_dashboard", "Show Dashboard", true, None::<&str>)?;
            let start_server =
                MenuItem::with_id(app, "start_server", "Start Server", true, None::<&str>)?;
            let stop_server =
                MenuItem::with_id(app, "stop_server", "Stop Server", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &show_dashboard,
                    &start_server,
                    &stop_server,
                    &separator,
                    &quit_item,
                ],
            )?;

            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
                .expect("embedded tray icon must be valid PNG");

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("oorouter")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show_dashboard" => {
                        if let Some(w) = app.get_webview_window("main") {
                            show_popover(&w);
                        }
                    }
                    "start_server" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app_handle.state::<TauriAppState>();
                            start_proxy_server_with_status(
                                app_handle.clone(),
                                state.proxy_state.clone(),
                                state.server_status.clone(),
                                state.server_handle.clone(),
                                state.server_shutdown.clone(),
                                state.server_generation.clone(),
                                state.server_stopping.clone(),
                            );
                        });
                    }
                    "stop_server" => {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app_handle.state::<TauriAppState>();
                            if let Err(error) = commands::stop_server_with_parts(
                                app_handle.clone(),
                                state.server_status.clone(),
                                state.server_handle.clone(),
                                state.server_shutdown.clone(),
                                state.server_generation.clone(),
                                state.server_stopping.clone(),
                            )
                            .await
                            {
                                tracing::warn!(%error, "failed to stop proxy server from tray");
                            }
                        });
                    }
                    "quit" => {
                        std::process::exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    let app = tray.app_handle();
                    tauri_plugin_positioner::on_tray_event(app, &event);

                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(w) = app.get_webview_window("main") {
                            toggle_popover(&w);
                        }
                    }
                })
                .build(app)?;

            if let Some(w) = app.get_webview_window("main") {
                if is_wdio_embedded {
                    let _ = w.show();
                    let _ = w.set_focus();
                } else {
                    let win = w.clone();
                    w.on_window_event(move |event| {
                        if let WindowEvent::Focused(false) = event {
                            let _ = win.hide();
                        }
                    });
                }
            }

            let app_for_conflict = app.app_handle().clone();
            app.listen("port-conflict", move |event| {
                let raw = event.payload();
                let err_msg =
                    serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_string());

                if let Err(e) = app_for_conflict
                    .notification()
                    .builder()
                    .title("Codex Ollama Proxy — 포트 충돌")
                    .body(&err_msg)
                    .show()
                {
                    tracing::warn!(error = %e, "notification send failed");
                }

                if let Some(tray) = app_for_conflict.tray_by_id("main-tray") {
                    update_tray_icon_for_state(&tray, false);
                }

                let _ = app_for_conflict.emit("navigate-to-settings", ());
            });

            let app_for_server_error = app.app_handle().clone();
            app.listen("server-error", move |event| {
                let raw = event.payload();
                let err_msg =
                    serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_string());

                if let Err(e) = app_for_server_error
                    .notification()
                    .builder()
                    .title("Codex Ollama Proxy — 서버 오류")
                    .body(&err_msg)
                    .show()
                {
                    tracing::warn!(error = %e, "notification send failed");
                }

                if let Some(tray) = app_for_server_error.tray_by_id("main-tray") {
                    update_tray_icon_for_state(&tray, false);
                }
            });

            let (proxy_state, port, auth_mode, auth_path, shared_auth) =
                create_proxy_state().map_err(std::io::Error::other)?;

            // auth.json 변경 감지 watcher 시작 (에러 시 warn만, 앱 종료 안 함)
            let auth_watcher = match proxy_core::auth_watcher::start_auth_watcher(
                auth_path,
                shared_auth.clone(),
            ) {
                Ok(watcher) => {
                    tracing::info!("auth watcher started");
                    Some(watcher)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "auth watcher start failed");
                    None
                }
            };
            app.manage(TauriAppState {
                proxy_state: proxy_state.clone(),
                server_status: Arc::new(Mutex::new(ServerStatus {
                    running: false,
                    port,
                    uptime_secs: 0,
                    auth_mode,
                    error: None,
                    started_at: None,
                })),
                server_handle: Arc::new(Mutex::new(None)),
                server_shutdown: Arc::new(Mutex::new(None)),
                server_generation: Arc::new(Mutex::new(0)),
                server_stopping: Arc::new(Mutex::new(false)),
                shared_auth,
                auth_watcher: Arc::new(Mutex::new(auth_watcher)),
            });

            let state = app.state::<TauriAppState>();
            start_proxy_server_with_status(
                app.app_handle().clone(),
                state.proxy_state.clone(),
                state.server_status.clone(),
                state.server_handle.clone(),
                state.server_shutdown.clone(),
                state.server_generation.clone(),
                state.server_stopping.clone(),
            );

            spawn_log_event_bridge(app.app_handle().clone(), proxy_state.clone());

            let auto_start_enabled = tokio::runtime::Runtime::new()
                .ok()
                .and_then(|rt| rt.block_on(proxy_state.db.get_setting("auto_start")).ok())
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true);

            let autolaunch = app.autolaunch();
            if !is_wdio_embedded {
                if auto_start_enabled {
                    let _ = autolaunch.enable();
                } else {
                    let _ = autolaunch.disable();
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build tauri application")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
