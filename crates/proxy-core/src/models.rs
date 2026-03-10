// Model registry and alias resolution
// Ported from: src/providers/codex/models.ts

use crate::types::ollama::{OllamaModelInfo, OllamaModelDetails};
use chrono::{SecondsFormat, Utc};

#[derive(Debug, Clone)]
pub struct ModelDefinition {
    pub slug: &'static str,
    pub name: &'static str,
    pub visible: bool,
    pub context_length: u64,
    pub supports_vision: bool,
}

const AVAILABLE_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        slug: "gpt-5.4",
        name: "gpt-5.4",
        visible: true,
        context_length: 1_050_000,
        supports_vision: true,
    },
    ModelDefinition {
        slug: "gpt-5.3-codex",
        name: "gpt-5.3-codex",
        visible: true,
        context_length: 400_000,
        supports_vision: true,
    },
    ModelDefinition {
        slug: "gpt-5.2-codex",
        name: "gpt-5.2-codex",
        visible: true,
        context_length: 400_000,
        supports_vision: true,
    },
    ModelDefinition {
        slug: "gpt-5.2",
        name: "gpt-5.2",
        visible: true,
        context_length: 400_000,
        supports_vision: true,
    },
    ModelDefinition {
        slug: "gpt-5.3-codex-spark",
        name: "gpt-5.3-codex-spark",
        visible: true,
        context_length: 128_000,
        supports_vision: false,
    },
    ModelDefinition {
        slug: "gpt-5.4-pro",
        name: "gpt-5.4-pro",
        visible: false,
        context_length: 1_050_000,
        supports_vision: true,
    },
    ModelDefinition {
        slug: "gpt-5-codex",
        name: "gpt-5-codex",
        visible: false,
        context_length: 400_000,
        supports_vision: true,
    },
    ModelDefinition {
        slug: "gpt-5",
        name: "gpt-5",
        visible: false,
        context_length: 400_000,
        supports_vision: true,
    },
    ModelDefinition {
        slug: "gpt-5-codex-mini",
        name: "gpt-5-codex-mini",
        visible: false,
        context_length: 400_000,
        supports_vision: false,
    },
];

fn create_model_details() -> OllamaModelDetails {
    OllamaModelDetails {
        parent_model: String::new(),
        format: "api".to_string(),
        family: "gpt".to_string(),
        families: vec!["gpt".to_string()],
        parameter_size: "unknown".to_string(),
        quantization_level: "none".to_string(),
    }
}

fn to_ollama_model_info(model: &ModelDefinition) -> OllamaModelInfo {
    OllamaModelInfo {
        name: format!("{}:latest", model.slug),
        model: format!("{}:latest", model.slug),
        modified_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        size: 0,
        digest: format!("sha256:{}", "0".repeat(64)),
        details: create_model_details(),
    }
}

pub fn get_visible_models() -> Vec<OllamaModelInfo> {
    AVAILABLE_MODELS
        .iter()
        .filter(|m| m.visible)
        .map(to_ollama_model_info)
        .collect()
}

pub fn get_all_models() -> Vec<OllamaModelInfo> {
    AVAILABLE_MODELS
        .iter()
        .map(to_ollama_model_info)
        .collect()
}

pub fn model_exists(name: &str) -> bool {
    let slug = name.trim_end_matches(":latest");
    AVAILABLE_MODELS.iter().any(|m| m.slug == slug)
}

pub fn get_context_length(name: &str) -> u64 {
    let slug = name.trim_end_matches(":latest");
    AVAILABLE_MODELS
        .iter()
        .find(|m| m.slug == slug)
        .map(|m| m.context_length)
        .unwrap_or(400_000)
}

pub fn get_capabilities(name: &str) -> Vec<String> {
    let slug = name.trim_end_matches(":latest");
    let model = AVAILABLE_MODELS.iter().find(|m| m.slug == slug);
    let mut caps = vec!["completion".to_string(), "tools".to_string()];
    if model.map(|m| m.supports_vision).unwrap_or(true) {
        caps.push("vision".to_string());
    }
    caps
}

pub fn get_model_definition(name: &str) -> Option<&'static ModelDefinition> {
    let slug = name.trim_end_matches(":latest");
    AVAILABLE_MODELS.iter().find(|m| m.slug == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_models_count() {
        assert_eq!(AVAILABLE_MODELS.len(), 9);
    }

    #[test]
    fn test_visible_models() {
        let visible = get_visible_models();
        assert_eq!(visible.len(), 5);
    }

    #[test]
    fn test_model_exists() {
        assert!(model_exists("gpt-5.3-codex"));
        assert!(model_exists("gpt-5.3-codex:latest"));
        assert!(!model_exists("nonexistent-model"));
    }

    #[test]
    fn test_get_context_length() {
        assert_eq!(get_context_length("gpt-5.4"), 1_050_000);
        assert_eq!(get_context_length("gpt-5.3-codex-spark"), 128_000);
        assert_eq!(get_context_length("unknown"), 400_000);
    }

    #[test]
    fn test_get_capabilities_vision() {
        let caps = get_capabilities("gpt-5.3-codex");
        assert!(caps.contains(&"vision".to_string()));
        assert!(caps.contains(&"completion".to_string()));
        assert!(caps.contains(&"tools".to_string()));
    }

    #[test]
    fn test_get_capabilities_no_vision() {
        let caps = get_capabilities("gpt-5.3-codex-spark");
        assert!(!caps.contains(&"vision".to_string()));
    }

    #[test]
    fn test_ollama_model_info_format() {
        let models = get_visible_models();
        for model in &models {
            assert!(model.name.ends_with(":latest"));
            assert!(model.digest.starts_with("sha256:"));
        }
    }
}
