// Ported from: src/plugins/ollama.ts (embed, ps, version, copy, delete, pull, push stubs)

use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use chrono::{Duration, SecondsFormat, Utc};

use crate::types::ollama::{OllamaEmbedRequest, OllamaPsResponse, OllamaVersionResponse, OllamaPsModel};

pub async fn post_embed(
    Json(body): Json<OllamaEmbedRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "Embedding is not supported by this proxy",
            "embeddings": [],
            "model": body.model,
            "total_duration": 0
        })),
    )
}

pub async fn get_ps() -> Json<OllamaPsResponse> {
    use crate::models::get_visible_models;

    let now = Utc::now();
    let now_str = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let expires_at = (now + Duration::minutes(5))
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let models = get_visible_models()
        .into_iter()
        .map(|m| OllamaPsModel {
            name: m.name.clone(),
            model: m.model.clone(),
            modified_at: now_str.clone(),
            size: m.size,
            digest: m.digest.clone(),
            details: m.details.clone(),
            expires_at: expires_at.clone(),
            size_vram: 0,
        })
        .collect();

    Json(OllamaPsResponse { models })
}

pub async fn get_version() -> Json<OllamaVersionResponse> {
    Json(OllamaVersionResponse {
        version: "0.17.4".to_string(),
    })
}

pub async fn post_copy() -> StatusCode {
    StatusCode::OK
}

pub async fn delete_model() -> StatusCode {
    StatusCode::OK
}

pub async fn post_pull() -> Json<serde_json::Value> {
    Json(json!({ "status": "success" }))
}

pub async fn post_push() -> Json<serde_json::Value> {
    Json(json!({ "status": "success" }))
}

pub async fn health() -> &'static str {
    "Ollama is running"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embed_returns_501() {
        let req = OllamaEmbedRequest {
            model: "test".to_string(),
            input: crate::types::ollama::OllamaEmbedInput::Single("hello".to_string()),
        };
        let (status, Json(body)) = post_embed(Json(req)).await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(body["model"], "test");
        assert!(body["embeddings"].as_array().map_or(false, |a| a.is_empty()));
    }

    #[tokio::test]
    async fn test_ps_returns_models() {
        let Json(resp) = get_ps().await;
        // visible 모델이 있어야 함
        assert!(!resp.models.is_empty());
        // expires_at, size_vram 필드 확인
        let m = &resp.models[0];
        assert_eq!(m.size_vram, 0);
        assert!(m.expires_at.contains('T'));
        assert!(m.modified_at.contains('T'));
    }

    #[tokio::test]
    async fn test_version() {
        let Json(resp) = get_version().await;
        assert_eq!(resp.version, "0.17.4");
    }

    #[tokio::test]
    async fn test_health() {
        let result = health().await;
        assert_eq!(result, "Ollama is running");
    }

    #[tokio::test]
    async fn test_copy_returns_ok() {
        let status = post_copy().await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_returns_ok() {
        let status = delete_model().await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_pull_returns_success() {
        let Json(body) = post_pull().await;
        assert_eq!(body["status"], "success");
    }

    #[tokio::test]
    async fn test_push_returns_success() {
        let Json(body) = post_push().await;
        assert_eq!(body["status"], "success");
    }
}
