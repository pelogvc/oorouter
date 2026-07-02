use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use super::AppState;
use crate::logger::{get_recent_logs, LogEntry};

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    limit: Option<usize>,
}

pub async fn get_logs(
    State(state): State<AppState>,
    Query(query): Query<LogsQuery>,
) -> Json<Vec<LogEntry>> {
    Json(get_recent_logs(
        &state.log_buffer,
        query.limit.unwrap_or(100).clamp(1, 500),
    ))
}
