use std::time::Instant;

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue},
    response::IntoResponse,
    Json,
};
use chrono::{SecondsFormat, Utc};
use uuid::Uuid;

use super::{map_proxy_error, AppState};
use crate::{
    converter::generate_request_to_codex,
    logger::{push_log, LogEntry},
    streaming::{collect_sse_response, create_generate_stream, StreamContext},
    types::ollama::{OllamaGenerateRequest, OllamaGenerateResponse},
    usage::{record_token_usage, usage_counts_for_log},
};

pub async fn handle_generate(
    State(state): State<AppState>,
    Json(body): Json<OllamaGenerateRequest>,
) -> super::RouteResult {
    let model = body.model.clone();
    let codex_req = generate_request_to_codex(&body);

    if body.stream == Some(false) {
        let start_time = Instant::now();
        let upstream_response = state
            .client
            .send_request(&codex_req)
            .await
            .map_err(map_proxy_error)?;
        let collected = collect_sse_response(upstream_response)
            .await
            .map_err(map_proxy_error)?;

        let total_ns = start_time.elapsed().as_nanos() as u64;
        let (input_tokens, output_tokens) = usage_counts_for_log(collected.usage.as_ref(), &model);
        push_log(
            &state.log_buffer,
            LogEntry {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                method: "POST".to_string(),
                path: "/api/generate".to_string(),
                model: Some(model.clone()),
                status: 200,
                duration_ms: total_ns / 1_000_000,
                input_tokens,
                output_tokens,
            },
        );
        if let Some(ref usage) = collected.usage {
            record_token_usage(&state.db, &model, "/api/generate", usage).await;
        }
        let response = OllamaGenerateResponse {
            model,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            response: collected.text,
            done: true,
            done_reason: Some(collected.done_reason),
            context: Some(vec![]),
            total_duration: Some(total_ns),
            load_duration: Some(0),
            prompt_eval_count: Some(0),
            prompt_eval_duration: Some(0),
            eval_count: Some(0),
            eval_duration: Some(total_ns),
        };

        return Ok(Json(response).into_response());
    }

    let upstream_response = state
        .client
        .send_request(&codex_req)
        .await
        .map_err(map_proxy_error)?;
    let stream = create_generate_stream(
        StreamContext::new(
            model,
            state.log_buffer.clone(),
            state.db.clone(),
            "/api/generate".to_string(),
        ),
        upstream_response,
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_static("application/x-ndjson"),
    );
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));

    Ok((headers, Body::from_stream(stream)).into_response())
}
