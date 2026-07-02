use std::{fmt, path::Path};

use serde::{Deserialize, Serialize};

use crate::error::{ProxyError, Result};

#[derive(Clone, Serialize, Deserialize)]
pub struct CodexTokenData {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    pub id_token: Option<String>,
}

impl fmt::Debug for CodexTokenData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodexTokenData")
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "account_id",
                &self.account_id.as_ref().map(|_| "<redacted>"),
            )
            .field("id_token", &self.id_token.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CodexAuthFile {
    pub auth_mode: Option<String>,
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    pub tokens: Option<CodexTokenData>,
    pub last_refresh: Option<String>,
}

impl fmt::Debug for CodexAuthFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodexAuthFile")
            .field("auth_mode", &self.auth_mode)
            .field(
                "openai_api_key",
                &self.openai_api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("tokens", &self.tokens)
            .field("last_refresh", &self.last_refresh)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum AuthMode {
    ChatGPT,
    ApiKey,
}

#[derive(Clone)]
pub struct AuthInfo {
    pub mode: AuthMode,
    pub access_token: String,
    pub account_id: Option<String>,
}

impl fmt::Debug for AuthInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthInfo")
            .field("mode", &self.mode)
            .field("access_token", &"<redacted>")
            .field(
                "account_id",
                &self.account_id.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

fn clean_secret(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn load_auth(path: &Path) -> Result<AuthInfo> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ProxyError::AuthError(format!("Failed to read auth file: {}", e)))?;

    let auth_file: CodexAuthFile = serde_json::from_str(&content)
        .map_err(|e| ProxyError::AuthError(format!("Failed to parse auth file: {}", e)))?;

    // 인증 우선순위: tokens.access_token → OPENAI_API_KEY
    if let Some(tokens) = &auth_file.tokens {
        if let Some(access_token) = clean_secret(tokens.access_token.as_deref()) {
            let account_id = clean_secret(tokens.account_id.as_deref());
            if account_id.is_none() {
                tracing::warn!(
                    "auth.json: account_id missing, ChatGPT-Account-ID header will be omitted"
                );
            }
            return Ok(AuthInfo {
                mode: AuthMode::ChatGPT,
                access_token,
                account_id,
            });
        }

        tracing::warn!(
            "auth.json: tokens.access_token is missing or empty; falling back to OPENAI_API_KEY if available"
        );
    }

    if let Some(api_key) = clean_secret(auth_file.openai_api_key.as_deref()) {
        return Ok(AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: api_key,
            account_id: None,
        });
    }

    Err(ProxyError::AuthError(
        "No valid auth credentials found in auth.json".to_string(),
    ))
}

pub fn get_auth_headers(auth: &AuthInfo) -> Vec<(String, String)> {
    let mut headers = vec![(
        "Authorization".to_string(),
        format!("Bearer {}", auth.access_token),
    )];

    if let (AuthMode::ChatGPT, Some(account_id)) = (&auth.mode, &auth.account_id) {
        if let Some(account_id) = clean_secret(Some(account_id)) {
            headers.push(("ChatGPT-Account-ID".to_string(), account_id));
        }
    }

    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_auth_chatgpt_mode() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"{{"tokens": {{"access_token": "eyJ_test", "account_id": "uuid-123"}}}}"#
        )
        .unwrap();

        let auth = load_auth(file.path()).unwrap();
        assert!(matches!(auth.mode, AuthMode::ChatGPT));
        assert_eq!(auth.access_token, "eyJ_test");
        assert_eq!(auth.account_id, Some("uuid-123".to_string()));
    }

    #[test]
    fn test_load_auth_api_key_mode() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, r#"{{"OPENAI_API_KEY": "sk-proj-test"}}"#).unwrap();

        let auth = load_auth(file.path()).unwrap();
        assert!(matches!(auth.mode, AuthMode::ApiKey));
        assert_eq!(auth.access_token, "sk-proj-test");
        assert!(auth.account_id.is_none());
    }

    #[test]
    fn test_tokens_without_access_token_falls_back_to_api_key() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"{{"tokens": {{"account_id": "uuid-123"}}, "OPENAI_API_KEY": "sk-proj-test"}}"#
        )
        .unwrap();

        let auth = load_auth(file.path()).unwrap();
        assert!(matches!(auth.mode, AuthMode::ApiKey));
        assert_eq!(auth.access_token, "sk-proj-test");
    }

    #[test]
    fn test_auth_debug_redacts_sensitive_fields() {
        let auth_file = CodexAuthFile {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: Some("sk-secret".to_string()),
            tokens: Some(CodexTokenData {
                access_token: Some("access-secret".to_string()),
                refresh_token: Some("refresh-secret".to_string()),
                account_id: Some("account-secret".to_string()),
                id_token: Some("id-secret".to_string()),
            }),
            last_refresh: None,
        };

        let debug = format!("{auth_file:?}");
        assert!(!debug.contains("sk-secret"));
        assert!(!debug.contains("access-secret"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn test_get_auth_headers_chatgpt() {
        let auth = AuthInfo {
            mode: AuthMode::ChatGPT,
            access_token: "token123".to_string(),
            account_id: Some("acc-456".to_string()),
        };

        let headers = get_auth_headers(&auth);
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer token123"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "ChatGPT-Account-ID" && v == "acc-456"));
    }

    #[test]
    fn test_get_auth_headers_api_key() {
        let auth = AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: "sk-test".to_string(),
            account_id: None,
        };

        let headers = get_auth_headers(&auth);
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer sk-test"));
        assert!(!headers.iter().any(|(k, _)| k == "ChatGPT-Account-ID"));
    }
}
