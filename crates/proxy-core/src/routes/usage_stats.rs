use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::AppState;

#[derive(Debug, Deserialize)]
pub struct TokenUsageQuery {
    days: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenUsageDto {
    pub date: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub request_count: i64,
}

pub async fn get_token_usage(
    State(state): State<AppState>,
    Query(query): Query<TokenUsageQuery>,
) -> Result<Json<Vec<TokenUsageDto>>, (StatusCode, Json<serde_json::Value>)> {
    let days = query.days.unwrap_or(7);
    if !(1..=3650).contains(&days) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("days must be between 1 and 3650: {days}")
            })),
        ));
    }

    let rows = state
        .db
        .get_token_usage_summary(i64::from(days))
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": error.to_string()
                })),
            )
        })?;

    Ok(Json(
        rows.into_iter()
            .map(|row| TokenUsageDto {
                date: row.date,
                model: row.model,
                input_tokens: row.input_tokens,
                output_tokens: row.output_tokens,
                total_tokens: row.total_tokens,
                request_count: row.request_count,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        auth::{AuthInfo, AuthMode},
        client::CodexClient,
        db::Database,
        logger::new_log_buffer,
    };

    fn test_client() -> Arc<CodexClient> {
        Arc::new(CodexClient::new(
            AuthInfo {
                mode: AuthMode::ApiKey,
                access_token: "test-key".to_string(),
                account_id: None,
            },
            "https://example.test/backend-api/codex/responses".to_string(),
        ))
    }

    #[tokio::test]
    async fn test_get_token_usage_returns_summary() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = Arc::new(Database::new(&dir.path().join("test.db")).await.unwrap());
        db.insert_token_usage("gpt-5.5", "codex", 10, 5, "/v1/chat/completions")
            .await
            .unwrap();

        let state = AppState {
            client: test_client(),
            db,
            log_buffer: new_log_buffer(),
        };

        let Json(rows) = get_token_usage(State(state), Query(TokenUsageQuery { days: Some(7) }))
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "gpt-5.5");
        assert_eq!(rows[0].input_tokens, 10);
        assert_eq!(rows[0].output_tokens, 5);
        assert_eq!(rows[0].request_count, 1);
    }

    #[tokio::test]
    async fn test_get_token_usage_rejects_invalid_days() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = Arc::new(Database::new(&dir.path().join("test.db")).await.unwrap());
        let state = AppState {
            client: test_client(),
            db,
            log_buffer: new_log_buffer(),
        };

        let (status, _) = get_token_usage(State(state), Query(TokenUsageQuery { days: Some(0) }))
            .await
            .unwrap_err();

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
