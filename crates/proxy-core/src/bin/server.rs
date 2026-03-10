use std::sync::Arc;

use proxy_core::{
    auth::load_auth,
    client::CodexClient,
    config::load_config,
    db::Database,
    logger::new_log_buffer,
    routes::{AppState, create_router},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = load_config()?;
    let auth = load_auth(&config.auth_path)?;
    let client = Arc::new(CodexClient::new(auth, config.chatgpt_api_url.clone()));

    let db_path = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".local/share/codex-ollama-proxy"))
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/codex-ollama-proxy"))
        .join("proxy.db");
    let db = Database::new(&db_path).await?;

    let state = AppState {
        client,
        db: Arc::new(db),
        log_buffer: new_log_buffer(),
    };

    let router = create_router(state);
    let addr = std::net::SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("[proxy] Listening on http://{addr}");
    axum::serve(listener, router).await?;
    Ok(())
}
