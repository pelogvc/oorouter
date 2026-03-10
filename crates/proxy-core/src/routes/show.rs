// Ported from: src/plugins/ollama.ts (.post("/show", ...))

use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use std::collections::HashMap;

use crate::models::{get_capabilities, get_context_length, model_exists};
use crate::types::ollama::{OllamaModelDetails, OllamaShowRequest, OllamaShowResponse};

pub async fn post_show(
    Json(body): Json<OllamaShowRequest>,
) -> Result<Json<OllamaShowResponse>, (StatusCode, Json<serde_json::Value>)> {
    let model_name = body.name.trim_end_matches(":latest");

    if !model_exists(model_name) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("model '{}' not found", model_name) })),
        ));
    }

    let mut model_info = HashMap::new();
    model_info.insert("general.architecture".to_string(), json!("gpt"));
    model_info.insert("general.basename".to_string(), json!(model_name));
    model_info.insert(
        "gpt.context_length".to_string(),
        json!(get_context_length(model_name)),
    );

    Ok(Json(OllamaShowResponse {
        modelfile: format!("FROM {}", model_name),
        parameters: String::new(),
        template: "{{ .Prompt }}".to_string(),
        details: OllamaModelDetails {
            parent_model: String::new(),
            format: "api".to_string(),
            family: "gpt".to_string(),
            families: vec!["gpt".to_string()],
            parameter_size: "unknown".to_string(),
            quantization_level: "none".to_string(),
        },
        model_info,
        capabilities: get_capabilities(model_name),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_show_existing_model() {
        let req = OllamaShowRequest {
            name: "gpt-5.3-codex".to_string(),
        };
        let result = post_show(Json(req)).await;
        assert!(result.is_ok());
        let Json(resp) = result.unwrap();
        assert_eq!(resp.modelfile, "FROM gpt-5.3-codex");
        assert!(resp.capabilities.contains(&"completion".to_string()));
        assert!(resp.capabilities.contains(&"vision".to_string()));
    }

    #[tokio::test]
    async fn test_show_existing_model_with_latest_suffix() {
        let req = OllamaShowRequest {
            name: "gpt-5.3-codex:latest".to_string(),
        };
        let result = post_show(Json(req)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_show_nonexistent_model() {
        let req = OllamaShowRequest {
            name: "nonexistent-model".to_string(),
        };
        let result = post_show(Json(req)).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
