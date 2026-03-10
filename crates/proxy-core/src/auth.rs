use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::error::{ProxyError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexTokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    pub id_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAuthFile {
    pub auth_mode: Option<String>,
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    pub tokens: Option<CodexTokenData>,
    pub last_refresh: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuthMode {
    ChatGPT,
    ApiKey,
}

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub mode: AuthMode,
    pub access_token: String,
    pub account_id: Option<String>,
}

pub fn load_auth(path: &Path) -> Result<AuthInfo> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ProxyError::AuthError(format!("Failed to read auth file: {}", e)))?;
    
    let auth_file: CodexAuthFile = serde_json::from_str(&content)
        .map_err(|e| ProxyError::AuthError(format!("Failed to parse auth file: {}", e)))?;
    
    // 인증 우선순위: tokens.access_token → OPENAI_API_KEY
    if let Some(tokens) = &auth_file.tokens {
        if !tokens.access_token.is_empty() {
            if tokens.account_id.is_none() {
                tracing::warn!("auth.json: account_id missing, ChatGPT-Account-ID header will be omitted");
            }
            return Ok(AuthInfo {
                mode: AuthMode::ChatGPT,
                access_token: tokens.access_token.clone(),
                account_id: tokens.account_id.clone(),
            });
        }
    }
    
    if let Some(api_key) = &auth_file.openai_api_key {
        if !api_key.is_empty() {
            return Ok(AuthInfo {
                mode: AuthMode::ApiKey,
                access_token: api_key.clone(),
                account_id: None,
            });
        }
    }
    
    Err(ProxyError::AuthError("No valid auth credentials found in auth.json".to_string()))
}

pub fn get_auth_headers(auth: &AuthInfo) -> Vec<(String, String)> {
    let mut headers = vec![
        ("Authorization".to_string(), format!("Bearer {}", auth.access_token)),
    ];
    
    if let (AuthMode::ChatGPT, Some(account_id)) = (&auth.mode, &auth.account_id) {
        headers.push(("ChatGPT-Account-ID".to_string(), account_id.clone()));
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
        write!(file, r#"{{"tokens": {{"access_token": "eyJ_test", "account_id": "uuid-123"}}}}"#).unwrap();
        
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
    fn test_get_auth_headers_chatgpt() {
        let auth = AuthInfo {
            mode: AuthMode::ChatGPT,
            access_token: "token123".to_string(),
            account_id: Some("acc-456".to_string()),
        };
        
        let headers = get_auth_headers(&auth);
        assert!(headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer token123"));
        assert!(headers.iter().any(|(k, v)| k == "ChatGPT-Account-ID" && v == "acc-456"));
    }
    
    #[test]
    fn test_get_auth_headers_api_key() {
        let auth = AuthInfo {
            mode: AuthMode::ApiKey,
            access_token: "sk-test".to_string(),
            account_id: None,
        };
        
        let headers = get_auth_headers(&auth);
        assert!(headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer sk-test"));
        assert!(!headers.iter().any(|(k, _)| k == "ChatGPT-Account-ID"));
    }
}
