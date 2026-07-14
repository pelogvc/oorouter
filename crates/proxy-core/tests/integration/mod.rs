use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::{
    io::Read,
    process::{Child, Command, Output, Stdio},
    thread::JoinHandle as ThreadJoinHandle,
    time::Duration,
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
    client_auth::{ClientApiKey, ClientAuthRuntime, ClientAuthSnapshot},
    db::Database,
    logger::new_log_buffer,
    routes::{create_router, AppState},
};
use tempfile::TempDir;
use tokio::{net::TcpListener, sync::Notify, task::JoinHandle};

struct ChildProcess {
    child: Option<Child>,
    stdout_reader: Option<ThreadJoinHandle<Vec<u8>>>,
    stderr_reader: Option<ThreadJoinHandle<Vec<u8>>>,
}

impl ChildProcess {
    fn spawn(command: &mut Command) -> Self {
        let mut child = command.spawn().expect("spawn standalone proxy server");
        let stdout = child.stdout.take().expect("standalone stdout pipe");
        let stderr = child.stderr.take().expect("standalone stderr pipe");
        Self {
            child: Some(child),
            stdout_reader: Some(std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let mut stdout = stdout;
                stdout
                    .read_to_end(&mut bytes)
                    .expect("drain standalone stdout");
                bytes
            })),
            stderr_reader: Some(std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let mut stderr = stderr;
                stderr
                    .read_to_end(&mut bytes)
                    .expect("drain standalone stderr");
                bytes
            })),
        }
    }

    fn has_exited(&mut self) -> bool {
        self.child
            .as_mut()
            .expect("child process")
            .try_wait()
            .expect("poll standalone proxy server")
            .is_some()
    }

    fn stop(mut self) -> Output {
        let mut child = self.child.take().expect("child process");
        let _ = child.kill();
        let status = child.wait().expect("wait for standalone proxy server");
        let stdout = self
            .stdout_reader
            .take()
            .expect("stdout reader")
            .join()
            .expect("join stdout reader");
        let stderr = self
            .stderr_reader
            .take()
            .expect("stderr reader")
            .join()
            .expect("join stderr reader");
        Output {
            status,
            stdout,
            stderr,
        }
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.stdout_reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

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

async fn spawn_blocking_models_backend() -> (String, Arc<Notify>, Arc<Notify>, JoinHandle<()>) {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let entered_for_handler = Arc::clone(&entered);
    let release_for_handler = Arc::clone(&release);
    let app = Router::new().route(
        "/backend-api/codex/models",
        get(move || {
            let entered_for_handler = Arc::clone(&entered_for_handler);
            let release_for_handler = Arc::clone(&release_for_handler);
            async move {
                entered_for_handler.notify_one();
                release_for_handler.notified().await;
                Json(serde_json::json!({
                    "models": [{ "slug": "mock-codex" }]
                }))
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind blocking mock backend listener");
    let addr = listener
        .local_addr()
        .expect("blocking mock backend local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("run blocking mock backend server");
    });

    (
        format!("http://{addr}/backend-api/codex/responses"),
        entered,
        release,
        handle,
    )
}

fn reserve_loopback_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    listener.local_addr().expect("reserved address").port()
}

async fn wait_for_standalone_server(
    child: &mut ChildProcess,
    http: &reqwest::Client,
    base_url: &str,
) {
    for _ in 0..100 {
        assert!(!child.has_exited(), "standalone proxy server exited early");
        if tokio::time::timeout(
            Duration::from_millis(250),
            http.get(format!("{base_url}/health")).send(),
        )
        .await
        .is_ok_and(|result| result.is_ok_and(|response| response.status() == StatusCode::OK))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("standalone proxy server did not become ready");
}

fn standalone_command(
    port: u16,
    host: &str,
    auth_path: &std::path::Path,
    data_home: &std::path::Path,
    mock_api_url: &str,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_proxy-server"));
    command
        .arg("--host")
        .arg(host)
        .env("PORT", port.to_string())
        .env("AUTH_PATH", auth_path)
        .env("XDG_DATA_HOME", data_home)
        .env("CHATGPT_API_URL", mock_api_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
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
        client_auth: ClientAuthRuntime::default(),
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

async fn spawn_proxy_server_with_client_auth(
    api_url: String,
    client_auth: ClientAuthRuntime,
) -> (String, TempDir, JoinHandle<()>) {
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
        client_auth,
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
async fn protected_openai_route_rejects_missing_bearer_before_upstream() {
    let (mock_api_url, _responses_request_count, models_request_count, mock_handle) =
        spawn_mock_backend().await;
    let key =
        ClientApiKey::parse(&format!("sk-{}", "A".repeat(64))).expect("valid test client API key");
    let snapshot = ClientAuthSnapshot::enabled([key]).expect("non-empty auth policy");
    let runtime = ClientAuthRuntime::new(snapshot);
    let (base_url, _temp_dir, proxy_handle) =
        spawn_proxy_server_with_client_auth(mock_api_url, runtime).await;

    let response = reqwest::Client::new()
        .get(format!("{}/v1/models", base_url))
        .send()
        .await
        .expect("GET protected /v1/models without authorization");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer realm=\"OpenAI API\"")
    );
    let body: serde_json::Value = response.json().await.expect("OpenAI error JSON");
    assert_eq!(
        body,
        serde_json::json!({
            "error": {
                "message": "Missing bearer authentication in header",
                "type": "invalid_request_error",
                "param": null,
                "code": null
            }
        })
    );
    assert_eq!(models_request_count.load(Ordering::SeqCst), 0);

    proxy_handle.abort();
    mock_handle.abort();
}

#[tokio::test]
async fn client_auth_keeps_public_routes_preflight_and_unknown_v1_semantics() {
    let (mock_api_url, _responses_request_count, _models_request_count, mock_handle) =
        spawn_mock_backend().await;
    let key =
        ClientApiKey::parse(&format!("sk-{}", "A".repeat(64))).expect("valid test client API key");
    let snapshot = ClientAuthSnapshot::enabled([key]).expect("non-empty auth policy");
    let runtime = ClientAuthRuntime::new(snapshot);
    let (base_url, _temp_dir, proxy_handle) =
        spawn_proxy_server_with_client_auth(mock_api_url, runtime).await;
    let http = reqwest::Client::new();

    for path in ["/", "/health"] {
        let response = http
            .get(format!("{}{}", base_url, path))
            .header(header::AUTHORIZATION, "Basic ignored-on-public-routes")
            .send()
            .await
            .expect("GET public route");
        assert_eq!(response.status(), StatusCode::OK, "public route {path}");
    }

    let public_api_routes = [
        (reqwest::Method::POST, "/api/chat"),
        (reqwest::Method::POST, "/api/generate"),
        (reqwest::Method::GET, "/api/tags"),
        (reqwest::Method::POST, "/api/show"),
        (reqwest::Method::POST, "/api/embed"),
        (reqwest::Method::GET, "/api/ps"),
        (reqwest::Method::GET, "/api/version"),
        (reqwest::Method::GET, "/api/logs"),
        (reqwest::Method::GET, "/api/token-usage"),
        (reqwest::Method::POST, "/api/copy"),
        (reqwest::Method::DELETE, "/api/delete"),
        (reqwest::Method::POST, "/api/pull"),
        (reqwest::Method::POST, "/api/push"),
    ];
    for (method, path) in public_api_routes {
        let response = http
            .request(method, format!("{}{}", base_url, path))
            .header(header::AUTHORIZATION, "Basic ignored-on-public-routes")
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("request public /api route");
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "client auth must not protect {path}"
        );
    }

    let preflight = http
        .request(reqwest::Method::OPTIONS, format!("{}/v1/models", base_url))
        .header(header::ORIGIN, "http://localhost:1420")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
        .send()
        .await
        .expect("OpenAI route CORS preflight");
    assert_eq!(preflight.status(), StatusCode::OK);
    assert_eq!(
        preflight
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:1420")
    );

    let unknown = http
        .get(format!("{}/v1/not-a-registered-route", base_url))
        .send()
        .await
        .expect("GET unknown OpenAI path");
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

    proxy_handle.abort();
    mock_handle.abort();
}

#[tokio::test]
async fn protected_openai_routes_accept_members_and_return_fixed_invalid_key_errors() {
    let (mock_api_url, responses_request_count, models_request_count, mock_handle) =
        spawn_mock_backend().await;
    let first_key = ClientApiKey::parse(&format!("sk-{}", "A".repeat(64)))
        .expect("valid first test client API key");
    let second_key = ClientApiKey::parse(&format!("sk-{}", "B".repeat(64)))
        .expect("valid second test client API key");
    let second_key_header = format!("bEaReR {}", second_key.expose_secret());
    let snapshot = ClientAuthSnapshot::enabled([first_key, second_key.clone()])
        .expect("non-empty auth policy");
    let runtime = ClientAuthRuntime::new(snapshot);
    let (base_url, _temp_dir, proxy_handle) =
        spawn_proxy_server_with_client_auth(mock_api_url, runtime).await;
    let http = reqwest::Client::new();

    let valid = http
        .get(format!("{}/v1/models", base_url))
        .header(header::AUTHORIZATION, second_key_header)
        .send()
        .await
        .expect("GET protected /v1/models with valid key");
    assert_eq!(valid.status(), StatusCode::OK);
    assert_eq!(models_request_count.load(Ordering::SeqCst), 1);

    let valid_with_multiple_spaces = http
        .get(format!("{}/v1/models", base_url))
        .header(
            header::AUTHORIZATION,
            format!("Bearer    {}", second_key.expose_secret()),
        )
        .send()
        .await
        .expect("GET protected /v1/models with multiple separator spaces");
    assert_eq!(valid_with_multiple_spaces.status(), StatusCode::OK);
    assert_eq!(models_request_count.load(Ordering::SeqCst), 2);

    let valid_stream = http
        .post(format!("{}/v1/responses", base_url))
        .bearer_auth(second_key.expose_secret())
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "input": [{ "role": "user", "content": "hello" }],
            "stream": true
        }))
        .send()
        .await
        .expect("POST protected streaming /v1/responses with valid key");
    assert_eq!(valid_stream.status(), StatusCode::OK);
    assert!(valid_stream
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream")));
    assert!(valid_stream
        .text()
        .await
        .expect("valid streaming response body")
        .contains("response.output_text.delta"));

    let valid_chat = http
        .post(format!("{}/v1/chat/completions", base_url))
        .bearer_auth(second_key.expose_secret())
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": false
        }))
        .send()
        .await
        .expect("POST protected /v1/chat/completions with valid key");
    assert_eq!(valid_chat.status(), StatusCode::OK);

    let wrong_key = ClientApiKey::parse(&format!("sk-{}", "Z".repeat(64)))
        .expect("validly formatted unknown key");
    let invalid_headers = [
        "Basic credentials".to_string(),
        "Bearer".to_string(),
        "Bearer ".to_string(),
        "Bearer  embedded-space".to_string(),
        format!("Bearer {} trailing", second_key.expose_secret()),
        format!("Bearer {}", wrong_key.expose_secret()),
    ];
    for authorization in invalid_headers {
        let response = http
            .get(format!("{}/v1/models", base_url))
            .header(header::AUTHORIZATION, authorization)
            .send()
            .await
            .expect("GET protected /v1/models with invalid authorization");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer realm=\"OpenAI API\"")
        );
        let body = response.text().await.expect("invalid-key error body");
        assert!(!body.contains(second_key.expose_secret()));
        assert!(!body.contains(wrong_key.expose_secret()));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).expect("invalid-key error JSON"),
            serde_json::json!({
                "error": {
                    "message": "Incorrect API key provided.",
                    "type": "invalid_request_error",
                    "param": null,
                    "code": "invalid_api_key"
                }
            })
        );
    }

    let mut duplicate_headers = reqwest::header::HeaderMap::new();
    duplicate_headers.append(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", second_key.expose_secret()))
            .expect("first authorization header"),
    );
    duplicate_headers.append(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", second_key.expose_secret()))
            .expect("second authorization header"),
    );
    let duplicate = http
        .get(format!("{}/v1/models", base_url))
        .headers(duplicate_headers)
        .send()
        .await
        .expect("GET protected /v1/models with duplicate authorization");
    assert_eq!(duplicate.status(), StatusCode::UNAUTHORIZED);

    for path in ["/v1/responses", "/v1/chat/completions"] {
        let response = http
            .post(format!("{}{}", base_url, path))
            .header(header::CONTENT_TYPE, "application/json")
            .body("not valid JSON")
            .send()
            .await
            .expect("POST protected OpenAI route without authorization");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "route {path}");
        assert!(response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json")));
    }

    assert_eq!(models_request_count.load(Ordering::SeqCst), 2);
    assert_eq!(responses_request_count.load(Ordering::SeqCst), 2);

    proxy_handle.abort();
    mock_handle.abort();
}

#[tokio::test]
async fn client_auth_runtime_hot_swap_applies_to_subsequent_requests() {
    let (mock_api_url, _responses_request_count, models_request_count, mock_handle) =
        spawn_mock_backend().await;
    let runtime = ClientAuthRuntime::default();
    let (base_url, _temp_dir, proxy_handle) =
        spawn_proxy_server_with_client_auth(mock_api_url, runtime.clone()).await;
    let http = reqwest::Client::new();

    let initially_open = http
        .get(format!("{}/v1/models", base_url))
        .header(header::AUTHORIZATION, "Basic ignored-while-disabled")
        .send()
        .await
        .expect("GET /v1/models before enabling client auth");
    assert_eq!(initially_open.status(), StatusCode::OK);

    let key =
        ClientApiKey::parse(&format!("sk-{}", "C".repeat(64))).expect("valid test client API key");
    runtime
        .replace(ClientAuthSnapshot::enabled([key.clone()]).expect("non-empty auth snapshot"))
        .await;

    let protected = http
        .get(format!("{}/v1/models", base_url))
        .send()
        .await
        .expect("GET /v1/models after enabling client auth");
    assert_eq!(protected.status(), StatusCode::UNAUTHORIZED);

    let authorized = http
        .get(format!("{}/v1/models", base_url))
        .bearer_auth(key.expose_secret())
        .send()
        .await
        .expect("GET /v1/models with key after enabling client auth");
    assert_eq!(authorized.status(), StatusCode::OK);

    runtime.replace(ClientAuthSnapshot::disabled()).await;
    let reopened = http
        .get(format!("{}/v1/models", base_url))
        .header(header::AUTHORIZATION, "Bearer malformed-but-ignored")
        .send()
        .await
        .expect("GET /v1/models after disabling client auth");
    assert_eq!(reopened.status(), StatusCode::OK);
    assert_eq!(models_request_count.load(Ordering::SeqCst), 3);

    proxy_handle.abort();
    mock_handle.abort();
}

#[tokio::test]
async fn policy_change_does_not_cancel_a_request_that_already_passed_authentication() {
    let (mock_api_url, entered, release, mock_handle) = spawn_blocking_models_backend().await;
    let runtime = ClientAuthRuntime::default();
    let (base_url, _temp_dir, proxy_handle) =
        spawn_proxy_server_with_client_auth(mock_api_url, runtime.clone()).await;
    let http = reqwest::Client::new();
    let in_flight_http = http.clone();
    let in_flight_url = format!("{base_url}/v1/models");
    let in_flight = tokio::spawn(async move { in_flight_http.get(in_flight_url).send().await });

    tokio::time::timeout(Duration::from_secs(1), entered.notified())
        .await
        .expect("request reached upstream after passing authentication");
    let key =
        ClientApiKey::parse(&format!("sk-{}", "F".repeat(64))).expect("valid test client API key");
    runtime
        .replace(ClientAuthSnapshot::enabled([key]).expect("non-empty auth snapshot"))
        .await;

    let subsequent = http
        .get(format!("{base_url}/v1/models"))
        .send()
        .await
        .expect("request started after enabling authentication");
    assert_eq!(subsequent.status(), StatusCode::UNAUTHORIZED);

    release.notify_one();
    let completed = tokio::time::timeout(Duration::from_secs(1), in_flight)
        .await
        .expect("in-flight request completed")
        .expect("in-flight request task")
        .expect("in-flight HTTP response");
    assert_eq!(completed.status(), StatusCode::OK);

    proxy_handle.abort();
    mock_handle.abort();
}

#[tokio::test]
async fn standalone_api_key_is_runtime_only_and_protects_only_openai_routes() {
    let (mock_api_url, _responses_request_count, models_request_count, mock_handle) =
        spawn_mock_backend().await;
    let temp_dir = TempDir::new().expect("create standalone test directory");
    let auth_path = temp_dir.path().join("auth.json");
    std::fs::write(&auth_path, r#"{"OPENAI_API_KEY":"upstream-test-key"}"#)
        .expect("write standalone upstream auth file");
    let data_home = temp_dir.path().join("data");
    let port = reserve_loopback_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let key = ClientApiKey::parse(&format!("sk-{}", "D".repeat(64)))
        .expect("valid standalone client API key");
    let wrong_key = ClientApiKey::parse(&format!("sk-{}", "E".repeat(64)))
        .expect("validly formatted unknown standalone key");
    let http = reqwest::Client::new();

    let mut protected_command =
        standalone_command(port, "127.0.0.1", &auth_path, &data_home, &mock_api_url);
    protected_command.arg("--api-key").arg(key.expose_secret());
    let mut protected_process = ChildProcess::spawn(&mut protected_command);
    wait_for_standalone_server(&mut protected_process, &http, &base_url).await;

    for path in ["/health", "/api/tags"] {
        let response = http
            .get(format!("{base_url}{path}"))
            .send()
            .await
            .expect("GET public standalone route");
        assert_eq!(response.status(), StatusCode::OK, "public route {path}");
    }

    let missing = http
        .get(format!("{base_url}/v1/models"))
        .send()
        .await
        .expect("GET protected standalone route without key");
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let wrong = http
        .get(format!("{base_url}/v1/models"))
        .bearer_auth(wrong_key.expose_secret())
        .send()
        .await
        .expect("GET protected standalone route with wrong key");
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let valid = http
        .get(format!("{base_url}/v1/models"))
        .bearer_auth(key.expose_secret())
        .send()
        .await
        .expect("GET protected standalone route with valid key");
    assert_eq!(valid.status(), StatusCode::OK);
    assert_eq!(models_request_count.load(Ordering::SeqCst), 1);

    let protected_output = protected_process.stop();
    assert!(!String::from_utf8_lossy(&protected_output.stdout).contains(key.expose_secret()));
    assert!(!String::from_utf8_lossy(&protected_output.stderr).contains(key.expose_secret()));

    let database_dir = data_home.join("oorouter");
    let database_path = database_dir.join("proxy.db");
    for entry in std::fs::read_dir(&database_dir).expect("read standalone data directory") {
        let entry = entry.expect("read standalone data entry");
        if entry.file_type().expect("read data entry type").is_file() {
            let bytes = std::fs::read(entry.path()).expect("read standalone data file");
            assert!(
                !bytes
                    .windows(key.expose_secret().len())
                    .any(|window| window == key.expose_secret().as_bytes()),
                "standalone CLI key leaked into {}",
                entry.path().display()
            );
        }
    }

    let desktop_database = Database::new(&database_path)
        .await
        .expect("open shared desktop database");
    desktop_database
        .create_client_api_key(Some("desktop-only"))
        .await
        .expect("create desktop-only client key");
    desktop_database
        .set_client_auth_enabled(true)
        .await
        .expect("enable desktop-only client auth state");
    drop(desktop_database);

    let mut open_process = ChildProcess::spawn(&mut standalone_command(
        port,
        "0.0.0.0",
        &auth_path,
        &data_home,
        &mock_api_url,
    ));
    wait_for_standalone_server(&mut open_process, &http, &base_url).await;
    let reopened = http
        .get(format!("{base_url}/v1/models"))
        .send()
        .await
        .expect("GET standalone route after restart without key");
    assert_eq!(reopened.status(), StatusCode::OK);
    assert_eq!(models_request_count.load(Ordering::SeqCst), 2);

    let open_output = open_process.stop();
    let open_stdout = String::from_utf8_lossy(&open_output.stdout);
    let open_stderr = String::from_utf8_lossy(&open_output.stderr);
    assert!(
        open_stdout.contains(
            "standalone server is exposed beyond loopback without client API authentication"
        ) || open_stderr.contains(
            "standalone server is exposed beyond loopback without client API authentication"
        )
    );
    assert!(!open_stdout.contains(key.expose_secret()));
    assert!(!open_stderr.contains(key.expose_secret()));
    mock_handle.abort();
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

    let chat = http
        .post(format!("{}/api/chat", base_url))
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": false
        }))
        .send()
        .await
        .expect("POST /api/chat non-streaming");
    assert_eq!(chat.status(), StatusCode::OK);
    let chat_json: serde_json::Value = chat.json().await.expect("Ollama chat JSON");
    assert_eq!(chat_json["message"]["content"], "mock");
    assert_eq!(chat_json["done"], true);

    let generate = http
        .post(format!("{}/api/generate", base_url))
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "prompt": "hello",
            "stream": false
        }))
        .send()
        .await
        .expect("POST /api/generate non-streaming");
    assert_eq!(generate.status(), StatusCode::OK);
    let generate_json: serde_json::Value = generate.json().await.expect("Ollama generate JSON");
    assert_eq!(generate_json["response"], "mock");
    assert_eq!(generate_json["done"], true);

    let chat_stream = http
        .post(format!("{}/api/chat", base_url))
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": true
        }))
        .send()
        .await
        .expect("POST /api/chat streaming");
    assert_eq!(chat_stream.status(), StatusCode::OK);
    assert_eq!(
        chat_stream
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/x-ndjson")
    );
    let chat_chunks = chat_stream
        .text()
        .await
        .expect("Ollama chat NDJSON")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("chat chunk JSON"))
        .collect::<Vec<_>>();
    assert!(chat_chunks
        .iter()
        .any(|chunk| chunk["message"]["content"] == "mock"));
    assert_eq!(chat_chunks.last().expect("final chat chunk")["done"], true);

    let generate_stream = http
        .post(format!("{}/api/generate", base_url))
        .json(&serde_json::json!({
            "model": "gpt-5.3-codex",
            "prompt": "hello",
            "stream": true
        }))
        .send()
        .await
        .expect("POST /api/generate streaming");
    assert_eq!(generate_stream.status(), StatusCode::OK);
    assert_eq!(
        generate_stream
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/x-ndjson")
    );
    let generate_chunks = generate_stream
        .text()
        .await
        .expect("Ollama generate NDJSON")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("generate chunk JSON"))
        .collect::<Vec<_>>();
    assert!(generate_chunks
        .iter()
        .any(|chunk| chunk["response"] == "mock"));
    assert_eq!(
        generate_chunks.last().expect("final generate chunk")["done"],
        true
    );

    let models_v1 = http
        .get(format!("{}/v1/models", base_url))
        .header(
            header::AUTHORIZATION,
            "Basic malformed-but-ignored-while-client-auth-is-disabled",
        )
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
    assert_eq!(responses_request_count.load(Ordering::SeqCst), 7);

    proxy_handle.abort();
    mock_handle.abort();
}
