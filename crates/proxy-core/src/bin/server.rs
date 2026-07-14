use std::{
    io::{self, Write},
    sync::Arc,
};

use proxy_core::{
    auth::load_auth,
    auth_watcher::{new_shared_auth, start_auth_watcher},
    client::CodexClient,
    client_auth::{ClientAuthRuntime, ClientAuthSnapshot},
    config::load_config,
    db::Database,
    logger::new_log_buffer,
    routes::{create_router, AppState},
    server_args::{parse_server_args, SERVER_USAGE},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server_args = parse_server_args(std::env::args_os().skip(1))?;
    if server_args.show_help {
        io::stdout().write_all(SERVER_USAGE.as_bytes())?;
        return Ok(());
    }

    tracing_subscriber::fmt().with_writer(io::stderr).init();

    let config = load_config()?;
    let bind_addr = std::net::SocketAddr::new(server_args.host, config.port);
    if !server_args.host.is_loopback() && server_args.api_keys.is_empty() {
        tracing::warn!(
            %bind_addr,
            "standalone server is exposed beyond loopback without client API authentication"
        );
    }
    let client_auth_snapshot = if server_args.api_keys.is_empty() {
        ClientAuthSnapshot::disabled()
    } else {
        ClientAuthSnapshot::enabled(server_args.api_keys)?
    };
    let client_auth = ClientAuthRuntime::new(client_auth_snapshot);

    let auth = load_auth(&config.auth_path)?;
    let shared_auth = new_shared_auth(auth);
    let client = Arc::new(CodexClient::new_with_shared_auth(
        shared_auth.clone(),
        config.chatgpt_api_url.clone(),
    ));
    let _auth_watcher = match start_auth_watcher(config.auth_path.clone(), shared_auth) {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            tracing::warn!(%error, "auth watcher start failed");
            None
        }
    };

    let db_path = proxy_core::config::default_db_path()?;
    let db = Database::new(&db_path).await?;

    let state = AppState {
        client,
        client_auth,
        db: Arc::new(db),
        log_buffer: new_log_buffer(),
    };

    let router = create_router(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    if bind_addr.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST) {
        let ipv6_addr = std::net::SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, config.port));
        match tokio::net::TcpListener::bind(ipv6_addr).await {
            Ok(ipv6_listener) => {
                tracing::info!("proxy listening on http://{bind_addr} and http://{ipv6_addr}");
                tokio::try_join!(
                    axum::serve(listener, router.clone()),
                    axum::serve(ipv6_listener, router),
                )?;
            }
            Err(error) => {
                tracing::warn!(%error, "IPv6 loopback bind failed; continuing with IPv4 only");
                tracing::info!("proxy listening on http://{bind_addr}");
                axum::serve(listener, router).await?;
            }
        }
    } else {
        tracing::info!("proxy listening on http://{bind_addr}");
        axum::serve(listener, router).await?;
    }

    Ok(())
}
