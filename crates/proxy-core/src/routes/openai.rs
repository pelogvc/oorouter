use std::time::{Duration, Instant};
use std::{collections::HashSet, sync::Arc};

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use chrono::{SecondsFormat, Utc};
use futures::StreamExt;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{AppState, RouteResult};
use crate::{
    client::redact_sensitive_text,
    converter::{openai_chat_request_to_codex, resolve_model},
    db::Database,
    error::ProxyError,
    logger::{push_log, LogEntry},
    openai_streaming::{collect_openai_response, create_openai_stream, OpenAIStreamContext},
    types::{
        codex::CodexSSEResponseUsage,
        openai::{OpenAIChatRequest, OpenAIModelObject, OpenAIModelsResponse, OpenAIStop},
    },
    usage::{record_token_usage, token_count_for_log},
};

const FALLBACK_CODEX_CLIENT_VERSION: &str = "0.144.1";
const MODELS_FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RESPONSES_SSE_LINE_BYTES: usize = 16 * 1024 * 1024;
const MAX_BACKEND_ERROR_BODY_BYTES: usize = 64 * 1024;
const REDACTED_BACKEND_RESPONSE: &str = "<redacted sensitive backend response>";

pub async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "replay_state": "stateless",
    }))
}

fn openai_error(
    status: StatusCode,
    message: impl Into<String>,
    error_type: &'static str,
) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({
            "error": {
                "message": message.into(),
                "type": error_type,
            },
        })),
    )
}

fn map_openai_proxy_error(error: ProxyError) -> (StatusCode, Json<Value>) {
    let (error_kind, status) = match &error {
        ProxyError::BackendApiError(_) => ("backend_api_error", StatusCode::BAD_GATEWAY),
        ProxyError::AuthError(_) => ("auth_error", StatusCode::UNAUTHORIZED),
        ProxyError::ConfigError(_) => ("config_error", StatusCode::BAD_REQUEST),
        ProxyError::JsonError(_) => ("json_error", StatusCode::INTERNAL_SERVER_ERROR),
        ProxyError::HttpError(_) => ("http_error", StatusCode::INTERNAL_SERVER_ERROR),
        ProxyError::IoError(_) => ("io_error", StatusCode::INTERNAL_SERVER_ERROR),
    };
    tracing::warn!(error_kind, "openai route error");
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
        Json(json!({
            "error": {
                "message": message,
                "type": error_kind,
                "param": null,
                "code": null,
            },
        })),
    )
}

fn uses_server_replay_state(body: &Map<String, Value>) -> bool {
    body.values().any(contains_server_replay_state)
}

fn contains_server_replay_state(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object
                .get("previous_response_id")
                .and_then(|value| value.as_str())
                .is_some()
                || (object.get("type").and_then(|value| value.as_str()) == Some("item_reference")
                    && object.get("id").and_then(|value| value.as_str()).is_some())
                || object.values().any(contains_server_replay_state)
        }
        Value::Array(values) => values.iter().any(contains_server_replay_state),
        _ => false,
    }
}

fn normalize_responses_body(mut body: Map<String, Value>) -> Value {
    let resolved_model = body.get("model").and_then(Value::as_str).map(resolve_model);
    if let Some(model) = resolved_model {
        body.insert("model".to_string(), Value::String(model));
    }

    if !body
        .get("instructions")
        .is_some_and(|value| value.is_string())
    {
        body.insert("instructions".to_string(), Value::String(String::new()));
    }

    body.entry("store".to_string())
        .or_insert(Value::Bool(false));
    body.insert("stream".to_string(), Value::Bool(true));
    // Match openai-oauth: the Codex backend rejects this OpenAI Responses field.
    body.remove("max_output_tokens");

    Value::Object(body)
}

fn responses_usage_from_value(response: &Value) -> Option<CodexSSEResponseUsage> {
    let usage = response.get("usage")?;
    let input_tokens = usage.get("input_tokens")?.as_u64()?;
    let output_tokens = usage.get("output_tokens")?.as_u64()?;

    Some(CodexSSEResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens: usage
            .get("total_tokens")
            .and_then(|value| value.as_u64())
            .unwrap_or_else(|| input_tokens.saturating_add(output_tokens)),
        input_tokens_details: usage.get("input_tokens_details").cloned(),
        output_tokens_details: usage.get("output_tokens_details").cloned(),
    })
}

fn stop_contains_non_empty_sequence(stop: Option<&OpenAIStop>) -> bool {
    match stop {
        Some(OpenAIStop::Single(value)) => !value.is_empty(),
        Some(OpenAIStop::Multiple(values)) => values.iter().any(|value| !value.is_empty()),
        None => false,
    }
}

fn is_supported_reasoning_effort(model: &str, value: Option<&str>) -> bool {
    let model = resolve_model(model);
    match value {
        None | Some("low" | "medium" | "high" | "xhigh") => true,
        Some("max") => matches!(
            model.as_str(),
            "gpt-5.6-sol" | "gpt-5.6-terra" | "gpt-5.6-luna"
        ),
        Some("ultra") => matches!(model.as_str(), "gpt-5.6-sol" | "gpt-5.6-terra"),
        _ => false,
    }
}

fn process_responses_usage_sse_line(
    line: &str,
    event_data: &mut String,
) -> std::result::Result<Option<CodexSSEResponseUsage>, ProxyError> {
    let trimmed = line.trim_end();
    if let Some(payload) = trimmed.strip_prefix("data:") {
        event_data.push_str(payload.trim_start());
        event_data.push('\n');
        if event_data.len() > MAX_RESPONSES_SSE_LINE_BYTES {
            return Err(ProxyError::BackendApiError(
                "Responses SSE frame exceeded maximum size".to_string(),
            ));
        }
        return Ok(None);
    }

    if !trimmed.is_empty() || event_data.is_empty() {
        return Ok(None);
    }

    let payload = event_data.trim_end_matches('\n').trim();
    if payload.is_empty() || payload == "[DONE]" {
        event_data.clear();
        return Ok(None);
    }

    let parsed: Value = serde_json::from_str(payload).map_err(|error| {
        ProxyError::BackendApiError(format!("Invalid JSON in Responses SSE stream: {}", error))
    })?;
    if matches!(
        parsed.get("type").and_then(|value| value.as_str()),
        Some("response.completed" | "response.done" | "response.incomplete")
    ) {
        if let Some(response) = parsed
            .get("response")
            .filter(|response| response.is_object())
        {
            let usage = responses_usage_from_value(response);
            event_data.clear();
            return Ok(usage);
        }
    }
    event_data.clear();
    Ok(None)
}

fn create_responses_usage_recording_response(
    db: Arc<Database>,
    model: Option<String>,
    response: reqwest::Response,
    default_content_type: &'static str,
) -> Response {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static(default_content_type));

    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    headers.insert("X-Accel-Buffering", HeaderValue::from_static("no"));

    let stream = async_stream::stream! {
        let mut upstream = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        let mut event_data = String::new();
        let mut usage_recorded = model.is_none();

        while let Some(chunk) = upstream.next().await {
            let chunk = chunk.map_err(ProxyError::HttpError)?;
            if !usage_recorded {
                for segment in chunk.split_inclusive(|byte| *byte == b'\n') {
                    if buffer.len().saturating_add(segment.len()) > MAX_RESPONSES_SSE_LINE_BYTES {
                        tracing::warn!("Responses SSE usage observer frame exceeded maximum size");
                        usage_recorded = true;
                        break;
                    }
                    buffer.extend_from_slice(segment);

                    while let Some(newline_idx) = buffer.iter().position(|byte| *byte == b'\n') {
                        let line = buffer.drain(..=newline_idx).collect::<Vec<_>>();
                        match std::str::from_utf8(&line) {
                            Ok(line) => {
                                match process_responses_usage_sse_line(
                                    line,
                                    &mut event_data,
                                ) {
                                    Ok(Some(usage)) => {
                                        if let Some(model) = model.as_deref() {
                                            record_token_usage(
                                                &db,
                                                model,
                                                "/v1/responses",
                                                &usage,
                                            ).await;
                                        }
                                        usage_recorded = true;
                                        break;
                                    }
                                    Ok(None) => {}
                                    Err(error) => {
                                        tracing::warn!(%error, "failed to observe Responses usage from stream");
                                        usage_recorded = true;
                                        break;
                                    }
                                }
                            }
                            Err(error) => {
                                tracing::warn!(%error, "invalid UTF-8 while observing Responses usage");
                                usage_recorded = true;
                                break;
                            }
                        }
                    }
                }
            }
            yield Ok::<Bytes, ProxyError>(chunk);
        }
    };

    (status, headers, Body::from_stream(stream)).into_response()
}

async fn backend_error_response_to_axum(
    response: reqwest::Response,
    default_content_type: &'static str,
) -> Response {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static(default_content_type));
    let body =
        read_limited_response_text(response, MAX_BACKEND_ERROR_BODY_BYTES)
            .await
            .unwrap_or_else(|error| {
                format!(
                    r#"{{"error":{{"message":"failed to read upstream error body: {}","type":"upstream_error"}}}}"#,
                    error
                )
            });
    let redacted_body = redact_sensitive_text(&body);
    let is_json_content = content_type
        .to_str()
        .map(|value| value.to_ascii_lowercase().contains("json"))
        .unwrap_or(false);
    let body = if is_json_content && redacted_body == REDACTED_BACKEND_RESPONSE {
        json!({
            "error": {
                "message": "Upstream backend returned a sensitive error body that was redacted.",
                "type": "upstream_error",
            }
        })
        .to_string()
    } else {
        redacted_body
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    (status, headers, Body::from(body)).into_response()
}

async fn read_limited_response_text(
    response: reqwest::Response,
    max_bytes: usize,
) -> std::result::Result<String, ProxyError> {
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(ProxyError::HttpError)?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Ok(REDACTED_BACKEND_RESPONSE.to_string());
        }
        body.extend_from_slice(&chunk);
    }

    Ok(String::from_utf8_lossy(&body).into_owned())
}

#[derive(Debug)]
enum ResponsesCollectFailure {
    Proxy(ProxyError),
    OpenAIError { status: StatusCode, body: Value },
}

impl From<ProxyError> for ResponsesCollectFailure {
    fn from(error: ProxyError) -> Self {
        ResponsesCollectFailure::Proxy(error)
    }
}

fn map_responses_collect_failure(error: ResponsesCollectFailure) -> (StatusCode, Json<Value>) {
    match error {
        ResponsesCollectFailure::Proxy(error) => map_openai_proxy_error(error),
        ResponsesCollectFailure::OpenAIError { status, body } => (status, Json(body)),
    }
}

fn failed_response_status(parsed: &Value) -> StatusCode {
    parsed
        .get("response")
        .and_then(|response| {
            [
                response.get("status"),
                response.get("status_code"),
                response.get("error").and_then(|error| error.get("status")),
                response
                    .get("error")
                    .and_then(|error| error.get("status_code")),
            ]
            .into_iter()
            .flatten()
            .find_map(|status| status.as_u64())
        })
        .and_then(|status| u16::try_from(status).ok())
        .and_then(|status| StatusCode::from_u16(status).ok())
        .unwrap_or(StatusCode::BAD_GATEWAY)
}

fn failed_response_body(parsed: &Value) -> Value {
    let error = parsed
        .get("response")
        .and_then(|response| response.get("error"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "message": "Responses request failed",
                "type": "server_error",
            })
        });

    json!({ "error": error })
}

fn parse_response_sse_payload(
    payload: &str,
    latest_response: &mut Option<Value>,
) -> std::result::Result<(), ResponsesCollectFailure> {
    if payload.is_empty() || payload == "[DONE]" {
        return Ok(());
    }

    let parsed: Value = serde_json::from_str(payload).map_err(|error| {
        ResponsesCollectFailure::Proxy(ProxyError::BackendApiError(format!(
            "Invalid JSON in Responses SSE stream: {}",
            error
        )))
    })?;

    if parsed.get("type").and_then(|value| value.as_str()) == Some("response.failed") {
        return Err(ResponsesCollectFailure::OpenAIError {
            status: failed_response_status(&parsed),
            body: failed_response_body(&parsed),
        });
    }

    if !matches!(
        parsed.get("type").and_then(|value| value.as_str()),
        Some("response.completed" | "response.done" | "response.incomplete")
    ) {
        return Ok(());
    }

    if let Some(response) = parsed
        .get("response")
        .filter(|response| response.is_object())
    {
        *latest_response = Some(response.clone());
    }

    Ok(())
}

async fn collect_responses_response(
    response: reqwest::Response,
) -> std::result::Result<Value, ResponsesCollectFailure> {
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut event_data = String::new();
    let mut latest_response = None;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(ProxyError::HttpError)?;
        for segment in chunk.split_inclusive(|byte| *byte == b'\n') {
            if buffer.len().saturating_add(segment.len()) > MAX_RESPONSES_SSE_LINE_BYTES {
                return Err(ResponsesCollectFailure::Proxy(ProxyError::BackendApiError(
                    "Responses SSE frame exceeded maximum size".to_string(),
                )));
            }
            buffer.extend_from_slice(segment);

            while let Some(newline_idx) = buffer.iter().position(|byte| *byte == b'\n') {
                let line = buffer.drain(..=newline_idx).collect::<Vec<_>>();
                if line.len() > MAX_RESPONSES_SSE_LINE_BYTES {
                    return Err(ResponsesCollectFailure::Proxy(ProxyError::BackendApiError(
                        "Responses SSE frame exceeded maximum size".to_string(),
                    )));
                }
                let line = std::str::from_utf8(&line).map_err(|e| {
                    ProxyError::BackendApiError(format!("Invalid UTF-8 in SSE stream: {}", e))
                })?;
                let trimmed = line.trim_end();
                if let Some(payload) = trimmed.strip_prefix("data:") {
                    event_data.push_str(payload.trim_start());
                    event_data.push('\n');
                    if event_data.len() > MAX_RESPONSES_SSE_LINE_BYTES {
                        return Err(ResponsesCollectFailure::Proxy(ProxyError::BackendApiError(
                            "Responses SSE frame exceeded maximum size".to_string(),
                        )));
                    }
                } else if trimmed.is_empty() && !event_data.is_empty() {
                    let payload = event_data.trim_end_matches('\n');
                    parse_response_sse_payload(payload.trim(), &mut latest_response)?;
                    event_data.clear();
                }
            }

            if buffer.len() > MAX_RESPONSES_SSE_LINE_BYTES {
                return Err(ResponsesCollectFailure::Proxy(ProxyError::BackendApiError(
                    "Responses SSE frame exceeded maximum size".to_string(),
                )));
            }
        }
    }

    if !buffer.is_empty() {
        if buffer.len() > MAX_RESPONSES_SSE_LINE_BYTES {
            return Err(ResponsesCollectFailure::Proxy(ProxyError::BackendApiError(
                "Responses SSE frame exceeded maximum size".to_string(),
            )));
        }
        let line = std::str::from_utf8(&buffer).map_err(|e| {
            ProxyError::BackendApiError(format!("Invalid UTF-8 in SSE stream: {}", e))
        })?;
        let trimmed = line.trim();
        if let Some(payload) = trimmed.strip_prefix("data:") {
            event_data.push_str(payload.trim_start());
            event_data.push('\n');
            if event_data.len() > MAX_RESPONSES_SSE_LINE_BYTES {
                return Err(ResponsesCollectFailure::Proxy(ProxyError::BackendApiError(
                    "Responses SSE frame exceeded maximum size".to_string(),
                )));
            }
        } else if trimmed.is_empty() && !event_data.is_empty() {
            let payload = event_data.trim_end_matches('\n');
            parse_response_sse_payload(payload.trim(), &mut latest_response)?;
            event_data.clear();
        }
    }

    if !event_data.is_empty() {
        let payload = event_data.trim_end_matches('\n');
        parse_response_sse_payload(payload.trim(), &mut latest_response)?;
    }

    latest_response.ok_or_else(|| {
        ResponsesCollectFailure::Proxy(ProxyError::BackendApiError(
            "No completed response found in SSE stream".to_string(),
        ))
    })
}

pub async fn post_responses(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> super::RouteResult {
    let Some(body) = body.as_object().cloned() else {
        return Err(openai_error(
            StatusCode::BAD_REQUEST,
            "Request body must be a JSON object.",
            "invalid_request_error",
        ));
    };

    if uses_server_replay_state(&body) {
        return Err(openai_error(
            StatusCode::BAD_REQUEST,
            "Stateless Codex responses endpoint does not support `previous_response_id` or `item_reference`. Replay the full conversation history in `input` on each request.",
            "invalid_request_error",
        ));
    }

    let wants_stream = body
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let normalized_body = normalize_responses_body(body);
    let usage_model = normalized_body
        .get("model")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let upstream_response = state
        .client
        .send_raw_responses_request(&normalized_body)
        .await
        .map_err(map_openai_proxy_error)?;

    if !upstream_response.status().is_success() {
        return Ok(backend_error_response_to_axum(
            upstream_response,
            "application/json; charset=utf-8",
        )
        .await);
    }

    if wants_stream {
        return Ok(create_responses_usage_recording_response(
            state.db.clone(),
            usage_model,
            upstream_response,
            "text/event-stream; charset=utf-8",
        ));
    }

    let completed = collect_responses_response(upstream_response)
        .await
        .map_err(map_responses_collect_failure)?;
    if let (Some(model), Some(usage)) = (usage_model, responses_usage_from_value(&completed)) {
        record_token_usage(&state.db, &model, "/v1/responses", &usage).await;
    }
    Ok(Json(completed).into_response())
}

pub async fn post_chat_completions(
    State(state): State<AppState>,
    Json(body): Json<OpenAIChatRequest>,
) -> super::RouteResult {
    if stop_contains_non_empty_sequence(body.stop.as_ref()) {
        return Err(openai_error(
            StatusCode::BAD_REQUEST,
            "`stop` is not supported by the Codex backend.",
            "invalid_request_error",
        ));
    }

    if !is_supported_reasoning_effort(&body.model, body.reasoning_effort.as_deref()) {
        return Err(openai_error(
            StatusCode::BAD_REQUEST,
            "Unsupported `reasoning_effort` for the selected model.",
            "invalid_request_error",
        ));
    }

    let codex_req = openai_chat_request_to_codex(&body);
    let include_usage = body
        .stream_options
        .as_ref()
        .and_then(|options| options.include_usage)
        .unwrap_or(false);
    let ctx = OpenAIStreamContext::new(
        codex_req.model.clone(),
        include_usage,
        state.log_buffer.clone(),
        state.db.clone(),
        "/v1/chat/completions".to_string(),
    );

    if body.stream != Some(true) {
        let start_time = Instant::now();
        let upstream_response = state
            .client
            .send_request(&codex_req)
            .await
            .map_err(map_openai_proxy_error)?;
        let completion = collect_openai_response(upstream_response, &ctx)
            .await
            .map_err(map_openai_proxy_error)?;
        let input_tokens = completion["usage"]["prompt_tokens"]
            .as_u64()
            .and_then(|value| token_count_for_log(value, "prompt_tokens", &codex_req.model));
        let output_tokens = completion["usage"]["completion_tokens"]
            .as_u64()
            .and_then(|value| token_count_for_log(value, "completion_tokens", &codex_req.model));

        push_log(
            &state.log_buffer,
            LogEntry {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model: Some(codex_req.model.clone()),
                status: 200,
                duration_ms: start_time.elapsed().as_millis() as u64,
                input_tokens,
                output_tokens,
            },
        );
        let prompt_tokens = completion["usage"]["prompt_tokens"].as_u64();
        let completion_tokens = completion["usage"]["completion_tokens"].as_u64();
        let prompt_tokens = prompt_tokens.unwrap_or(0);
        let completion_tokens = completion_tokens.unwrap_or(0);
        if prompt_tokens > 0 || completion_tokens > 0 {
            let usage = CodexSSEResponseUsage {
                input_tokens: prompt_tokens,
                output_tokens: completion_tokens,
                total_tokens: completion["usage"]["total_tokens"]
                    .as_u64()
                    .unwrap_or_else(|| prompt_tokens.saturating_add(completion_tokens)),
                input_tokens_details: completion["usage"].get("prompt_tokens_details").cloned(),
                output_tokens_details: completion["usage"]
                    .get("completion_tokens_details")
                    .cloned(),
            };
            record_token_usage(&state.db, &codex_req.model, "/v1/chat/completions", &usage).await;
        }
        return Ok(Json(completion).into_response());
    }

    let upstream_response = state
        .client
        .send_request(&codex_req)
        .await
        .map_err(map_openai_proxy_error)?;
    let stream = create_openai_stream(ctx, upstream_response);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert("X-Accel-Buffering", HeaderValue::from_static("no"));

    Ok((headers, Body::from_stream(stream)).into_response())
}

fn codex_client_version() -> String {
    std::env::var("CODEX_VERSION")
        .ok()
        .filter(|version| !version.trim().is_empty())
        .unwrap_or_else(|| FALLBACK_CODEX_CLIENT_VERSION.to_string())
}

fn model_response_from_slugs(slugs: Vec<String>) -> OpenAIModelsResponse {
    let data = merge_visible_model_slugs(slugs)
        .into_iter()
        .map(|model| OpenAIModelObject {
            id: model.trim_end_matches(":latest").to_string(),
            object: "model".to_string(),
            created: 0,
            owned_by: "codex-oauth".to_string(),
        })
        .collect();

    OpenAIModelsResponse {
        object: "list".to_string(),
        data,
    }
}

fn merge_visible_model_slugs(upstream_slugs: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut slugs = Vec::new();

    for model in crate::models::get_visible_models() {
        let slug = model.name.trim_end_matches(":latest").to_string();
        if seen.insert(slug.clone()) {
            slugs.push(slug);
        }
    }

    for slug in upstream_slugs {
        let normalized = slug.trim_end_matches(":latest").to_string();
        if seen.insert(normalized.clone()) {
            slugs.push(normalized);
        }
    }

    slugs
}

pub async fn get_models(State(state): State<AppState>) -> RouteResult {
    let client_version = codex_client_version();
    let slugs = match tokio::time::timeout(
        MODELS_FETCH_TIMEOUT,
        state.client.fetch_model_slugs(&client_version),
    )
    .await
    {
        Ok(Ok(slugs)) => slugs,
        Ok(Err(error)) => {
            return Err(openai_error(
                StatusCode::BAD_GATEWAY,
                error.to_string(),
                "upstream_error",
            ));
        }
        Err(_) => {
            return Err(openai_error(
                StatusCode::BAD_GATEWAY,
                "Timed out loading models from upstream.",
                "upstream_error",
            ));
        }
    };

    Ok(Json(model_response_from_slugs(slugs)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_supported_reasoning_effort_for_gpt56_models() {
        assert!(is_supported_reasoning_effort("gpt-5.6", Some("ultra")));
        assert!(is_supported_reasoning_effort(
            "gpt-5.6-terra",
            Some("ultra")
        ));
        assert!(is_supported_reasoning_effort("gpt-5.6-luna", Some("max")));
        assert!(!is_supported_reasoning_effort(
            "gpt-5.6-luna",
            Some("ultra")
        ));
        assert!(!is_supported_reasoning_effort("gpt-5.5", Some("max")));
    }

    #[test]
    fn test_normalize_responses_body_resolves_model_alias() {
        let body = serde_json::json!({ "model": "gpt-5.6", "input": "hello" })
            .as_object()
            .unwrap()
            .clone();
        let normalized = normalize_responses_body(body);
        assert_eq!(normalized["model"], "gpt-5.6-sol");
    }

    async fn mock_sse_response(body: &str) -> reqwest::Response {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let body = body.to_string();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut req_buf = [0_u8; 1024];
            let _ = socket.read(&mut req_buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        reqwest::get(format!("http://{}", addr)).await.unwrap()
    }

    async fn mock_backend_response(
        status: StatusCode,
        content_type: &'static str,
        body: &'static str,
    ) -> reqwest::Response {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut req_buf = [0_u8; 1024];
            let _ = socket.read(&mut req_buf).await;
            let response = format!(
                "HTTP/1.1 {} Test\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                status.as_u16(),
                content_type,
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        reqwest::get(format!("http://{}", addr)).await.unwrap()
    }

    async fn spawn_models_error_backend() -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut req_buf = [0_u8; 1024];
            let _ = socket.read(&mut req_buf).await;
            let body = r#"{"error":{"message":"models unavailable"}}"#;
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        (
            format!("http://{}/backend-api/codex/responses", addr),
            handle,
        )
    }

    #[tokio::test]
    async fn test_collect_responses_response_multiline_data_event() {
        let sse_body = concat!(
            "data: {\"type\":\"response.completed\",\n",
            "data: \"response\":{\"id\":\"resp-1\",\"object\":\"response\"}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let collected = collect_responses_response(response).await.unwrap();

        assert_eq!(collected["id"], "resp-1");
        assert_eq!(collected["object"], "response");
    }

    #[tokio::test]
    async fn test_collect_responses_response_incomplete_event() {
        let sse_body = concat!(
            "data: {\"type\":\"response.incomplete\",\"response\":{\"id\":\"resp-1\",\"object\":\"response\",\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let collected = collect_responses_response(response).await.unwrap();

        assert_eq!(collected["id"], "resp-1");
        assert_eq!(collected["status"], "incomplete");
        assert_eq!(
            collected["incomplete_details"]["reason"],
            "max_output_tokens"
        );
    }

    #[tokio::test]
    async fn test_collect_responses_response_failed_preserves_openai_error() {
        let sse_body = concat!(
            "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"message\":\"bad request\",\"type\":\"invalid_request_error\",\"code\":\"bad_request\",\"status_code\":400}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let result = collect_responses_response(response).await;

        match result {
            Err(ResponsesCollectFailure::OpenAIError { status, body }) => {
                assert_eq!(status, StatusCode::BAD_REQUEST);
                assert_eq!(body["error"]["message"], "bad request");
                assert_eq!(body["error"]["type"], "invalid_request_error");
                assert_eq!(body["error"]["code"], "bad_request");
            }
            _ => panic!("expected OpenAI error"),
        }
    }

    #[tokio::test]
    async fn test_backend_error_response_redacts_json_as_json_error() {
        let upstream = mock_backend_response(
            StatusCode::UNAUTHORIZED,
            "application/json",
            r#"{"access_token":"secret-token"}"#,
        )
        .await;

        let response = backend_error_response_to_axum(upstream, "application/json").await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(parsed["error"]["type"], "upstream_error");
        assert!(!String::from_utf8_lossy(&body).contains("secret-token"));
    }

    #[tokio::test]
    async fn test_get_models_returns_upstream_error_without_static_fallback() {
        let (api_url, backend_handle) = spawn_models_error_backend().await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let client = std::sync::Arc::new(crate::client::CodexClient::new(
            crate::auth::AuthInfo {
                mode: crate::auth::AuthMode::ApiKey,
                access_token: "test-key".to_string(),
                account_id: None,
            },
            api_url,
        ));
        let state = AppState {
            client,
            db,
            log_buffer: crate::logger::new_log_buffer(),
        };

        let (status, body) = get_models(State(state)).await.unwrap_err();

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body.0["error"]["type"], "upstream_error");
        backend_handle.abort();
    }
}
