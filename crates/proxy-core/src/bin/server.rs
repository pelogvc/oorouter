use std::sync::Arc;

use proxy_core::{
    auth::load_auth,
    auth_watcher::{new_shared_auth, start_auth_watcher},
    client::CodexClient,
    config::load_config,
    db::Database,
    logger::new_log_buffer,
    routes::{create_router, AppState},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = load_config()?;
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
        db: Arc::new(db),
        log_buffer: new_log_buffer(),
    };

    let router = create_router(state);
    let ipv4_addr = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, config.port));
    let ipv6_addr = std::net::SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, config.port));
    let ipv4_listener = tokio::net::TcpListener::bind(ipv4_addr).await?;
    let ipv6_listener = match tokio::net::TcpListener::bind(ipv6_addr).await {
        Ok(listener) => Some(listener),
        Err(error) => {
            tracing::warn!(%error, "IPv6 loopback bind failed; continuing with IPv4 only");
            None
        }
    };

    if let Some(ipv6_listener) = ipv6_listener {
        tracing::info!("proxy listening on http://{ipv4_addr} and http://{ipv6_addr}");
        tokio::try_join!(
            axum::serve(ipv4_listener, router.clone()),
            axum::serve(ipv6_listener, router),
        )?;
    } else {
        tracing::info!("proxy listening on http://{ipv4_addr}");
        axum::serve(ipv4_listener, router).await?;
    }

    Ok(())
}
