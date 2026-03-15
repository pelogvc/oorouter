use bytes::Bytes;
use futures::Stream;
use std::sync::Arc;

use crate::db::Database;
use crate::error::{ProxyError, Result};
use crate::logger::LogBuffer;
use crate::types::codex::CodexSSEEvent;

pub fn parse_sse_line(line: &str) -> Option<CodexSSEEvent> {
    if !line.starts_with("data: ") {
        return None;
    }

    let data = line[6..].trim();
    if data == "[DONE]" {
        return None;
    }

    serde_json::from_str(data).ok()
}

fn create_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn create_final_metrics(start_time: std::time::Instant) -> serde_json::Value {
    let total_ns = start_time.elapsed().as_nanos() as u64;

    serde_json::json!({
        "total_duration": total_ns,
        "load_duration": 0,
        "prompt_eval_count": 0,
        "prompt_eval_duration": 0,
        "eval_count": 0,
        "eval_duration": total_ns,
    })
}

pub struct StreamContext {
    pub model: String,
    pub start_time: std::time::Instant,
    pub log_buffer: LogBuffer,
    pub db: Arc<Database>,
    pub path: String,
}

pub struct CollectedResponse {
    pub text: String,
    pub usage: Option<crate::types::codex::CodexSSEResponseUsage>,
}

impl StreamContext {
    pub fn new(model: String, log_buffer: LogBuffer, db: Arc<Database>, path: String) -> Self {
        StreamContext {
            model,
            start_time: std::time::Instant::now(),
            log_buffer,
            db,
            path,
        }
    }
}

pub fn create_chat_stream(
    ctx: StreamContext,
    response: reqwest::Response,
) -> impl Stream<Item = std::result::Result<Bytes, ProxyError>> {
    let mut buffer = String::new();
    let mut done_sent = false;

    async_stream::stream! {
        use futures::StreamExt;
        let mut stream = response.bytes_stream();

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

                let Some(event) = parse_sse_line(trimmed) else {
                    continue;
                };

                match event.event_type() {
                    "response.output_text.delta" => {
                        if let CodexSSEEvent::Delta(e) = &event {
                            let chunk_json = serde_json::json!({
                                "model": ctx.model,
                                "created_at": create_timestamp(),
                                "message": {"role": "assistant", "content": e.delta},
                                "done": false,
                            });

                            yield Ok(Bytes::from(format!("{}\n", chunk_json)));
                        }
                    }
                    "response.completed" | "response.done" => {
                        if !done_sent {
                            done_sent = true;
                            let (input_tokens, output_tokens) = if let CodexSSEEvent::Completed(ref e) = event {
                                (
                                    e.response.usage.as_ref().map(|u| u.input_tokens as u32),
                                    e.response.usage.as_ref().map(|u| u.output_tokens as u32),
                                )
                            } else {
                                (None, None)
                            };
                            crate::logger::push_log(&ctx.log_buffer, crate::logger::LogEntry {
                                id: uuid::Uuid::new_v4().to_string(),
                                timestamp: create_timestamp(),
                                method: "POST".to_string(),
                                path: ctx.path.clone(),
                                model: Some(ctx.model.clone()),
                                status: 200,
                                duration_ms: ctx.start_time.elapsed().as_millis() as u64,
                                input_tokens,
                                output_tokens,
                            });
                            if let CodexSSEEvent::Completed(ref e) = event {
                                if let Some(ref usage) = e.response.usage {
                                    let db = ctx.db.clone();
                                    let model = ctx.model.clone();
                                    let path = ctx.path.clone();
                                    let input = usage.input_tokens as i64;
                                    let output = usage.output_tokens as i64;
                                    tokio::spawn(async move {
                                        if let Err(e) = db.insert_token_usage(&model, "codex", input, output, &path).await {
                                            eprintln!("[usage] stream insert failed: {e}");
                                        }
                                    });
                                }
                            }
                            let metrics = create_final_metrics(ctx.start_time);
                            let mut final_json = serde_json::json!({
                                "model": ctx.model,
                                "created_at": create_timestamp(),
                                "message": {"role": "assistant", "content": ""},
                                "done": true,
                                "done_reason": "stop",
                            });

                            if let (Some(obj), Some(m)) = (final_json.as_object_mut(), metrics.as_object()) {
                                for (k, v) in m {
                                    obj.insert(k.clone(), v.clone());
                                }
                            }

                            yield Ok(Bytes::from(format!("{}\n", final_json)));
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
                        let chunk_json = serde_json::json!({
                            "model": ctx.model,
                            "created_at": create_timestamp(),
                            "message": {"role": "assistant", "content": e.delta},
                            "done": false,
                        });

                        yield Ok(Bytes::from(format!("{}\n", chunk_json)));
                    }
                }
            }
        }
    }
}

pub fn create_generate_stream(
    ctx: StreamContext,
    response: reqwest::Response,
) -> impl Stream<Item = std::result::Result<Bytes, ProxyError>> {
    let mut buffer = String::new();
    let mut done_sent = false;

    async_stream::stream! {
        use futures::StreamExt;
        let mut stream = response.bytes_stream();

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

                let Some(event) = parse_sse_line(trimmed) else {
                    continue;
                };

                match event.event_type() {
                    "response.output_text.delta" => {
                        if let CodexSSEEvent::Delta(e) = &event {
                            let chunk_json = serde_json::json!({
                                "model": ctx.model,
                                "created_at": create_timestamp(),
                                "response": e.delta,
                                "done": false,
                            });

                            yield Ok(Bytes::from(format!("{}\n", chunk_json)));
                        }
                    }
                    "response.completed" | "response.done" => {
                        if !done_sent {
                            done_sent = true;
                            let (input_tokens, output_tokens) = if let CodexSSEEvent::Completed(ref e) = event {
                                (
                                    e.response.usage.as_ref().map(|u| u.input_tokens as u32),
                                    e.response.usage.as_ref().map(|u| u.output_tokens as u32),
                                )
                            } else {
                                (None, None)
                            };
                            crate::logger::push_log(&ctx.log_buffer, crate::logger::LogEntry {
                                id: uuid::Uuid::new_v4().to_string(),
                                timestamp: create_timestamp(),
                                method: "POST".to_string(),
                                path: ctx.path.clone(),
                                model: Some(ctx.model.clone()),
                                status: 200,
                                duration_ms: ctx.start_time.elapsed().as_millis() as u64,
                                input_tokens,
                                output_tokens,
                            });
                            if let CodexSSEEvent::Completed(ref e) = event {
                                if let Some(ref usage) = e.response.usage {
                                    let db = ctx.db.clone();
                                    let model = ctx.model.clone();
                                    let path = ctx.path.clone();
                                    let input = usage.input_tokens as i64;
                                    let output = usage.output_tokens as i64;
                                    tokio::spawn(async move {
                                        if let Err(e) = db.insert_token_usage(&model, "codex", input, output, &path).await {
                                            eprintln!("[usage] stream insert failed: {e}");
                                        }
                                    });
                                }
                            }
                            let metrics = create_final_metrics(ctx.start_time);
                            let mut final_json = serde_json::json!({
                                "model": ctx.model,
                                "created_at": create_timestamp(),
                                "response": "",
                                "done": true,
                                "done_reason": "stop",
                                "context": [],
                            });

                            if let (Some(obj), Some(m)) = (final_json.as_object_mut(), metrics.as_object()) {
                                for (k, v) in m {
                                    obj.insert(k.clone(), v.clone());
                                }
                            }

                            yield Ok(Bytes::from(format!("{}\n", final_json)));
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
    }
}

pub async fn collect_sse_response(response: reqwest::Response) -> Result<CollectedResponse> {
    use futures::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full_text = String::new();
    let mut collected_usage = None;

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

            let Some(event) = parse_sse_line(trimmed) else {
                continue;
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
                "response.completed" | "response.done" => {
                    if let CodexSSEEvent::Completed(e) = &event {
                        collected_usage = e.response.usage.clone();
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

    Ok(CollectedResponse {
        text: full_text,
        usage: collected_usage,
    })
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
    fn test_parse_sse_line_delta() {
        let line = r#"data: {"type":"response.output_text.delta","delta":"hello"}"#;
        let event = parse_sse_line(line).unwrap();
        assert_eq!(event.event_type(), "response.output_text.delta");
    }

    #[test]
    fn test_parse_sse_line_done() {
        let line = "data: [DONE]";
        assert!(parse_sse_line(line).is_none());
    }

    #[test]
    fn test_parse_sse_line_non_data() {
        let line = "event: message";
        assert!(parse_sse_line(line).is_none());
    }

    #[test]
    fn test_parse_sse_line_invalid_json() {
        let line = "data: {invalid json}";
        assert!(parse_sse_line(line).is_none());
    }

    #[test]
    fn test_parse_sse_line_completed() {
        let line = r#"data: {"type":"response.completed","response":{"id":"r1","usage":{"input_tokens":10,"output_tokens":20,"total_tokens":30}}}"#;
        let event = parse_sse_line(line).unwrap();
        assert_eq!(event.event_type(), "response.completed");
    }

    #[tokio::test]
    async fn test_chat_stream_delta_completed_and_done_sent_once() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
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
        let ctx = StreamContext::new("gpt-5".to_string(), log_buffer, db, "/api/chat".to_string());
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_chat_stream(ctx, response).collect().await;

        assert_eq!(chunks.len(), 2);
        let first: serde_json::Value = serde_json::from_slice(&chunks[0].as_ref().unwrap()).unwrap();
        assert_eq!(first["message"]["content"], "Hi");
        assert_eq!(first["done"], false);

        let second: serde_json::Value =
            serde_json::from_slice(&chunks[1].as_ref().unwrap()).unwrap();
        assert_eq!(second["done"], true);
        assert_eq!(second["done_reason"], "stop");
    }

    #[tokio::test]
    async fn test_chat_stream_failed_returns_backend_error() {
        let sse_body = concat!(
            "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"rate_limit\",\"message\":\"Rate limit exceeded\"}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = StreamContext::new("gpt-5".to_string(), log_buffer, db, "/api/chat".to_string());
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_chat_stream(ctx, response).collect().await;

        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            Err(ProxyError::BackendApiError(msg)) => assert_eq!(msg, "Rate limit exceeded"),
            other => panic!("unexpected stream item: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_generate_stream_delta_and_completed() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"World\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let dir = tempfile::TempDir::new().unwrap();
        let db = std::sync::Arc::new(
            crate::db::Database::new(&dir.path().join("test.db"))
                .await
                .unwrap(),
        );
        let log_buffer = crate::logger::new_log_buffer();
        let ctx = StreamContext::new("gpt-5".to_string(), log_buffer, db, "/api/chat".to_string());
        let chunks: Vec<std::result::Result<Bytes, ProxyError>> =
            create_generate_stream(ctx, response).collect().await;

        assert_eq!(chunks.len(), 2);
        let first: serde_json::Value = serde_json::from_slice(&chunks[0].as_ref().unwrap()).unwrap();
        assert_eq!(first["response"], "World");
        assert_eq!(first["done"], false);

        let second: serde_json::Value =
            serde_json::from_slice(&chunks[1].as_ref().unwrap()).unwrap();
        assert_eq!(second["done"], true);
        assert_eq!(second["context"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_collect_sse_response_returns_usage() {
        let sse_body = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"usage\":{\"input_tokens\":42,\"output_tokens\":17,\"total_tokens\":59}}}\n\n"
        );
        let response = mock_sse_response(sse_body).await;
        let result = collect_sse_response(response).await.unwrap();
        assert_eq!(result.text, "hello");
        let usage = result.usage.unwrap();
        assert_eq!(usage.input_tokens, 42);
        assert_eq!(usage.output_tokens, 17);
    }

    #[tokio::test]
    async fn test_collect_sse_response_no_usage() {
        let sse_body = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n";
        let response = mock_sse_response(sse_body).await;
        let result = collect_sse_response(response).await.unwrap();
        assert_eq!(result.text, "hi");
        assert!(result.usage.is_none());
    }
}
