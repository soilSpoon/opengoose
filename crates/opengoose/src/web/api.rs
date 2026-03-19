use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use opengoose_board::{PostWorkItem, Priority, RigId};
use serde::{Deserialize, Serialize};

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

#[derive(Deserialize)]
pub struct CreateItem {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_rig")]
    pub created_by: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_rig() -> String {
    "web".into()
}

pub async fn board_create(
    State(state): State<AppState>,
    Json(body): Json<CreateItem>,
) -> Result<Json<opengoose_board::WorkItem>, StatusCode> {
    let priority = match body.priority.as_str() {
        "P0" => Priority::P0,
        "P2" => Priority::P2,
        _ => Priority::P1,
    };
    let item = state
        .board
        .post(PostWorkItem {
            title: body.title,
            description: body.description,
            created_by: RigId::new(body.created_by),
            priority,
            tags: body.tags,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(item))
}

#[derive(Deserialize)]
pub struct ClaimBody {
    pub rig_id: String,
}

pub async fn board_claim(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ClaimBody>,
) -> Result<Json<opengoose_board::WorkItem>, StatusCode> {
    let item = state
        .board
        .claim(id, &RigId::new(body.rig_id))
        .await
        .map_err(|e| match e {
            opengoose_board::BoardError::NotFound(_) => StatusCode::NOT_FOUND,
            opengoose_board::BoardError::AlreadyClaimed { .. } => StatusCode::CONFLICT,
            _ => StatusCode::BAD_REQUEST,
        })?;
    Ok(Json(item))
}
