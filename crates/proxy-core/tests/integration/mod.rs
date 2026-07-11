use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use proxy_core::{
    auth::{AuthInfo, AuthMode},
    client::CodexClient,
    db::Database,
    logger::new_log_buffer,
    routes::{create_router, AppState},
};
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};

async fn spawn_mock_backend() -> (String, Arc<AtomicUsize>, Arc<AtomicUsize>, JoinHandle<()>) {
    let responses_request_count = Arc::new(AtomicUsize::new(0));
    let models_request_count = Arc::new(AtomicUsize::new(0));
    let responses_request_count_for_handler = Arc::clone(&responses_request_count);
    let models_request_count_for_handler = Arc::clone(&models_request_count);

    let app = Router::new()
        .route(
            "/backend-api/codex/responses",
            post(move |Json(payload): Json<serde_json::Value>| {
                let responses_request_count_for_handler =
                    Arc::clone(&responses_request_count_for_handler);
                async move {
                    responses_request_count_for_handler.fetch_add(1, Ordering::SeqCst);
                    assert!(
                        payload.get("max_output_tokens").is_none(),
                        "proxy should strip max_output_tokens before forwarding to Codex"
                    );
                    assert_eq!(payload["stream"], true);

                    let sse = concat!(
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"mock\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"object\":\"response\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"mock\"}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n"
                    );

                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"))
                        .body(Body::from(sse))
                        .expect("mock backend response build")
                }
            }),
        )
        .route(
            "/backend-api/codex/models",
            get(move || {
                let models_request_count_for_handler =
                    Arc::clone(&models_request_count_for_handler);
                async move {
                    models_request_count_for_handler.fetch_add(1, Ordering::SeqCst);
                    Json(serde_json::json!({
                        "models": [
                            { "slug": "mock-codex" },
                            { "slug": "mock-codex" },
                            { "slug": "mock-spark" }
                        ]
                    }))
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
        responses_request_count,
        models_request_count,
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
        axum::serve(listener, app).await.expect("run proxy server");
    });

    (format!("http://{}", addr), temp_dir, handle)
}

#[tokio::test]
async fn integration_endpoints_with_live_axum_server() {
    let (mock_api_url, responses_request_count, models_request_count, mock_handle) =
        spawn_mock_backend().await;
    let (base_url, _temp_dir, proxy_handle) = spawn_proxy_server(mock_api_url).await;
    let http = reqwest::Client::new();

    let root = http
        .get(format!("{}/", base_url))
        .send()
        .await
        .expect("GET /");
    assert_eq!(root.status(), StatusCode::OK);
    assert_eq!(root.text().await.expect("root text"), "Ollama is running");

    let health = http
        .get(format!("{}/health", base_url))
        .send()
        .await
        .expect("GET /health");
    assert_eq!(health.status(), StatusCode::OK);
    let health_json: serde_json::Value = health.json().await.expect("health json");
    assert_eq!(health_json["ok"], true);
    assert_eq!(health_json["replay_state"], "stateless");

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
    let tag_names: Vec<&str> = models
        .iter()
        .map(|model| model["name"].as_str().expect("tag model name"))
        .collect();
    assert!(tag_names.contains(&"gpt-5.6-sol:latest"));
    assert!(tag_names.contains(&"gpt-5.6-terra:latest"));
    assert!(tag_names.contains(&"gpt-5.6-luna:latest"));

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
        .json(&serde_json::json!({ "name": "gpt-5.6" }))
        .send()
        .await
        .expect("POST /api/show valid");
    assert_eq!(show_ok.status(), StatusCode::OK);
    let show_json: serde_json::Value = show_ok.json().await.expect("POST /api/show json");
    assert_eq!(show_json["model_info"]["gpt.context_length"], 372_000);
    assert!(show_json["capabilities"]
        .as_array()
        .expect("show capabilities")
        .iter()
        .any(|capability| capability == "vision"));

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
    let model_ids: Vec<&str> = data
        .iter()
        .map(|model| model["id"].as_str().expect("model id string"))
        .collect();
    assert_eq!(
        model_ids,
        vec![
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "gpt-5.5",
            "gpt-5.4",
            "gpt-5.3-codex",
            "gpt-5.2-codex",
            "gpt-5.2",
            "gpt-5.3-codex-spark",
            "mock-codex",
            "mock-spark",
        ]
    );
    assert!(data
        .iter()
        .all(|model| model["object"] == "model" && model["created"] == 0));

    let responses_v1 = http
        .post(format!("{}/v1/responses", base_url))
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "input": [{ "role": "user", "content": "hello" }],
            "stream": false,
            "max_output_tokens": 8
        }))
        .send()
        .await
        .expect("POST /v1/responses");
    assert_eq!(responses_v1.status(), StatusCode::OK);
    let responses_v1_json: serde_json::Value =
        responses_v1.json().await.expect("v1/responses json");
    assert_eq!(responses_v1_json["id"], "resp-1");
    assert_eq!(responses_v1_json["object"], "response");
    assert_eq!(responses_v1_json["usage"]["total_tokens"], 2);

    let responses_v1_stream = http
        .post(format!("{}/v1/responses", base_url))
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "input": [{ "role": "user", "content": "hello" }],
            "stream": true
        }))
        .send()
        .await
        .expect("POST /v1/responses stream");
    assert_eq!(responses_v1_stream.status(), StatusCode::OK);
    assert!(responses_v1_stream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream")));
    let responses_stream_body = responses_v1_stream
        .text()
        .await
        .expect("v1/responses stream body");
    assert!(responses_stream_body.contains("response.output_text.delta"));

    let chat_v1 = http
        .post(format!("{}/v1/chat/completions", base_url))
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "messages": [{ "role": "user", "content": "hello" }],
            "temperature": 0.7,
            "top_p": 0.9,
            "max_tokens": 8,
            "reasoning_effort": "low",
            "stream": false
        }))
        .send()
        .await
        .expect("POST /v1/chat/completions");
    assert_eq!(chat_v1.status(), StatusCode::OK);
    let chat_v1_json: serde_json::Value = chat_v1.json().await.expect("v1/chat json");
    assert_eq!(chat_v1_json["object"], "chat.completion");
    assert_eq!(chat_v1_json["choices"][0]["message"]["content"], "mock");

    assert_eq!(models_request_count.load(Ordering::SeqCst), 1);
    assert_eq!(responses_request_count.load(Ordering::SeqCst), 3);

    proxy_handle.abort();
    mock_handle.abort();
}
