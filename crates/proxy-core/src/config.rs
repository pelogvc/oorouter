use std::path::PathBuf;
use crate::error::{ProxyError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum BackendType {
    Codex,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub auth_path: PathBuf,
    pub log_level: LogLevel,
    pub chatgpt_api_url: String,
    pub backend: BackendType,
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") || path == "~" {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        PathBuf::from(home).join(&path[2..])
    } else {
        PathBuf::from(path)
    }
}

fn parse_log_level(value: Option<&str>) -> LogLevel {
    match value.map(|s| s.to_lowercase()).as_deref() {
        Some("debug") => LogLevel::Debug,
        Some("warn") => LogLevel::Warn,
        Some("error") => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

fn parse_backend(value: Option<&str>) -> BackendType {
    match value.map(|s| s.to_lowercase()).as_deref() {
        Some("codex") | None => BackendType::Codex,
        _ => BackendType::Codex,
    }
}

pub fn load_config() -> Result<Config> {
    load_config_from_env(|key| std::env::var(key).ok())
}

pub fn load_config_from_env<F>(get_env: F) -> Result<Config>
where
    F: Fn(&str) -> Option<String>,
{
    let port_str = get_env("PORT").unwrap_or_else(|| "11434".to_string());
    let port: u16 = port_str.parse().map_err(|_| {
        ProxyError::ConfigError(format!("Invalid PORT: {}", port_str))
    })?;
    
    if port < 1 {
        return Err(ProxyError::ConfigError(format!("PORT out of range: {}", port)));
    }
    
    let auth_path_str = get_env("AUTH_PATH").unwrap_or_else(|| "~/.codex/auth.json".to_string());
    let auth_path = expand_tilde(&auth_path_str);
    
    let log_level = parse_log_level(get_env("LOG_LEVEL").as_deref());
    let chatgpt_api_url = get_env("CHATGPT_API_URL")
        .unwrap_or_else(|| "https://chatgpt.com/backend-api/codex/responses".to_string());
    let backend = parse_backend(get_env("BACKEND").as_deref());
    
    Ok(Config {
        port,
        auth_path,
        log_level,
        chatgpt_api_url,
        backend,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = load_config_from_env(|_| None).unwrap();
        assert_eq!(config.port, 11434);
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(config.chatgpt_api_url, "https://chatgpt.com/backend-api/codex/responses");
        assert_eq!(config.backend, BackendType::Codex);
    }
    
    #[test]
    fn test_custom_port() {
        let config = load_config_from_env(|key| {
            if key == "PORT" { Some("8080".to_string()) } else { None }
        }).unwrap();
        assert_eq!(config.port, 8080);
    }
    
    #[test]
    fn test_invalid_port() {
        let result = load_config_from_env(|key| {
            if key == "PORT" { Some("invalid".to_string()) } else { None }
        });
        assert!(result.is_err());
    }
    
    #[test]
    fn test_tilde_expansion() {
        let config = load_config_from_env(|key| {
            if key == "AUTH_PATH" { Some("~/.codex/auth.json".to_string()) } else { None }
        }).unwrap();
        assert!(!config.auth_path.to_string_lossy().contains('~'));
        assert!(config.auth_path.to_string_lossy().contains(".codex/auth.json"));
    }
    
    #[test]
    fn test_log_level_parsing() {
        let config = load_config_from_env(|key| {
            if key == "LOG_LEVEL" { Some("debug".to_string()) } else { None }
        }).unwrap();
        assert_eq!(config.log_level, LogLevel::Debug);
    }
}
