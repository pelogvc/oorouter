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

fn expand_tilde_with_home(path: &str, home: Option<&str>) -> Result<PathBuf> {
    if path == "~" {
        let home = home.ok_or_else(|| {
            ProxyError::ConfigError("AUTH_PATH uses '~' but HOME is not set".to_string())
        })?;
        Ok(PathBuf::from(home))
    } else if let Some(rest) = path.strip_prefix("~/") {
        let home = home.ok_or_else(|| {
            ProxyError::ConfigError("AUTH_PATH uses '~/' but HOME is not set".to_string())
        })?;
        Ok(PathBuf::from(home).join(rest))
    } else {
        Ok(PathBuf::from(path))
    }
}

fn non_empty_env_value(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

pub fn expand_path(path: &str) -> Result<PathBuf> {
    let home = non_empty_env_value(std::env::var("HOME").ok())
        .or_else(|| non_empty_env_value(std::env::var("USERPROFILE").ok()));
    expand_tilde_with_home(path, home.as_deref())
}

fn parse_log_level(value: Option<&str>) -> LogLevel {
    match value.map(|s| s.to_lowercase()).as_deref() {
        Some("debug") => LogLevel::Debug,
        Some("warn") => LogLevel::Warn,
        Some("error") => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

fn parse_backend(value: Option<&str>) -> Result<BackendType> {
    match value.map(|s| s.to_lowercase()).as_deref() {
        Some("codex") | None => Ok(BackendType::Codex),
        Some(value) => Err(ProxyError::ConfigError(format!(
            "Unsupported BACKEND: {}",
            value
        ))),
    }
}

pub fn load_config() -> Result<Config> {
    load_config_from_env(|key| std::env::var(key).ok())
}

fn is_loopback_http_url(url: &reqwest::Url) -> bool {
    url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
}

pub fn default_db_path() -> Result<PathBuf> {
    if let Some(data_home) = non_empty_env_value(std::env::var("XDG_DATA_HOME").ok()) {
        let data_home = PathBuf::from(data_home);
        if data_home.is_absolute() {
            return Ok(data_home.join("oorouter").join("proxy.db"));
        }
    }

    if let Some(home) = non_empty_env_value(std::env::var("HOME").ok()) {
        return Ok(PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("oorouter")
            .join("proxy.db"));
    }

    if let Some(profile) = non_empty_env_value(std::env::var("USERPROFILE").ok()) {
        return Ok(PathBuf::from(profile)
            .join("AppData")
            .join("Local")
            .join("oorouter")
            .join("proxy.db"));
    }

    Ok(std::env::temp_dir().join("oorouter").join("proxy.db"))
}

pub fn load_config_from_env<F>(get_env: F) -> Result<Config>
where
    F: Fn(&str) -> Option<String>,
{
    let port_str = get_env("PORT").unwrap_or_else(|| "11434".to_string());
    let port: u16 = port_str
        .parse()
        .map_err(|_| ProxyError::ConfigError(format!("Invalid PORT: {}", port_str)))?;
    if port == 0 {
        return Err(ProxyError::ConfigError(
            "PORT must be between 1 and 65535".to_string(),
        ));
    }

    let home = non_empty_env_value(get_env("HOME"))
        .or_else(|| non_empty_env_value(get_env("USERPROFILE")));
    let auth_path = if let Some(auth_path_str) = get_env("AUTH_PATH") {
        expand_tilde_with_home(&auth_path_str, home.as_deref())?
    } else if let Some(home) = home.as_deref() {
        PathBuf::from(home).join(".codex").join("auth.json")
    } else {
        PathBuf::from("/root").join(".codex").join("auth.json")
    };

    let log_level = parse_log_level(get_env("LOG_LEVEL").as_deref());
    let chatgpt_api_url = get_env("CHATGPT_API_URL")
        .unwrap_or_else(|| "https://chatgpt.com/backend-api/codex/responses".to_string());
    let parsed_url = reqwest::Url::parse(&chatgpt_api_url)
        .map_err(|e| ProxyError::ConfigError(format!("Invalid CHATGPT_API_URL: {}", e)))?;
    if parsed_url.scheme() != "https" && !is_loopback_http_url(&parsed_url) {
        return Err(ProxyError::ConfigError(format!(
            "CHATGPT_API_URL must use https or loopback http: {}",
            chatgpt_api_url
        )));
    }
    let backend = parse_backend(get_env("BACKEND").as_deref())?;

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
        let config = load_config_from_env(|key| match key {
            "HOME" => Some("/Users/test".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(config.port, 11434);
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(
            config.chatgpt_api_url,
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(config.backend, BackendType::Codex);
    }

    #[test]
    fn test_custom_port() {
        let config = load_config_from_env(|key| match key {
            "PORT" => Some("8080".to_string()),
            "HOME" => Some("/Users/test".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_invalid_port() {
        let result = load_config_from_env(|key| {
            if key == "PORT" {
                Some("invalid".to_string())
            } else {
                None
            }
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_port_is_invalid() {
        let result = load_config_from_env(|key| {
            if key == "PORT" {
                Some("0".to_string())
            } else {
                None
            }
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_tilde_expansion() {
        let config = load_config_from_env(|key| match key {
            "AUTH_PATH" => Some("~/.codex/auth.json".to_string()),
            "HOME" => Some("/Users/test".to_string()),
            _ => None,
        })
        .unwrap();
        assert!(!config.auth_path.to_string_lossy().contains('~'));
        assert!(config
            .auth_path
            .to_string_lossy()
            .contains(".codex/auth.json"));
    }

    #[test]
    fn test_tilde_only_expansion() {
        let config = load_config_from_env(|key| match key {
            "AUTH_PATH" => Some("~".to_string()),
            "HOME" => Some("/Users/test".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(config.auth_path, PathBuf::from("/Users/test"));
    }

    #[test]
    fn test_tilde_expansion_uses_userprofile_when_home_is_empty() {
        let config = load_config_from_env(|key| match key {
            "AUTH_PATH" => Some("~/auth.json".to_string()),
            "HOME" => Some("".to_string()),
            "USERPROFILE" => Some("/Users/fallback".to_string()),
            _ => None,
        })
        .unwrap();

        assert_eq!(config.auth_path, PathBuf::from("/Users/fallback/auth.json"));
    }

    #[test]
    fn test_invalid_backend() {
        let result = load_config_from_env(|key| match key {
            "BACKEND" => Some("other".to_string()),
            "HOME" => Some("/Users/test".to_string()),
            _ => None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_log_level_parsing() {
        let config = load_config_from_env(|key| match key {
            "LOG_LEVEL" => Some("debug".to_string()),
            "HOME" => Some("/Users/test".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(config.log_level, LogLevel::Debug);
    }

    #[test]
    fn test_rejects_non_loopback_http_backend_url() {
        let result = load_config_from_env(|key| match key {
            "CHATGPT_API_URL" => Some("http://example.com/backend-api/codex/responses".to_string()),
            "HOME" => Some("/Users/test".to_string()),
            _ => None,
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_allows_loopback_http_backend_url() {
        let config = load_config_from_env(|key| match key {
            "CHATGPT_API_URL" => {
                Some("http://127.0.0.1:3000/backend-api/codex/responses".to_string())
            }
            "HOME" => Some("/Users/test".to_string()),
            _ => None,
        })
        .unwrap();

        assert_eq!(
            config.chatgpt_api_url,
            "http://127.0.0.1:3000/backend-api/codex/responses"
        );
    }
}
