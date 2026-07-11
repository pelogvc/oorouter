// Ported from: src/plugins/ollama.ts (.get("/tags", ...))

use crate::models::get_visible_models;
use crate::types::ollama::OllamaTagsResponse;
use axum::Json;

pub async fn get_tags() -> Json<OllamaTagsResponse> {
    let models = get_visible_models();
    Json(OllamaTagsResponse { models })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_tags_returns_visible_models() {
        let Json(resp) = get_tags().await;
        assert_eq!(resp.models.len(), 9, "Should return 9 visible models");
        let names: Vec<&str> = resp
            .models
            .iter()
            .map(|model| model.name.as_str())
            .collect();
        assert_eq!(
            &names[..3],
            &[
                "gpt-5.6-sol:latest",
                "gpt-5.6-terra:latest",
                "gpt-5.6-luna:latest",
            ]
        );
        for model in &resp.models {
            assert!(model.name.ends_with(":latest"));
        }
    }
}
