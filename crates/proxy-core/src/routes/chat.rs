use std::time::Instant;

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue},
    response::IntoResponse,
};
use chrono::{SecondsFormat, Utc};
use uuid::Uuid;

use super::{AppState, map_proxy_error};
use crate::{
    converter::chat_request_to_codex,
    logger::{LogEntry, push_log},
    streaming::{StreamContext, collect_sse_response, create_chat_stream},
    types::ollama::{OllamaChatMessage, OllamaChatRequest, OllamaChatResponse},
};

pub async fn handle_chat(
    State(state): State<AppState>,
    Json(body): Json<OllamaChatRequest>,
) -> super::RouteResult {
    let model = body.model.clone();
    let codex_req = chat_request_to_codex(&body);

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
        push_log(
            &state.log_buffer,
            LogEntry {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                method: "POST".to_string(),
                path: "/api/chat".to_string(),
                model: Some(model.clone()),
                status: 200,
                duration_ms: total_ns / 1_000_000,
                input_tokens: collected.usage.as_ref().map(|u| u.input_tokens as u32),
                output_tokens: collected.usage.as_ref().map(|u| u.output_tokens as u32),
            },
        );
        if let Some(ref usage) = collected.usage {
            let db = state.db.clone();
            let model = model.clone();
            let input = usage.input_tokens as i64;
            let output = usage.output_tokens as i64;
            tokio::spawn(async move {
                if let Err(e) = db.insert_token_usage(&model, "codex", input, output, "/api/chat").await {
                    eprintln!("[usage] insert failed: {e}");
                }
            });
        }
        let response = OllamaChatResponse {
            model,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            message: OllamaChatMessage {
                role: "assistant".to_string(),
                content: collected.text,
                images: None,
            },
            done: true,
            done_reason: Some("stop".to_string()),
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
    let stream = create_chat_stream(
        StreamContext::new(
            model,
            state.log_buffer.clone(),
            state.db.clone(),
            "/api/chat".to_string(),
        ),
        upstream_response,
    );

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/x-ndjson"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));

    Ok((headers, Body::from_stream(stream)).into_response())
}
