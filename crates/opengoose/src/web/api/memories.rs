use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use opengoose_board::Memory;
use opengoose_board::memory::MemoryScope;
use serde::Deserialize;

use super::AppState;

pub async fn memories_list(State(state): State<AppState>) -> Result<Json<Vec<Memory>>, StatusCode> {
    state
        .board
        .list_memories(None)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
pub struct PromoteRequest {
    pub scope: String,
}

pub async fn memories_promote(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<PromoteRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let scope = MemoryScope::parse(&body.scope).ok_or((
        StatusCode::BAD_REQUEST,
        format!("invalid scope: {}", body.scope),
    ))?;
    state
        .board
        .promote_memory(id, scope)
        .await
        .map(|_| StatusCode::OK)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Promote failed: {e}")))
}
