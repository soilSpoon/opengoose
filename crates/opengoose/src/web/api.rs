use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use super::AppState;

#[derive(Serialize)]
pub struct RigInfo {
    id: String,
    rig_type: String,
    recipe: Option<String>,
    tags: Option<String>,
    trust_level: String,
    trust_score: f32,
}

pub async fn board_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<opengoose_board::WorkItem>>, StatusCode> {
    let items = state.board.list().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(items))
}

pub async fn board_get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<opengoose_board::WorkItem>, StatusCode> {
    let item = state
        .board
        .get(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(item))
}

pub async fn rigs_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<RigInfo>>, StatusCode> {
    let rigs = state.board.list_rigs().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut result = Vec::with_capacity(rigs.len());
    for rig in rigs {
        let trust_score = state.board.weighted_score(&rig.id).await.unwrap_or(0.0);
        let trust_level = state
            .board
            .trust_level(&rig.id)
            .await
            .unwrap_or("L1")
            .to_string();
        result.push(RigInfo {
            id: rig.id,
            rig_type: rig.rig_type,
            recipe: rig.recipe,
            tags: rig.tags,
            trust_level,
            trust_score,
        });
    }
    Ok(Json(result))
}
