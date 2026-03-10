use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json, Router,
    http::StatusCode,
    response::Response,
    routing::{delete, get, post},
};
use tower_http::cors::CorsLayer;

use crate::{client::CodexClient, db::Database, error::ProxyError, logger::LogBuffer};

pub mod chat;
pub mod generate;
pub mod openai;
pub mod tags;
pub mod show;
pub mod stubs;

#[derive(Clone)]
pub struct AppState {
    pub client: Arc<CodexClient>,
    pub db: Arc<Database>,
    pub log_buffer: LogBuffer,
}

pub type RouteResult = std::result::Result<Response, (StatusCode, Json<serde_json::Value>)>;

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::very_permissive()
        .expose_headers([
            axum::http::header::HOST,
            axum::http::header::USER_AGENT,
            axum::http::header::ACCEPT,
            axum::http::header::ORIGIN,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::CONTENT_LENGTH,
            "access-control-request-method".parse().unwrap(),
        ])
        .max_age(Duration::from_secs(5));

    Router::new()
        .route("/", get(stubs::health))
        .route("/api/chat", post(chat::handle_chat))
        .route("/api/generate", post(generate::handle_generate))
        .route("/api/tags", get(tags::get_tags))
        .route("/api/show", post(show::post_show))
        .route("/api/embed", post(stubs::post_embed))
        .route("/api/ps", get(stubs::get_ps))
        .route("/api/version", get(stubs::get_version))
        .route("/api/copy", post(stubs::post_copy))
        .route("/api/delete", delete(stubs::delete_model))
        .route("/api/pull", post(stubs::post_pull))
        .route("/api/push", post(stubs::post_push))
        .route("/v1/chat/completions", post(openai::post_chat_completions))
        .route("/v1/models", get(openai::get_models))
        .layer(cors)
        .with_state(state)
}

pub(crate) fn map_proxy_error(error: ProxyError) -> (StatusCode, Json<serde_json::Value>) {
    let status = match &error {
        ProxyError::BackendApiError(_) => StatusCode::BAD_GATEWAY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (
        status,
        Json(serde_json::json!({
            "error": error.to_string(),
        })),
    )
}
