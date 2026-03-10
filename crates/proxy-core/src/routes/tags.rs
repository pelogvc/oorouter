// Ported from: src/plugins/ollama.ts (.get("/tags", ...))

use axum::Json;
use crate::models::get_visible_models;
use crate::types::ollama::OllamaTagsResponse;

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
        assert_eq!(resp.models.len(), 5, "Should return 5 visible models");
        for model in &resp.models {
            assert!(model.name.ends_with(":latest"));
        }
    }
}
