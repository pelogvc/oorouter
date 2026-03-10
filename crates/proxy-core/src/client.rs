use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};
use std::str::FromStr;
use uuid::Uuid;

use crate::auth::AuthInfo;
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

pub struct CodexClient {
    client: Client,
    auth: AuthInfo,
    api_url: String,
    session_id: String,
}

impl CodexClient {
    pub fn new(auth: AuthInfo, api_url: String) -> Self {
        CodexClient {
            client: Client::new(),
            auth,
            api_url,
            session_id: Uuid::new_v4().to_string(),
        }
    }

    pub async fn send_request(&self, body: &CodexResponsesRequest) -> Result<reqwest::Response> {
        let mut headers = HeaderMap::new();

        for (name, value) in BROWSER_HEADERS {
            headers.insert(
                HeaderName::from_str(name).map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
                HeaderValue::from_str(value).map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
            );
        }

        let auth_headers = crate::auth::get_auth_headers(&self.auth);
        for (name, value) in &auth_headers {
            headers.insert(
                HeaderName::from_str(name).map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
                HeaderValue::from_str(value).map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
            );
        }

        headers.insert(
            HeaderName::from_str("session_id").map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
            HeaderValue::from_str(&self.session_id).map_err(|e| ProxyError::BackendApiError(e.to_string()))?,
        );

        let response = self
            .client
            .post(&self.api_url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(ProxyError::HttpError)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(ProxyError::BackendApiError(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        Ok(response)
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
}
