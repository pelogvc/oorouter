use std::{io::ErrorKind, path::PathBuf, process::Stdio, time::Duration};

use chrono::{SecondsFormat, Utc};
use tokio::process::Command;

use crate::error::{ProxyError, Result};

use crate::types::codex::CodexModel;
use crate::types::ollama::{OllamaModelDetails, OllamaModelInfo};

impl CodexModel {
    pub fn is_visible(&self) -> bool {
        matches!(self.visibility.as_deref(), None | Some("list"))
    }

    pub fn name(&self) -> &str {
        if self.display_name.is_empty() {
            &self.slug
        } else {
            &self.display_name
        }
    }

    pub fn supports_vision(&self) -> bool {
        self.input_modalities
            .iter()
            .any(|modality| modality == "image")
    }

    pub fn capabilities(&self) -> Vec<String> {
        let mut capabilities = vec!["completion".to_string(), "tools".to_string()];
        if self.supports_vision() {
            capabilities.push("vision".to_string());
        }
        capabilities
    }

    pub fn to_ollama_model_info(&self) -> OllamaModelInfo {
        OllamaModelInfo {
            name: format!("{}:latest", self.slug),
            model: format!("{}:latest", self.slug),
            modified_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            size: 0,
            digest: format!("sha256:{}", "0".repeat(64)),
            details: create_model_details(),
            context_length: self.context_window,
            supports_vision: self.supports_vision(),
        }
    }
}

pub fn create_model_details() -> OllamaModelDetails {
    OllamaModelDetails {
        parent_model: String::new(),
        format: "api".to_string(),
        family: "gpt".to_string(),
        families: vec!["gpt".to_string()],
        parameter_size: "unknown".to_string(),
        quantization_level: "none".to_string(),
    }
}

fn codex_executables() -> Vec<PathBuf> {
    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    let mut executables = vec![PathBuf::from(name)];
    if let Some(home) = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE"))
    {
        executables.push(PathBuf::from(home).join(".local").join("bin").join(name));
    }
    #[cfg(target_os = "macos")]
    executables.extend([
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
    ]);
    executables
}

fn parse_codex_version(stdout: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(stdout).ok()?;
    let version = text.trim().strip_prefix("codex-cli ")?.trim();
    if !version
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
    {
        return None;
    }
    let mut numbers = version.split(['-', '+']).next()?.split('.');
    for _ in 0..3 {
        numbers.next()?.parse::<u64>().ok()?;
    }
    numbers.next().is_none().then(|| version.to_string())
}

pub async fn codex_client_version() -> Result<String> {
    if let Ok(version) = std::env::var("CODEX_VERSION") {
        if !version.trim().is_empty() {
            return Ok(version.trim().to_string());
        }
    }

    for executable in codex_executables() {
        let mut command = Command::new(&executable);
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let output = match tokio::time::timeout(Duration::from_secs(2), command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) if error.kind() == ErrorKind::NotFound => continue,
            Ok(Err(error)) => {
                return Err(ProxyError::ConfigError(format!(
                    "Could not run Codex CLI at {}: {error}",
                    executable.display()
                )))
            }
            Err(_) => {
                return Err(ProxyError::ConfigError(
                    "Codex --version timed out".to_string(),
                ))
            }
        };
        if !output.status.success() {
            return Err(ProxyError::ConfigError(format!(
                "Codex --version failed at {} ({})",
                executable.display(),
                output.status
            )));
        }
        return parse_codex_version(&output.stdout).ok_or_else(|| {
            ProxyError::ConfigError("Codex --version returned an invalid version".to_string())
        });
    }

    Err(ProxyError::ConfigError(
        "Codex CLI was not found. Install Codex or set CODEX_VERSION explicitly.".to_string(),
    ))
}
