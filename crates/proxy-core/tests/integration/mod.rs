use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Router,
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::post,
};
use proxy_core::{
    auth::{AuthInfo, AuthMode},
    client::CodexClient,
    db::Database,
    logger::new_log_buffer,
    routes::{AppState, create_router},
};
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};

async fn spawn_mock_backend() -> (String, Arc<AtomicUsize>, JoinHandle<()>) {
    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_for_handler = Arc::clone(&request_count);

    let app = Router::new().route(
        "/backend-api/codex/responses",
        post(move || {
            let request_count_for_handler = Arc::clone(&request_count_for_handler);
            async move {
                request_count_for_handler.fetch_add(1, Ordering::SeqCst);

                let sse = concat!(
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"mock\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n"
                );

                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"))
                    .body(Body::from(sse))
                    .expect("mock backend response build")
            }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock backend listener");
    let addr = listener.local_addr().expect("mock backend local addr");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("run mock backend server");
    });

    (
        format!("http://{}/backend-api/codex/responses", addr),
        request_count,
        handle,
    )
}

async fn spawn_proxy_server(api_url: String) -> (String, TempDir, JoinHandle<()>) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("integration.db");
    let db = Arc::new(Database::new(&db_path).await.expect("create sqlite db"));
    let client = Arc::new(CodexClient::new(
        AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: "test-key".to_string(),
            account_id: None,
        },
        api_url,
    ));

    let state = AppState {
        client,
        db,
        log_buffer: new_log_buffer(),
    };

    let app = create_router(state);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind proxy listener");
    let addr = listener.local_addr().expect("proxy local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("run proxy server");
    });

    (format!("http://{}", addr), temp_dir, handle)
}

#[tokio::test]
async fn integration_endpoints_with_live_axum_server() {
    let (mock_api_url, mock_request_count, mock_handle) = spawn_mock_backend().await;
    let (base_url, _temp_dir, proxy_handle) = spawn_proxy_server(mock_api_url).await;
    let http = reqwest::Client::new();

    let root = http
        .get(format!("{}/", base_url))
        .send()
        .await
        .expect("GET /");
    assert_eq!(root.status(), StatusCode::OK);
    assert_eq!(root.text().await.expect("root text"), "Ollama is running");

    let tags = http
        .get(format!("{}/api/tags", base_url))
        .send()
        .await
        .expect("GET /api/tags");
    assert_eq!(tags.status(), StatusCode::OK);
    let tags_json: serde_json::Value = tags.json().await.expect("tags json");
    let models = tags_json
        .get("models")
        .and_then(|v| v.as_array())
        .expect("models array");
    assert!(!models.is_empty());

    let version = http
        .get(format!("{}/api/version", base_url))
        .send()
        .await
        .expect("GET /api/version");
    assert_eq!(version.status(), StatusCode::OK);
    let version_json: serde_json::Value = version.json().await.expect("version json");
    assert!(version_json
        .get("version")
        .and_then(|v| v.as_str())
        .map(|v| !v.is_empty())
        .unwrap_or(false));

    let show_ok = http
        .post(format!("{}/api/show", base_url))
        .json(&serde_json::json!({ "name": "gpt-5.3-codex" }))
        .send()
        .await
        .expect("POST /api/show valid");
    assert_eq!(show_ok.status(), StatusCode::OK);

    let show_missing = http
        .post(format!("{}/api/show", base_url))
        .json(&serde_json::json!({ "name": "not-a-real-model" }))
        .send()
        .await
        .expect("POST /api/show missing");
    assert_eq!(show_missing.status(), StatusCode::NOT_FOUND);

    let ps = http
        .get(format!("{}/api/ps", base_url))
        .send()
        .await
        .expect("GET /api/ps");
    assert_eq!(ps.status(), StatusCode::OK);

    let copy = http
        .post(format!("{}/api/copy", base_url))
        .json(&serde_json::json!({ "source": "a", "destination": "b" }))
        .send()
        .await
        .expect("POST /api/copy");
    assert_eq!(copy.status(), StatusCode::OK);

    let delete = http
        .delete(format!("{}/api/delete", base_url))
        .json(&serde_json::json!({ "name": "gpt-5" }))
        .send()
        .await
        .expect("DELETE /api/delete");
    assert_eq!(delete.status(), StatusCode::OK);

    let pull = http
        .post(format!("{}/api/pull", base_url))
        .json(&serde_json::json!({ "name": "gpt-5" }))
        .send()
        .await
        .expect("POST /api/pull");
    assert_eq!(pull.status(), StatusCode::OK);

    let push = http
        .post(format!("{}/api/push", base_url))
        .json(&serde_json::json!({ "name": "gpt-5" }))
        .send()
        .await
        .expect("POST /api/push");
    assert_eq!(push.status(), StatusCode::OK);

    let models_v1 = http
        .get(format!("{}/v1/models", base_url))
        .send()
        .await
        .expect("GET /v1/models");
    assert_eq!(models_v1.status(), StatusCode::OK);
    let models_v1_json: serde_json::Value = models_v1.json().await.expect("v1/models json");
    let data = models_v1_json
        .get("data")
        .and_then(|v| v.as_array())
        .expect("data array");
    assert!(!data.is_empty());

    assert_eq!(mock_request_count.load(Ordering::SeqCst), 0);

    proxy_handle.abort();
    mock_handle.abort();
}
