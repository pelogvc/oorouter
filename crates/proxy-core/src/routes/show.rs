use std::collections::HashMap;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use super::{map_proxy_error, AppState, RouteResult};
use crate::converter::resolve_model;
use crate::models::create_model_details;
use crate::types::ollama::{OllamaShowRequest, OllamaShowResponse};

pub async fn post_show(
    State(state): State<AppState>,
    Json(body): Json<OllamaShowRequest>,
) -> RouteResult {
    let requested_model = body.name.trim_end_matches(":latest");
    let model_name = resolve_model(requested_model);
    let models = state.client.fetch_models().await.map_err(map_proxy_error)?;
    let model = models
        .iter()
        .find(|model| model.slug == model_name)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("model '{}' not found", requested_model) })),
            )
        })?;

    let mut model_info = HashMap::new();
    model_info.insert("general.architecture".to_string(), json!("gpt"));
    model_info.insert("general.basename".to_string(), json!(model_name));
    model_info.insert(
        "gpt.context_length".to_string(),
        json!(model.context_window),
    );

    Ok(Json(OllamaShowResponse {
        modelfile: format!("FROM {}", model_name),
        parameters: String::new(),
        template: "{{ .Prompt }}".to_string(),
        details: create_model_details(),
        model_info,
        capabilities: model.capabilities(),
    })
    .into_response())
}
