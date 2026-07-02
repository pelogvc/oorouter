use std::sync::Arc;
use std::time::Duration;

use axum::{
    http::{header, HeaderValue, Method, StatusCode},
    response::Response,
    routing::{delete, get, post},
    Json, Router,
};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{client::CodexClient, db::Database, error::ProxyError, logger::LogBuffer};

pub mod chat;
pub mod generate;
pub mod logs;
pub mod openai;
pub mod show;
pub mod stubs;
pub mod tags;
pub mod usage_stats;

#[derive(Clone)]
pub struct AppState {
    pub client: Arc<CodexClient>,
    pub db: Arc<Database>,
    pub log_buffer: LogBuffer,
}

pub type RouteResult = std::result::Result<Response, (StatusCode, Json<serde_json::Value>)>;

fn is_allowed_local_origin(origin: &HeaderValue) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };

    if origin == "tauri://localhost" {
        return true;
    }

    let Ok(url) = reqwest::Url::parse(origin) else {
        return false;
    };

    matches!(
        (url.scheme(), url.host_str(), url.port()),
        ("http", Some("localhost"), Some(1420))
            | ("http", Some("127.0.0.1"), Some(1420))
            | ("http", Some("::1"), Some(1420))
    )
}

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            is_allowed_local_origin(origin)
        }))
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::HeaderName::from_static("openai-beta"),
            header::HeaderName::from_static("openai-organization"),
            header::HeaderName::from_static("openai-project"),
            header::HeaderName::from_static("openai-version"),
            header::HeaderName::from_static("x-request-id"),
            header::HeaderName::from_static("x-stainless-arch"),
            header::HeaderName::from_static("x-stainless-lang"),
            header::HeaderName::from_static("x-stainless-os"),
            header::HeaderName::from_static("x-stainless-package-version"),
            header::HeaderName::from_static("x-stainless-retry-count"),
            header::HeaderName::from_static("x-stainless-runtime"),
            header::HeaderName::from_static("x-stainless-runtime-version"),
            header::HeaderName::from_static("x-stainless-timeout"),
        ])
        .expose_headers([header::CONTENT_TYPE])
        .max_age(Duration::from_secs(5));

    Router::new()
        .route("/", get(stubs::health))
        .route("/health", get(openai::health))
        .route("/api/chat", post(chat::handle_chat))
        .route("/api/generate", post(generate::handle_generate))
        .route("/api/tags", get(tags::get_tags))
        .route("/api/show", post(show::post_show))
        .route("/api/embed", post(stubs::post_embed))
        .route("/api/ps", get(stubs::get_ps))
        .route("/api/version", get(stubs::get_version))
        .route("/api/logs", get(logs::get_logs))
        .route("/api/token-usage", get(usage_stats::get_token_usage))
        .route("/api/copy", post(stubs::post_copy))
        .route("/api/delete", delete(stubs::delete_model))
        .route("/api/pull", post(stubs::post_pull))
        .route("/api/push", post(stubs::post_push))
        .route("/v1/responses", post(openai::post_responses))
        .route("/v1/chat/completions", post(openai::post_chat_completions))
        .route("/v1/models", get(openai::get_models))
        .layer(cors)
        .with_state(state)
}

pub(crate) fn map_proxy_error(error: ProxyError) -> (StatusCode, Json<serde_json::Value>) {
    let (error_kind, status) = match &error {
        ProxyError::BackendApiError(_) => ("backend_api_error", StatusCode::BAD_GATEWAY),
        ProxyError::AuthError(_) => ("auth_error", StatusCode::UNAUTHORIZED),
        ProxyError::ConfigError(_) => ("config_error", StatusCode::BAD_REQUEST),
        ProxyError::JsonError(_) => ("json_error", StatusCode::INTERNAL_SERVER_ERROR),
        ProxyError::HttpError(_) => ("http_error", StatusCode::INTERNAL_SERVER_ERROR),
        ProxyError::IoError(_) => ("io_error", StatusCode::INTERNAL_SERVER_ERROR),
    };
    tracing::warn!(error_kind, "route error");
    let message = match &error {
        ProxyError::BackendApiError(_) => "Upstream backend request failed",
        ProxyError::AuthError(_) => "Authentication failed",
        ProxyError::ConfigError(_) => "Invalid proxy configuration",
        ProxyError::JsonError(_) => "Invalid JSON payload",
        ProxyError::HttpError(_) => "HTTP request failed",
        ProxyError::IoError(_) => "Internal IO error",
    };

    (
        status,
        Json(serde_json::json!({
            "error": message,
        })),
    )
}
