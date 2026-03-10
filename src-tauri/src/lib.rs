mod commands;
mod state;

use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Listener, Manager, RunEvent, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_autostart::ManagerExt;
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

fn create_proxy_state() -> Result<(Arc<proxy_core::routes::AppState>, u16, String), String> {
    let config = proxy_core::config::load_config().map_err(|e| format!("load config failed: {e}"))?;

    let auth =
        proxy_core::auth::load_auth(&config.auth_path).map_err(|e| format!("load auth failed: {e}"))?;

    let auth_mode = match auth.mode {
        proxy_core::auth::AuthMode::ChatGPT => "ChatGPT".to_string(),
        proxy_core::auth::AuthMode::ApiKey => "ApiKey".to_string(),
    };

    let client = Arc::new(proxy_core::client::CodexClient::new(
        auth,
        config.chatgpt_api_url.clone(),
    ));

    let db_path = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".local/share/codex-ollama-proxy"))
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/codex-ollama-proxy"))
        .join("proxy.db");

    let db = tokio::runtime::Runtime::new()
        .map_err(|e| format!("create tokio runtime failed: {e}"))?
        .block_on(proxy_core::db::Database::new(&db_path))
        .map_err(|e| format!("init database failed: {e}"))?;

    let state = proxy_core::routes::AppState {
        client,
        db: Arc::new(db),
        log_buffer: proxy_core::logger::new_log_buffer(),
    };

    Ok((Arc::new(state), config.port, auth_mode))
}

fn spawn_proxy_server(
    proxy_state: Arc<proxy_core::routes::AppState>,
    port: u16,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let router = proxy_core::routes::create_router((*proxy_state).clone());
        let addr = std::net::SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, port));
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[proxy] Failed to bind {addr}: {e}");
                return;
            }
        };

        eprintln!("[proxy] Listening on http://{addr}");
        if let Err(e) = axum::serve(listener, router).await {
            eprintln!("[proxy] Server error: {e}");
        }
    })
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
                    eprintln!("[tauri] failed to emit log-entry: {e}");
                }
            }

            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    });
}

fn update_tray_icon_for_state(
    tray: &tauri::tray::TrayIcon<impl tauri::Runtime>,
    running: bool,
) {
    if running {
        if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")) {
            let _ = tray.set_icon(Some(icon));
        }
    } else {
        let icon = tauri::image::Image::new(&ERROR_ICON_RGBA, 32, 32);
        let _ = tray.set_icon(Some(icon));
    }
    eprintln!(
        "[tray] icon state: {}",
        if running { "running" } else { "error" }
    );
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
    tauri::Builder::default()
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
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let show_dashboard =
                MenuItem::with_id(app, "show_dashboard", "Show Dashboard", true, None::<&str>)?;
            let start_server =
                MenuItem::with_id(app, "start_server", "Start Server", true, None::<&str>)?;
            let stop_server =
                MenuItem::with_id(app, "stop_server", "Stop Server", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_item =
                MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

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

            let icon = app
                .default_window_icon()
                .cloned()
                .unwrap_or_else(|| {
                    tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
                        .expect("embedded tray icon must be valid PNG")
                });

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Codex Ollama Proxy")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show_dashboard" => {
                        if let Some(w) = app.get_webview_window("main") {
                            show_popover(&w);
                        }
                    }
                    "start_server" => {
                        eprintln!("[tray] start_server requested");
                    }
                    "stop_server" => {
                        eprintln!("[tray] stop_server requested");
                    }
                    "quit" => {
                        app.exit(0);
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
                let win = w.clone();
                w.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let _ = win.hide();
                    }
                });
            }

            let app_for_conflict = app.app_handle().clone();
            app.listen("port-conflict", move |event| {
                let raw = event.payload();
                let err_msg = serde_json::from_str::<String>(raw)
                    .unwrap_or_else(|_| raw.to_string());

                if let Err(e) = app_for_conflict
                    .notification()
                    .builder()
                    .title("Codex Ollama Proxy — 포트 충돌")
                    .body(&err_msg)
                    .show()
                {
                    eprintln!("[notification] 알림 전송 실패: {e}");
                }

                if let Some(tray) = app_for_conflict.tray_by_id("main-tray") {
                    update_tray_icon_for_state(&tray, false);
                }

                let _ = app_for_conflict.emit("navigate-to-settings", ());
            });

            let (proxy_state, port, auth_mode) = create_proxy_state().map_err(std::io::Error::other)?;

            // auth.json 변경 감지 watcher 시작 (에러 시 warn만, 앱 종료 안 함)
            if let Ok(config) = proxy_core::config::load_config() {
                if let Ok(auth) = proxy_core::auth::load_auth(&config.auth_path) {
                    let shared = proxy_core::auth_watcher::new_shared_auth(auth);
                    match proxy_core::auth_watcher::start_auth_watcher(config.auth_path, shared) {
                        Ok(watcher) => {
                            // watcher를 drop하면 감시 중지되므로 leak으로 영구 유지
                            Box::leak(Box::new(watcher));
                            eprintln!("[auth] auth.json watcher started");
                        }
                        Err(e) => eprintln!("[auth] watcher start failed: {e}"),
                    }
                }
            }
            let server_handle = spawn_proxy_server(proxy_state.clone(), port);

            app.manage(TauriAppState {
                proxy_state: proxy_state.clone(),
                server_status: Arc::new(Mutex::new(ServerStatus {
                    running: true,
                    port,
                    uptime_secs: 0,
                    auth_mode,
                    error: None,
                })),
                server_handle: Arc::new(Mutex::new(Some(server_handle))),
                start_time: Instant::now(),
            });

            spawn_log_event_bridge(app.app_handle().clone(), proxy_state);


            // 자동시작 기본 ON
            if let Err(e) = app.autolaunch().enable() {
                eprintln!("[autostart] enable failed: {e}");
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
