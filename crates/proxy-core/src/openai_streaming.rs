use bytes::Bytes;
use futures::Stream;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{ProxyError, Result};
use crate::types::codex::CodexSSEEvent;
use crate::types::openai::{
    OpenAIChoice, OpenAIChunk, OpenAIDelta, OpenAIToolCallDelta, OpenAIToolCallDeltaFunction,
    OpenAIUsage,
};
use crate::usage::{record_token_usage, usage_counts_for_log};

const MAX_SSE_BUFFER_BYTES: usize = 16 * 1024 * 1024;
const MAX_SSE_LINES_PER_CHUNK: usize = 65_536;

pub struct OpenAIStreamContext {
    pub completion_id: String,
    pub created: u64,
    pub model: String,
    pub system_fingerprint: String,
    pub include_usage: bool,
    pub log_buffer: crate::logger::LogBuffer,
    pub db: std::sync::Arc<crate::db::Database>,
    pub path: String,
}

impl OpenAIStreamContext {
    pub fn new(
        model: String,
        include_usage: bool,
        log_buffer: crate::logger::LogBuffer,
        db: std::sync::Arc<crate::db::Database>,
        path: String,
    ) -> Self {
        let id = Uuid::new_v4().to_string().replace('-', "");
        OpenAIStreamContext {
            completion_id: format!("chatcmpl-{}", &id[..24]),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            model,
            system_fingerprint: format!("fp_{}", &id[..12]),
            include_usage,
            log_buffer,
            db,
            path,
        }
    }
}

struct ToolCallState {
    index: u32,
    arguments: String,
}

fn parse_sse_line(line: &str) -> Option<CodexSSEEvent> {
    if !line.starts_with("data:") {
        return None;
    }

    let payload = line[5..].trim();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }

    serde_json::from_str::<CodexSSEEvent>(payload).ok()
}

fn sse_frame_too_large_error() -> ProxyError {
    ProxyError::BackendApiError("SSE stream buffer exceeded maximum size".to_string())
}

fn invalid_utf8_error(error: std::str::Utf8Error) -> ProxyError {
    ProxyError::BackendApiError(format!("Invalid UTF-8 in SSE stream: {}", error))
}

fn has_too_many_sse_lines(buffer: &[u8]) -> bool {
    buffer
        .iter()
        .filter(|byte| **byte == b'\n')
        .take(MAX_SSE_LINES_PER_CHUNK + 1)
        .count()
        > MAX_SSE_LINES_PER_CHUNK
}

fn append_sse_chunk(buffer: &mut Vec<u8>, chunk: &Bytes) -> Result<()> {
    if buffer.len().saturating_add(chunk.len()) > MAX_SSE_BUFFER_BYTES {
        return Err(sse_frame_too_large_error());
    }
    buffer.extend_from_slice(chunk);
    if has_too_many_sse_lines(buffer) {
        return Err(sse_frame_too_large_error());
    }
    Ok(())
}

fn drain_next_sse_line(buffer: &mut Vec<u8>) -> Result<Option<String>> {
    let Some(newline_idx) = buffer.iter().position(|byte| *byte == b'\n') else {
        return Ok(None);
    };

    let line = buffer.drain(..=newline_idx).collect::<Vec<_>>();
    if line.len() > MAX_SSE_BUFFER_BYTES {
        return Err(sse_frame_too_large_error());
    }
    std::str::from_utf8(&line)
        .map(|line| Some(line.to_string()))
        .map_err(invalid_utf8_error)
}

fn decode_remaining_sse_line(buffer: &[u8]) -> Result<String> {
    if buffer.len() > MAX_SSE_BUFFER_BYTES {
        return Err(sse_frame_too_large_error());
    }
    std::str::from_utf8(buffer)
        .map(ToString::to_string)
        .map_err(invalid_utf8_error)
}

fn build_chunk(
    ctx: &OpenAIStreamContext,
    choices: Vec<OpenAIChoice>,
    usage: Option<Option<OpenAIUsage>>,
) -> OpenAIChunk {
    let usage = usage.map(|usage| match usage {
        Some(usage) => serde_json::to_value(usage).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    });

    OpenAIChunk {
        id: ctx.completion_id.clone(),
        object: "chat.completion.chunk".to_string(),
        created: ctx.created,
        model: ctx.model.clone(),
        system_fingerprint: ctx.system_fingerprint.clone(),
        choices,
        usage,
    }
}

fn format_sse(chunk: &OpenAIChunk) -> String {
    format!(
        "data: {}\n\n",
        serde_json::to_string(chunk).unwrap_or_default()
    )
}

fn map_usage(codex_usage: Option<&crate::types::codex::CodexSSEResponseUsage>) -> OpenAIUsage {
    match codex_usage {
        None => OpenAIUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        },
        Some(u) => OpenAIUsage {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            total_tokens: u.total_tokens,
            prompt_tokens_details: u.input_tokens_details.clone(),
            completion_tokens_details: u.output_tokens_details.clone(),
        },
    }
}

fn incomplete_reason(response: &crate::types::codex::CodexSSEResponseData) -> Option<&str> {
    response
        .incomplete_details
        .as_ref()
        .and_then(|details| details.get("reason"))
        .and_then(|reason| reason.as_str())
}

fn map_finish_reason_from_response(
    response: Option<&crate::types::codex::CodexSSEResponseData>,
    has_tool_calls: bool,
) -> Option<String> {
    if has_tool_calls {
        return Some("tool_calls".to_string());
    }

    match response {
        Some(response)
            if response.status.as_deref() == Some("incomplete")
                || response.incomplete_details.is_some() =>
        {
            match incomplete_reason(response) {
                Some("content_filter") | Some("content-filter") => {
                    Some("content_filter".to_string())
                }
                Some("max_output_tokens") | Some("length") => Some("length".to_string()),
                _ => Some("length".to_string()),
            }
        }
        Some(_) => Some("stop".to_string()),
        None => None,
    }
}

pub fn create_openai_stream(
    ctx: OpenAIStreamContext,
    response: reqwest::Response,
) -> impl Stream<Item = std::result::Result<Bytes, ProxyError>> {
    let mut buffer = Vec::new();
    let mut is_first_chunk = true;
    let mut tool_calls_by_output_index: HashMap<u32, ToolCallState> = HashMap::new();
    let mut next_tool_call_index: u32 = 0;
    let mut done_sent = false;

    async_stream::stream! {
        let mut stream = response.bytes_stream();
        use futures::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(ProxyError::HttpError)?;
            if let Err(error) = append_sse_chunk(&mut buffer, &chunk) {
                yield Err(error);
                return;
            }

            loop {
                let line = match drain_next_sse_line(&mut buffer) {
                    Ok(Some(line)) => line,
                    Ok(None) => break,
                    Err(error) => {
                        yield Err(error);
                        return;
                    }
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let event = match parse_sse_line(trimmed) {
                    Some(e) => e,
                    None => continue,
                };

                match event.event_type() {
                    "response.output_text.delta" => {
                        if let CodexSSEEvent::Delta(e) = &event {
                            if is_first_chunk {
                                let role_chunk = build_chunk(
                                    &ctx,
                                    vec![OpenAIChoice {
                                        index: 0,
                                        delta: OpenAIDelta {
                                            role: Some("assistant".to_string()),
                                            content: None,
                                            tool_calls: None,
                                        },
                                        finish_reason: None,
                                    }],
                                    if ctx.include_usage { Some(None) } else { None },
                                );
                                yield Ok(Bytes::from(format_sse(&role_chunk)));
                                is_first_chunk = false;
                            }

                            let content_chunk = build_chunk(
                                &ctx,
                                vec![OpenAIChoice {
                                    index: 0,
                                    delta: OpenAIDelta {
                                        role: None,
                                        content: Some(e.delta.clone()),
                                        tool_calls: None,
                                    },
                                    finish_reason: None,
                                }],
                                if ctx.include_usage { Some(None) } else { None },
                            );
                            yield Ok(Bytes::from(format_sse(&content_chunk)));
                        }
                    }
                    "response.output_item.added" => {
                        if let CodexSSEEvent::OutputItemAdded(e) = &event {
                            if e.item.item_type == "function_call" {
                                let tc_index = next_tool_call_index;
                                next_tool_call_index += 1;

                                tool_calls_by_output_index.insert(
                                    e.output_index,
                                    ToolCallState {
                                        index: tc_index,
                                        arguments: String::new(),
                                    },
                                );

                                if is_first_chunk {
                                    let role_chunk = build_chunk(
                                        &ctx,
                                        vec![OpenAIChoice {
                                            index: 0,
                                            delta: OpenAIDelta {
                                                role: Some("assistant".to_string()),
                                                content: None,
                                                tool_calls: None,
                                            },
                                            finish_reason: None,
                                        }],
                                        if ctx.include_usage { Some(None) } else { None },
                                    );
                                    yield Ok(Bytes::from(format_sse(&role_chunk)));
                                    is_first_chunk = false;
                                }

                                let tc_chunk = build_chunk(
                                    &ctx,
                                    vec![OpenAIChoice {
                                        index: 0,
                                        delta: OpenAIDelta {
                                            role: None,
                                            content: None,
                                            tool_calls: Some(vec![OpenAIToolCallDelta {
                                                index: tc_index,
                                                id: Some(e.item.call_id.clone().unwrap_or_default()),
                                                call_type: Some("function".to_string()),
                                                function: Some(OpenAIToolCallDeltaFunction {
                                                    name: Some(e.item.name.clone().unwrap_or_default()),
                                                    arguments: Some(String::new()),
                                                }),
                                            }]),
                                        },
                                        finish_reason: None,
                                    }],
                                    if ctx.include_usage { Some(None) } else { None },
                                );
                                yield Ok(Bytes::from(format_sse(&tc_chunk)));
                            }
                        }
                    }
                    "response.function_call_arguments.delta" => {
                        if let CodexSSEEvent::FunctionCallArgsDelta(e) = &event {
                            if let Some(tc) = tool_calls_by_output_index.get_mut(&e.output_index) {
                                tc.arguments.push_str(&e.delta);
                                let tc_index = tc.index;

                                let arg_chunk = build_chunk(
                                    &ctx,
                                    vec![OpenAIChoice {
                                        index: 0,
                                        delta: OpenAIDelta {
                                            role: None,
                                            content: None,
                                            tool_calls: Some(vec![OpenAIToolCallDelta {
                                                index: tc_index,
                                                id: None,
                                                call_type: None,
                                                function: Some(OpenAIToolCallDeltaFunction {
                                                    name: None,
                                                    arguments: Some(e.delta.clone()),
                                                }),
                                            }]),
                                        },
                                        finish_reason: None,
                                    }],
                                    if ctx.include_usage { Some(None) } else { None },
                                );
                                yield Ok(Bytes::from(format_sse(&arg_chunk)));
                            }
                        }
                    }
                    "response.function_call_arguments.done" => {
                        if let CodexSSEEvent::FunctionCallArgsDone(e) = &event {
                            let arg_delta = if let Some(tc) = tool_calls_by_output_index.get_mut(&e.output_index) {
                                let suffix = if e.arguments.starts_with(&tc.arguments) {
                                    &e.arguments[tc.arguments.len()..]
                                } else if tc.arguments.is_empty() {
                                    e.arguments.as_str()
                                } else {
                                    yield Err(ProxyError::BackendApiError(
                                        "Tool call arguments changed after streamed deltas; cannot represent replacement in OpenAI chat completion stream".to_string(),
                                    ));
                                    return;
                                };
                                let arg_delta = if suffix.is_empty() {
                                    None
                                } else {
                                    Some((tc.index, suffix.to_string()))
                                };
                                tc.arguments = e.arguments.clone();
                                arg_delta
                            } else {
                                None
                            };

                            if let Some((tc_index, arguments)) = arg_delta {
                                let arg_chunk = build_chunk(
                                    &ctx,
                                    vec![OpenAIChoice {
                                        index: 0,
                                        delta: OpenAIDelta {
                                            role: None,
                                            content: None,
                                            tool_calls: Some(vec![OpenAIToolCallDelta {
                                                index: tc_index,
                                                id: None,
                                                call_type: None,
                                                function: Some(OpenAIToolCallDeltaFunction {
                                                    name: None,
                                                    arguments: Some(arguments),
                                                }),
                                            }]),
                                        },
                                        finish_reason: None,
                                    }],
                                    if ctx.include_usage { Some(None) } else { None },
                                );
                                yield Ok(Bytes::from(format_sse(&arg_chunk)));
                            }
                        }
                    }
                    "response.completed" | "response.done" | "response.incomplete" => {
                        if !done_sent {
                            done_sent = true;
                            let response = match &event {
                                CodexSSEEvent::Completed(e) | CodexSSEEvent::Incomplete(e) => {
                                    Some(&e.response)
                                }
                                _ => None,
                            };
                            let (input_tokens, output_tokens) = usage_counts_for_log(
                                response.and_then(|response| response.usage.as_ref()),
                                &ctx.model,
                            );
                            crate::logger::push_log(&ctx.log_buffer, crate::logger::LogEntry {
                                id: uuid::Uuid::new_v4().to_string(),
                                timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                                method: "POST".to_string(),
                                path: ctx.path.clone(),
                                model: Some(ctx.model.clone()),
                                status: 200,
                                duration_ms: 0,
                                input_tokens,
                                output_tokens,
                            });
                            if let Some(usage) =
                                response.and_then(|response| response.usage.as_ref())
                            {
                                record_token_usage(&ctx.db, &ctx.model, &ctx.path, usage).await;
                            }
                            let has_tool_calls = !tool_calls_by_output_index.is_empty();
                            let finish_reason =
                                map_finish_reason_from_response(response, has_tool_calls)
                                    .or_else(|| {
                                        Some(
                                            if has_tool_calls {
                                                "tool_calls"
                                            } else {
                                                "stop"
                                            }
                                            .to_string(),
                                        )
                                    });

                            let usage =
                                map_usage(response.and_then(|response| response.usage.as_ref()));

                            let finish_chunk = build_chunk(
                                &ctx,
                                vec![OpenAIChoice {
                                    index: 0,
                                    delta: OpenAIDelta {
                                        role: None,
                                        content: None,
                                        tool_calls: None,
                                    },
                                    finish_reason,
                                }],
                                if ctx.include_usage { Some(None) } else { None },
                            );
                            yield Ok(Bytes::from(format_sse(&finish_chunk)));

                            if ctx.include_usage {
                                let usage_chunk = build_chunk(&ctx, vec![], Some(Some(usage)));
                                yield Ok(Bytes::from(format_sse(&usage_chunk)));
                            }

                            yield Ok(Bytes::from("data: [DONE]\n\n"));
                        }
                    }
                    "response.failed" => {
                        if let CodexSSEEvent::Failed(e) = &event {
                            yield Err(ProxyError::BackendApiError(e.response.error.message.clone()));
                            return;
                        }
                    }
                    _ => {}
                }
            }

            if buffer.len() > MAX_SSE_BUFFER_BYTES {
                yield Err(sse_frame_too_large_error());
                return;
            }
        }

        let remaining = match decode_remaining_sse_line(&buffer) {
            Ok(remaining) => remaining,
            Err(error) => {
                yield Err(error);
                return;
            }
        };
        if !remaining.trim().is_empty() {
            if let Some(event) = parse_sse_line(remaining.trim()) {
                if event.event_type() == "response.output_text.delta" {
                    if let CodexSSEEvent::Delta(e) = &event {
                        let content_chunk = build_chunk(
                            &ctx,
                            vec![OpenAIChoice {
                                index: 0,
                                delta: OpenAIDelta {
                                    role: None,
                                    content: Some(e.delta.clone()),
                                    tool_calls: None,
                                },
                                finish_reason: None,
                            }],
                            if ctx.include_usage { Some(None) } else { None },
                        );
                        yield Ok(Bytes::from(format_sse(&content_chunk)));
                    }
                }
            }
        }
    }
}

pub async fn collect_openai_response(
    response: reqwest::Response,
    ctx: &OpenAIStreamContext,
) -> Result<serde_json::Value> {
    use crate::types::openai::{OpenAIToolCall, OpenAIToolCallFunction};
    use futures::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut full_text = String::new();
    let mut tool_calls: Vec<(u32, String, String, String)> = Vec::new();
    let mut tool_calls_by_output_index: HashMap<u32, usize> = HashMap::new();
    let mut final_response: Option<crate::types::codex::CodexSSEResponseData> = None;
    let mut saw_terminal_response = false;
    let mut usage = OpenAIUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        prompt_tokens_details: None,
        completion_tokens_details: None,
    };

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(ProxyError::HttpError)?;
        append_sse_chunk(&mut buffer, &chunk)?;

        while let Some(line) = drain_next_sse_line(&mut buffer)? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let event = match parse_sse_line(trimmed) {
                Some(e) => e,
                None => continue,
            };

            match event.event_type() {
                "response.output_text.delta" => {
                    if let CodexSSEEvent::Delta(e) = &event {
                        full_text.push_str(&e.delta);
                    }
                }
                "response.output_item.done" => {
                    if let CodexSSEEvent::OutputItemDone(e) = &event {
                        if let Some(content) = &e.item.content {
                            if let Some(first) = content.first() {
                                full_text = first.text.clone();
                            }
                        }
                    }
                }
                "response.output_item.added" => {
                    if let CodexSSEEvent::OutputItemAdded(e) = &event {
                        if e.item.item_type == "function_call" {
                            let idx = tool_calls.len();
                            tool_calls.push((
                                idx as u32,
                                e.item.call_id.clone().unwrap_or_default(),
                                e.item.name.clone().unwrap_or_default(),
                                String::new(),
                            ));
                            tool_calls_by_output_index.insert(e.output_index, idx);
                        }
                    }
                }
                "response.function_call_arguments.delta" => {
                    if let CodexSSEEvent::FunctionCallArgsDelta(e) = &event {
                        if let Some(&tc_idx) = tool_calls_by_output_index.get(&e.output_index) {
                            tool_calls[tc_idx].3.push_str(&e.delta);
                        }
                    }
                }
                "response.function_call_arguments.done" => {
                    if let CodexSSEEvent::FunctionCallArgsDone(e) = &event {
                        if let Some(&tc_idx) = tool_calls_by_output_index.get(&e.output_index) {
                            tool_calls[tc_idx].3 = e.arguments.clone();
                        }
                    }
                }
                "response.completed" | "response.done" | "response.incomplete" => {
                    saw_terminal_response = true;
                    if let CodexSSEEvent::Completed(e) | CodexSSEEvent::Incomplete(e) = &event {
                        usage = map_usage(e.response.usage.as_ref());
                        final_response = Some(e.response.clone());
                    }
                }
                "response.failed" => {
                    if let CodexSSEEvent::Failed(e) = &event {
                        return Err(ProxyError::BackendApiError(
                            e.response.error.message.clone(),
                        ));
                    }
                }
                _ => {}
            }
        }

        if buffer.len() > MAX_SSE_BUFFER_BYTES {
            return Err(sse_frame_too_large_error());
        }
    }

    let has_tool_calls = !tool_calls.is_empty();
    let formatted_tool_calls: Vec<OpenAIToolCall> = tool_calls
        .iter()
        .map(|(_, id, name, args)| OpenAIToolCall {
            id: id.clone(),
            call_type: "function".to_string(),
            function: OpenAIToolCallFunction {
                name: name.clone(),
                arguments: args.clone(),
            },
        })
        .collect();
    if !saw_terminal_response {
        return Err(ProxyError::BackendApiError(
            "No completed response found in SSE stream".to_string(),
        ));
    }
    let finish_reason = map_finish_reason_from_response(final_response.as_ref(), has_tool_calls)
        .or_else(|| Some(if has_tool_calls { "tool_calls" } else { "stop" }.to_string()));
    let mut message = serde_json::json!({
        "role": "assistant",
        "content": if has_tool_calls {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(full_text)
        },
    });
    if has_tool_calls {
        if let Some(message) = message.as_object_mut() {
            message.insert(
                "tool_calls".to_string(),
                serde_json::to_value(&formatted_tool_calls).unwrap_or(serde_json::Value::Null),
            );
        }
    }

    Ok(serde_json::json!({
        "id": ctx.completion_id,
        "object": "chat.completion",
        "created": ctx.created,
        "model": ctx.model,
        "system_fingerprint": ctx.system_fingerprint,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason,
        }],
        "usage": usage,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

    #[tokio::test]
    async fn test_context_creation() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "gpt-5.3-codex".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        assert!(ctx.completion_id.starts_with("chatcmpl-"));
        assert!(ctx.system_fingerprint.starts_with("fp_"));
        assert_eq!(ctx.model, "gpt-5.3-codex");
        assert!(!ctx.include_usage);
    }

    #[test]
    fn test_map_usage_none() {
        let usage = map_usage(None);
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_map_usage_some() {
        let codex_usage = crate::types::codex::CodexSSEResponseUsage {
            input_tokens: 10,
            output_tokens: 20,
            total_tokens: 30,
            input_tokens_details: None,
            output_tokens_details: None,
        };
        let usage = map_usage(Some(&codex_usage));
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.total_tokens, 30);
    }

    #[tokio::test]
    async fn test_build_chunk() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "gpt-5.3-codex".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let chunk = build_chunk(
            &ctx,
            vec![OpenAIChoice {
                index: 0,
                delta: OpenAIDelta {
                    role: Some("assistant".to_string()),
                    content: None,
                    tool_calls: None,
                },
                finish_reason: None,
            }],
            None,
        );
        assert_eq!(chunk.id, ctx.completion_id);
        assert_eq!(chunk.object, "chat.completion.chunk");
        assert_eq!(chunk.choices.len(), 1);
    }

    #[test]
    fn test_parse_sse_line() {
        let line = r#"data: {"type":"response.output_text.delta","delta":"hello"}"#;
        let event = parse_sse_line(line);
        assert!(matches!(event, Some(CodexSSEEvent::Delta(_))));
    }

    #[tokio::test]
    async fn test_openai_stream_text_delta_and_done() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\" world\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"total_tokens\":15}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "test-model".to_string(),
            true,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_openai_stream(ctx, response).collect().await;

        assert_eq!(chunks.len(), 6);
        let role = String::from_utf8(chunks[0].as_ref().unwrap().to_vec()).unwrap();
        assert!(role.contains("\"role\":\"assistant\""));

        let content = String::from_utf8(chunks[1].as_ref().unwrap().to_vec()).unwrap();
        assert!(content.contains("\"content\":\"Hello\""));

        let finish = String::from_utf8(chunks[3].as_ref().unwrap().to_vec()).unwrap();
        assert!(finish.contains("\"finish_reason\":\"stop\""));
        assert!(!finish.contains("\"usage\":{"));
        assert!(finish.contains("\"usage\":null"));

        let usage = String::from_utf8(chunks[4].as_ref().unwrap().to_vec()).unwrap();
        assert!(usage.contains("\"choices\":[]"));
        assert!(usage.contains("\"usage\":{"));
        assert!(usage.contains("\"total_tokens\":15"));

        let done = String::from_utf8(chunks[5].as_ref().unwrap().to_vec()).unwrap();
        assert_eq!(done, "data: [DONE]\n\n");
    }

    #[tokio::test]
    async fn test_openai_stream_tool_calls_state_machine() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"search\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\"{\\\"q\\\"\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\":\\\"x\\\"}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\"}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "test-model".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_openai_stream(ctx, response).collect().await;

        assert_eq!(chunks.len(), 6);
        let tc_header = String::from_utf8(chunks[1].as_ref().unwrap().to_vec()).unwrap();
        assert!(tc_header.contains("\"tool_calls\""));
        assert!(tc_header.contains("\"id\":\"call_1\""));
        assert!(tc_header.contains("\"name\":\"search\""));

        let arg_delta = String::from_utf8(chunks[2].as_ref().unwrap().to_vec()).unwrap();
        assert!(arg_delta.contains("\\\"q\\\""));

        let finish = String::from_utf8(chunks[4].as_ref().unwrap().to_vec()).unwrap();
        assert!(finish.contains("\"finish_reason\":\"tool_calls\""));
    }

    #[tokio::test]
    async fn test_openai_stream_tool_call_arguments_done_without_delta() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"search\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"output_index\":0,\"arguments\":\"{\\\"q\\\":\\\"x\\\"}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\"}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "test-model".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_openai_stream(ctx, response).collect().await;

        assert_eq!(chunks.len(), 5);
        let arg_done = String::from_utf8(chunks[2].as_ref().unwrap().to_vec()).unwrap();
        assert!(arg_done.contains("\\\"q\\\":\\\"x\\\""));

        let finish = String::from_utf8(chunks[3].as_ref().unwrap().to_vec()).unwrap();
        assert!(finish.contains("\"finish_reason\":\"tool_calls\""));
    }

    #[tokio::test]
    async fn test_openai_stream_tool_call_arguments_done_mismatch_errors() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"search\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\"{\\\"q\\\":\\\"partial\\\"}\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"output_index\":0,\"arguments\":\"{\\\"q\\\":\\\"final\\\"}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\"}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "test-model".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_openai_stream(ctx, response).collect().await;

        assert_eq!(chunks.len(), 4);
        assert!(chunks[3].as_ref().is_err());
    }

    #[tokio::test]
    async fn test_collect_openai_response_tool_calls_and_done_arguments() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"search\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\"{\\\"q\\\":\\\"partial\\\"}\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"output_index\":0,\"arguments\":\"{\\\"q\\\":\\\"test\\\"}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"usage\":{\"input_tokens\":5,\"output_tokens\":8,\"total_tokens\":13}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "gpt-5".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let result = collect_openai_response(response, &ctx).await.unwrap();

        assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(
            result["choices"][0]["message"]["content"],
            serde_json::Value::Null
        );
        assert_eq!(
            result["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"q\":\"test\"}"
        );
        assert_eq!(result["usage"]["prompt_tokens"], 5);
    }

    #[tokio::test]
    async fn test_collect_openai_response_omits_tool_calls_when_absent() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"usage\":{\"input_tokens\":5,\"output_tokens\":8,\"total_tokens\":13}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "gpt-5".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let result = collect_openai_response(response, &ctx).await.unwrap();

        assert_eq!(result["choices"][0]["message"]["content"], "Hello");
        assert!(result["choices"][0]["message"].get("tool_calls").is_none());
        assert_eq!(result["choices"][0]["finish_reason"], "stop");
    }

    #[tokio::test]
    async fn test_openai_stream_incomplete_maps_to_length_finish_reason() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\n",
            "data: {\"type\":\"response.incomplete\",\"response\":{\"id\":\"r1\",\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"},\"usage\":{\"input_tokens\":5,\"output_tokens\":8,\"total_tokens\":13}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "test-model".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_openai_stream(ctx, response).collect().await;

        let finish = chunks
            .iter()
            .filter_map(|chunk| chunk.as_ref().ok())
            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
            .find(|chunk| chunk.contains("\"finish_reason\":\"length\""))
            .expect("finish chunk");
        assert!(finish.contains("\"finish_reason\":\"length\""));
    }

    #[tokio::test]
    async fn test_openai_stream_done_sent_only_once() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"x\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\"}}\n\n",
            "data: {\"type\":\"response.done\",\"response\":{\"id\":\"r1\"}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = OpenAIStreamContext::new(
            "test-model".to_string(),
            false,
            log_buffer,
            db,
            "/v1/chat/completions".to_string(),
        );
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_openai_stream(ctx, response).collect().await;

        let done_count = chunks
            .iter()
            .filter(|item| {
                let Ok(bytes) = item else { return false };
                String::from_utf8_lossy(bytes).contains("[DONE]")
            })
            .count();
        let finish_count = chunks
            .iter()
            .filter(|item| {
                let Ok(bytes) = item else { return false };
                String::from_utf8_lossy(bytes).contains("\"finish_reason\":\"stop\"")
            })
            .count();

        assert_eq!(done_count, 1);
        assert_eq!(finish_count, 1);
    }
}
