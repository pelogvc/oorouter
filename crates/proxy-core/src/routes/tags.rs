use axum::{extract::State, response::IntoResponse, Json};

use super::{map_proxy_error, AppState, RouteResult};
use crate::types::ollama::OllamaTagsResponse;

pub async fn get_tags(State(state): State<AppState>) -> RouteResult {
    let models = state.client.fetch_models().await.map_err(map_proxy_error)?;
    Ok(Json(OllamaTagsResponse {
        models: models
            .iter()
            .filter(|model| model.is_visible())
            .map(|model| model.to_ollama_model_info())
            .collect(),
    })
    .into_response())
}
