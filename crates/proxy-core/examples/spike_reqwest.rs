use std::{fs, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

const CHATGPT_BACKEND_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const MAX_SSE_EVENTS: usize = 5;

#[derive(Debug, Deserialize)]
struct AuthFile {
    tokens: Option<AuthTokens>,
}

#[derive(Debug, Deserialize)]
struct AuthTokens {
    access_token: Option<String>,
    account_id: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let auth = load_auth_file().context("failed to load ~/.codex/auth.json")?;
    let tokens = auth
        .tokens
        .ok_or_else(|| anyhow!("missing tokens object in ~/.codex/auth.json"))?;
    let access_token = tokens
        .access_token
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| anyhow!("missing tokens.access_token in ~/.codex/auth.json"))?;
    let account_id = tokens.account_id.filter(|v| !v.trim().is_empty());

    let session_id = Uuid::new_v4().to_string();
    let mut headers = browser_headers();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {access_token}"))
            .context("invalid Authorization header value")?,
    );
    headers.insert(
        "session_id",
        HeaderValue::from_str(&session_id).context("invalid session_id header value")?,
    );

    if let Some(account_id) = account_id {
        headers.insert(
            "ChatGPT-Account-ID",
            HeaderValue::from_str(&account_id)
                .context("invalid ChatGPT-Account-ID header value")?,
        );
    }

    let body = json!({
        "model": "gpt-5.3-codex",
        "instructions": "",
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": "say hello"
            }]
        }],
        "tools": [],
        "tool_choice": "auto",
        "parallel_tool_calls": false,
        "store": false,
        "stream": true,
        "include": ["usage"]
    });

    let client = reqwest::Client::new();
    let response = client
        .post(CHATGPT_BACKEND_URL)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .context("request to ChatGPT backend failed")?;

    let status = response.status();
    println!("HTTP status: {status}");

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut event_count = 0usize;

    while event_count < MAX_SSE_EVENTS {
        let next = stream.next().await;
        let Some(chunk_result) = next else {
            break;
        };

        let chunk = chunk_result.context("failed to read response stream chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while event_count < MAX_SSE_EVENTS {
            let Some(split_pos) = buffer.find("\n\n") else {
                break;
            };

            let raw_event = buffer[..split_pos].trim().to_string();
            buffer.drain(..split_pos + 2);

            if raw_event.is_empty() {
                continue;
            }

            event_count += 1;
            println!("SSE event {event_count}: {raw_event}");
        }
    }

    if event_count == 0 {
        println!("No SSE events received.");
    }

    if status.is_success() {
        println!("Decision hint: GO (reqwest connectivity works).");
    } else {
        println!("Decision hint: NO-GO (reqwest connectivity failed).\n");
    }

    Ok(())
}

fn load_auth_file() -> Result<AuthFile> {
    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    let auth_path = PathBuf::from(home).join(".codex").join("auth.json");
    let content = fs::read_to_string(&auth_path)
        .with_context(|| format!("failed to read {}", auth_path.display()))?;
    let auth = serde_json::from_str::<AuthFile>(&content)
        .with_context(|| format!("invalid JSON in {}", auth_path.display()))?;
    Ok(auth)
}

fn browser_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    headers.insert("Accept", HeaderValue::from_static("text/event-stream"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert("Referer", HeaderValue::from_static("https://chatgpt.com/"));
    headers.insert("Origin", HeaderValue::from_static("https://chatgpt.com"));
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-origin"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert("DNT", HeaderValue::from_static("1"));
    headers.insert("OpenAI-Beta", HeaderValue::from_static("responses=experimental"));
    headers.insert("originator", HeaderValue::from_static("codex_cli_rs"));
    headers.insert(
        "User-Agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        ),
    );

    headers
}
