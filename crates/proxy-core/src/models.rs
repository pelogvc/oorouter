use std::path::PathBuf;

use chrono::{SecondsFormat, Utc};
use serde::Deserialize;

use crate::types::codex::CodexModel;
use crate::types::ollama::{OllamaModelDetails, OllamaModelInfo};

const FALLBACK_CODEX_CLIENT_VERSION: &str = "0.153.0";

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

fn version_numbers(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.split('-').next()?.split('.');
    let numbers = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    parts.next().is_none().then_some(numbers)
}

pub async fn codex_client_version() -> String {
    if let Ok(version) = std::env::var("CODEX_VERSION") {
        if !version.trim().is_empty() {
            return version.trim().to_string();
        }
    }

    cached_codex_client_version()
        .await
        .unwrap_or_else(|| FALLBACK_CODEX_CLIENT_VERSION.to_string())
}

async fn cached_codex_client_version() -> Option<String> {
    #[derive(Deserialize)]
    struct CacheVersion {
        client_version: String,
    }

    let codex_home = std::env::var_os("CODEX_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".codex"))
        })?;
    let contents = tokio::fs::read(codex_home.join("models_cache.json"))
        .await
        .ok()?;
    let cache: CacheVersion = serde_json::from_slice(&contents).ok()?;
    let version = version_numbers(&cache.client_version)?;
    let minimum = version_numbers(FALLBACK_CODEX_CLIENT_VERSION)?;
    (version >= minimum).then_some(cache.client_version)
}
