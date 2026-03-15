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
    converter::openai_chat_request_to_codex,
    logger::{LogEntry, push_log},
    models::get_visible_models,
    openai_streaming::{OpenAIStreamContext, collect_openai_response, create_openai_stream},
    types::openai::{OpenAIChatRequest, OpenAIModelObject, OpenAIModelsResponse},
};

pub async fn post_chat_completions(
    State(state): State<AppState>,
    Json(body): Json<OpenAIChatRequest>,
) -> super::RouteResult {
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
            .map_err(map_proxy_error)?;
        let completion = collect_openai_response(upstream_response, &ctx)
            .await
            .map_err(map_proxy_error)?;
        let input_tokens = completion["usage"]["prompt_tokens"].as_u64().map(|v| v as u32);
        let output_tokens = completion["usage"]["completion_tokens"]
            .as_u64()
            .map(|v| v as u32);

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
        let prompt_tokens = completion["usage"]["prompt_tokens"].as_i64().unwrap_or(0);
        let completion_tokens = completion["usage"]["completion_tokens"].as_i64().unwrap_or(0);
        if prompt_tokens > 0 || completion_tokens > 0 {
            let db = state.db.clone();
            let model_name = codex_req.model.clone();
            tokio::spawn(async move {
                if let Err(e) = db.insert_token_usage(&model_name, "codex", prompt_tokens, completion_tokens, "/v1/chat/completions").await {
                    eprintln!("[usage] insert failed: {e}");
                }
            });
        }
        return Ok(Json(completion).into_response());
    }

    let upstream_response = state
        .client
        .send_request(&codex_req)
        .await
        .map_err(map_proxy_error)?;
    let stream = create_openai_stream(ctx, upstream_response);

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("text/event-stream"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert("X-Accel-Buffering", HeaderValue::from_static("no"));

    Ok((headers, Body::from_stream(stream)).into_response())
}

pub async fn get_models() -> Json<OpenAIModelsResponse> {
    let now = Utc::now().timestamp();
    let created = if now.is_negative() { 0 } else { now as u64 };

    let data = get_visible_models()
        .into_iter()
        .map(|model| OpenAIModelObject {
            id: model.model.trim_end_matches(":latest").to_string(),
            object: "model".to_string(),
            created,
            owned_by: "oorouter".to_string(),
        })
        .collect();

    Json(OpenAIModelsResponse {
        object: "list".to_string(),
        data,
    })
}
