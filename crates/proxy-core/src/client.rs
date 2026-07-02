use std::{collections::HashSet, str::FromStr};

use futures::StreamExt;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Url,
};
use uuid::Uuid;

use crate::auth::AuthInfo;
use crate::auth_watcher::{read_shared_auth, SharedAuth};
use crate::error::{ProxyError, Result};
use crate::types::codex::CodexResponsesRequest;

const BROWSER_HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("Accept", "text/event-stream"),
    ("Accept-Language", "en-US,en;q=0.9"),
    ("Referer", "https://chatgpt.com/"),
    ("Origin", "https://chatgpt.com"),
    ("Sec-Fetch-Dest", "empty"),
    ("Sec-Fetch-Mode", "cors"),
    ("Sec-Fetch-Site", "same-origin"),
    ("Cache-Control", "no-cache"),
    ("DNT", "1"),
    ("OpenAI-Beta", "responses=experimental"),
    ("originator", "codex_cli_rs"),
    (
        "User-Agent",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    ),
];
const MAX_BACKEND_ERROR_BODY_BYTES: usize = 64 * 1024;
const MAX_MODELS_RESPONSE_BODY_BYTES: usize = 4 * 1024 * 1024;
const REDACTED_BACKEND_RESPONSE: &str = "<redacted sensitive backend response>";

pub(crate) fn redact_sensitive_text(input: &str) -> String {
    let mut output = input.to_string();
    let lower = input.to_ascii_lowercase();
    let normalized: String = lower
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();
    let contains_sensitive_marker = [
        "accesstoken",
        "refreshtoken",
        "idtoken",
        "authorization",
        "apikey",
        "openaiapikey",
        "setcookie",
        "cookie",
        "session",
        "secret",
        "token",
        "password",
        "credential",
        "clientsecret",
        "privatekey",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
        || lower.contains("bearer ");

    if contains_sensitive_marker {
        output = REDACTED_BACKEND_RESPONSE.to_string();
    }

    const MAX_ERROR_BODY_CHARS: usize = 512;
    if output.chars().count() > MAX_ERROR_BODY_CHARS {
        output = output
            .chars()
            .take(MAX_ERROR_BODY_CHARS)
            .collect::<String>();
        output.push_str("...");
    }
    output
}

async fn read_limited_error_body(response: reqwest::Response) -> String {
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => return format!("<failed to read error body: {}>", error),
        };

        if body.len().saturating_add(chunk.len()) > MAX_BACKEND_ERROR_BODY_BYTES {
            return format!(
                "<upstream error body exceeded {} bytes>",
                MAX_BACKEND_ERROR_BODY_BYTES
            );
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8_lossy(&body).into_owned()
}

async fn read_limited_response_text(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<String> {
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(ProxyError::HttpError)?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(ProxyError::BackendApiError(format!(
                "Upstream response body exceeded {} bytes",
                max_bytes
            )));
        }
        body.extend_from_slice(&chunk);
    }

    Ok(String::from_utf8_lossy(&body).into_owned())
}

pub struct CodexClient {
    client: Client,
    auth: SharedAuth,
    api_url: String,
    session_id: String,
}

impl CodexClient {
    pub fn new(auth: AuthInfo, api_url: String) -> Self {
        Self::new_with_shared_auth(crate::auth_watcher::new_shared_auth(auth), api_url)
    }

    pub fn new_with_shared_auth(auth: SharedAuth, api_url: String) -> Self {
        CodexClient {
            client: Client::new(),
            auth,
            api_url,
            session_id: Uuid::new_v4().to_string(),
        }
    }

    fn build_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();

        for (name, value) in BROWSER_HEADERS {
            headers.insert(
                HeaderName::from_str(name)
                    .map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
                HeaderValue::from_str(value)
                    .map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
            );
        }

        let auth = read_shared_auth(&self.auth)?;
        let auth_headers = crate::auth::get_auth_headers(&auth);
        for (name, value) in &auth_headers {
            headers.insert(
                HeaderName::from_str(name)
                    .map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
                HeaderValue::from_str(value)
                    .map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
            );
        }

        headers.insert(
            HeaderName::from_str("session_id")
                .map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
            HeaderValue::from_str(&self.session_id)
                .map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
        );

        Ok(headers)
    }

    async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response> {
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = read_limited_error_body(response).await;
            let text = redact_sensitive_text(&text);
            return Err(ProxyError::BackendApiError(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        Ok(response)
    }

    fn backend_url_for(&self, endpoint: &str) -> Result<Url> {
        let mut url = Url::parse(&self.api_url)
            .map_err(|e| ProxyError::BackendApiError(format!("Invalid backend URL: {}", e)))?;
        let mut path = url.path().trim_end_matches('/').to_string();

        if path.ends_with("/responses") {
            path.truncate(path.len() - "/responses".len());
        }

        let endpoint = endpoint.trim_start_matches('/');
        let next_path = if path.is_empty() {
            format!("/{}", endpoint)
        } else {
            format!("{}/{}", path, endpoint)
        };

        url.set_path(&next_path);
        Ok(url)
    }

    pub async fn send_request(&self, body: &CodexResponsesRequest) -> Result<reqwest::Response> {
        let response = self
            .client
            .post(&self.api_url)
            .headers(self.build_headers()?)
            .json(body)
            .send()
            .await
            .map_err(ProxyError::HttpError)?;

        Self::ensure_success(response).await
    }

    pub async fn send_raw_responses_request(
        &self,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response> {
        self.client
            .post(&self.api_url)
            .headers(self.build_headers()?)
            .json(body)
            .send()
            .await
            .map_err(ProxyError::HttpError)
    }

    pub async fn fetch_model_slugs(&self, codex_client_version: &str) -> Result<Vec<String>> {
        let mut url = self.backend_url_for("models")?;
        url.query_pairs_mut()
            .append_pair("client_version", codex_client_version);
        let mut headers = self.build_headers()?;
        headers.insert(
            reqwest::header::ACCEPT,
            HeaderValue::from_static("application/json"),
        );

        let response = self
            .client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(ProxyError::HttpError)?;
        let status = response.status();
        if !status.is_success() {
            let body = read_limited_error_body(response).await;
            let body = redact_sensitive_text(&body);
            return Err(ProxyError::BackendApiError(format!(
                "HTTP {}: {}",
                status.as_u16(),
                body
            )));
        }
        let body = read_limited_response_text(response, MAX_MODELS_RESPONSE_BODY_BYTES).await?;

        let parsed: serde_json::Value = serde_json::from_str(&body)?;
        let models = parsed
            .get("models")
            .and_then(|models| models.as_array())
            .ok_or_else(|| ProxyError::BackendApiError("Malformed models response".to_string()))?;

        let mut seen: HashSet<&str> = HashSet::new();
        let mut slugs = Vec::new();
        for model in models {
            let Some(slug) = model.get("slug").and_then(|slug| slug.as_str()) else {
                continue;
            };
            if slug.is_empty() || !seen.insert(slug) {
                continue;
            }
            slugs.push(slug.to_string());
        }

        if slugs.is_empty() {
            return Err(ProxyError::BackendApiError(
                "Codex returned an empty models list".to_string(),
            ));
        }

        Ok(slugs)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthInfo, AuthMode};

    #[test]
    fn test_session_id_stable() {
        let auth = AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: "test".to_string(),
            account_id: None,
        };
        let client = CodexClient::new(auth, "https://example.com".to_string());
        let id1 = client.session_id().to_string();
        let id2 = client.session_id().to_string();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_browser_headers_include_originator() {
        let has_originator = BROWSER_HEADERS
            .iter()
            .any(|(k, v)| *k == "originator" && *v == "codex_cli_rs");
        assert!(has_originator, "originator header must be codex_cli_rs");
    }

    #[test]
    fn test_browser_headers_count() {
        assert_eq!(BROWSER_HEADERS.len(), 13, "Should have 13 browser headers");
    }

    #[test]
    fn test_backend_url_for_replaces_responses_endpoint() {
        let auth = AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: "test".to_string(),
            account_id: None,
        };
        let client = CodexClient::new(
            auth,
            "https://chatgpt.com/backend-api/codex/responses".to_string(),
        );

        assert_eq!(
            client.backend_url_for("models").unwrap().as_str(),
            "https://chatgpt.com/backend-api/codex/models"
        );
    }

    #[test]
    fn test_backend_url_for_preserves_query_params() {
        let auth = AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: "test".to_string(),
            account_id: None,
        };
        let client = CodexClient::new(
            auth,
            "https://example.test/backend-api/codex/responses?tenant=test".to_string(),
        );

        assert_eq!(
            client.backend_url_for("models").unwrap().as_str(),
            "https://example.test/backend-api/codex/models?tenant=test"
        );
    }

    #[test]
    fn test_build_headers_reads_shared_auth_updates() {
        let shared_auth = crate::auth_watcher::new_shared_auth(AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: "old-token".to_string(),
            account_id: None,
        });
        let client = CodexClient::new_with_shared_auth(
            shared_auth.clone(),
            "https://example.com".to_string(),
        );

        {
            let mut guard = shared_auth.write().unwrap();
            guard.as_mut().unwrap().access_token = "new-token".to_string();
        }

        let headers = client.build_headers().unwrap();
        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer new-token"
        );
    }

    #[test]
    fn test_build_headers_errors_when_shared_auth_invalidated() {
        let shared_auth = crate::auth_watcher::new_shared_auth(AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: "old-token".to_string(),
            account_id: None,
        });
        let client = CodexClient::new_with_shared_auth(
            shared_auth.clone(),
            "https://example.com".to_string(),
        );
        *shared_auth.write().unwrap() = None;

        assert!(client.build_headers().is_err());
    }

    #[test]
    fn test_redact_sensitive_backend_error_body() {
        assert_eq!(
            redact_sensitive_text(r#"{"access_token":"secret"}"#),
            "<redacted sensitive backend response>"
        );
        assert!(redact_sensitive_text(&"x".repeat(600)).len() < 520);
    }
}
