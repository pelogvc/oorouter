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

pub struct OpenAIStreamContext {
    pub completion_id: String,
    pub created: u64,
    pub model: String,
    pub system_fingerprint: String,
    pub include_usage: bool,
}

impl OpenAIStreamContext {
    pub fn new(model: String, include_usage: bool) -> Self {
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

fn build_chunk(
    ctx: &OpenAIStreamContext,
    choices: Vec<OpenAIChoice>,
    usage: Option<Option<OpenAIUsage>>,
) -> OpenAIChunk {
    OpenAIChunk {
        id: ctx.completion_id.clone(),
        object: "chat.completion.chunk".to_string(),
        created: ctx.created,
        model: ctx.model.clone(),
        system_fingerprint: ctx.system_fingerprint.clone(),
        choices,
        usage: usage.flatten(),
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
        },
        Some(u) => OpenAIUsage {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            total_tokens: u.total_tokens,
        },
    }
}

pub fn create_openai_stream(
    ctx: OpenAIStreamContext,
    response: reqwest::Response,
) -> impl Stream<Item = std::result::Result<Bytes, ProxyError>> {
    let mut buffer = String::new();
    let mut is_first_chunk = true;
    let mut tool_calls_by_output_index: HashMap<u32, ToolCallState> = HashMap::new();
    let mut next_tool_call_index: u32 = 0;
    let mut done_sent = false;

    async_stream::stream! {
        let mut stream = response.bytes_stream();
        use futures::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(ProxyError::HttpError)?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            let lines: Vec<&str> = buffer.split('\n').collect();
            let last = lines.last().map(|s| s.to_string()).unwrap_or_default();

            for line in &lines[..lines.len() - 1] {
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
                    "response.completed" | "response.done" => {
                        if !done_sent {
                            done_sent = true;
                            let has_tool_calls = !tool_calls_by_output_index.is_empty();
                            let finish_reason = if has_tool_calls { "tool_calls" } else { "stop" };

                            let usage = if let CodexSSEEvent::Completed(e) = &event {
                                map_usage(e.response.usage.as_ref())
                            } else {
                                OpenAIUsage {
                                    prompt_tokens: 0,
                                    completion_tokens: 0,
                                    total_tokens: 0,
                                }
                            };

                            let finish_chunk = build_chunk(
                                &ctx,
                                vec![OpenAIChoice {
                                    index: 0,
                                    delta: OpenAIDelta {
                                        role: None,
                                        content: None,
                                        tool_calls: None,
                                    },
                                    finish_reason: Some(finish_reason.to_string()),
                                }],
                                if ctx.include_usage {
                                    Some(Some(usage))
                                } else {
                                    None
                                },
                            );
                            yield Ok(Bytes::from(format_sse(&finish_chunk)));
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

            buffer = last;
        }

        if !buffer.trim().is_empty() {
            if let Some(event) = parse_sse_line(buffer.trim()) {
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
    let mut buffer = String::new();
    let mut full_text = String::new();
    let mut tool_calls: Vec<(u32, String, String, String)> = Vec::new();
    let mut tool_calls_by_output_index: HashMap<u32, usize> = HashMap::new();
    let mut usage = OpenAIUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    };

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(ProxyError::HttpError)?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        let lines: Vec<&str> = buffer.split('\n').collect();
        let last = lines.last().map(|s| s.to_string()).unwrap_or_default();

        for line in &lines[..lines.len() - 1] {
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
                "response.completed" | "response.done" => {
                    if let CodexSSEEvent::Completed(e) = &event {
                        usage = map_usage(e.response.usage.as_ref());
                    }
                }
                "response.failed" => {
                    if let CodexSSEEvent::Failed(e) = &event {
                        return Err(ProxyError::BackendApiError(e.response.error.message.clone()));
                    }
                }
                _ => {}
            }
        }

        buffer = last;
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

    Ok(serde_json::json!({
        "id": ctx.completion_id,
        "object": "chat.completion",
        "created": ctx.created,
        "model": ctx.model,
        "system_fingerprint": ctx.system_fingerprint,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": if has_tool_calls { serde_json::Value::Null } else { serde_json::Value::String(full_text) },
                "tool_calls": if has_tool_calls {
                    serde_json::to_value(&formatted_tool_calls).unwrap_or(serde_json::Value::Null)
                } else {
                    serde_json::Value::Null
                },
            },
            "finish_reason": if has_tool_calls { "tool_calls" } else { "stop" },
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

    #[test]
    fn test_context_creation() {
        let ctx = OpenAIStreamContext::new("gpt-5.3-codex".to_string(), false);
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

    #[test]
    fn test_build_chunk() {
        let ctx = OpenAIStreamContext::new("gpt-5.3-codex".to_string(), false);
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
        let ctx = OpenAIStreamContext::new("test-model".to_string(), true);
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_openai_stream(ctx, response).collect().await;

        assert_eq!(chunks.len(), 5);
        let role = String::from_utf8(chunks[0].as_ref().unwrap().to_vec()).unwrap();
        assert!(role.contains("\"role\":\"assistant\""));

        let content = String::from_utf8(chunks[1].as_ref().unwrap().to_vec()).unwrap();
        assert!(content.contains("\"content\":\"Hello\""));

        let finish = String::from_utf8(chunks[3].as_ref().unwrap().to_vec()).unwrap();
        assert!(finish.contains("\"finish_reason\":\"stop\""));
        assert!(finish.contains("\"usage\":{"));

        let done = String::from_utf8(chunks[4].as_ref().unwrap().to_vec()).unwrap();
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
        let ctx = OpenAIStreamContext::new("test-model".to_string(), false);
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
    async fn test_collect_openai_response_tool_calls_and_done_arguments() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"search\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"delta\":\"{\\\"q\\\":\\\"partial\\\"}\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"output_index\":0,\"arguments\":\"{\\\"q\\\":\\\"test\\\"}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"usage\":{\"input_tokens\":5,\"output_tokens\":8,\"total_tokens\":13}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let ctx = OpenAIStreamContext::new("gpt-5".to_string(), false);
        let result = collect_openai_response(response, &ctx).await.unwrap();

        assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(result["choices"][0]["message"]["content"], serde_json::Value::Null);
        assert_eq!(
            result["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"q\":\"test\"}"
        );
        assert_eq!(result["usage"]["prompt_tokens"], 5);
    }

    #[tokio::test]
    async fn test_openai_stream_done_sent_only_once() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"x\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\"}}\n\n",
            "data: {\"type\":\"response.done\",\"response\":{\"id\":\"r1\"}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let ctx = OpenAIStreamContext::new("test-model".to_string(), false);
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
