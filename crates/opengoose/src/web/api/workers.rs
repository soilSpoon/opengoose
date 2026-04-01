use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use super::AppState;
use crate::worker_pool::{WorkerConfig, WorkerInfo};

#[derive(Deserialize)]
pub struct CreateWorkerRequest {
    pub id: Option<String>,
    pub recipe: Option<String>,
    pub model: Option<String>,
}

#[derive(serde::Serialize)]
pub struct WorkerResponse {
    pub id: String,
    pub status: &'static str,
}

pub async fn workers_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkerInfo>>, StatusCode> {
    Ok(Json(state.workers.list().await))
}

pub async fn workers_create(
    State(state): State<AppState>,
    Json(body): Json<CreateWorkerRequest>,
) -> Result<(StatusCode, Json<WorkerResponse>), (StatusCode, String)> {
    let config = WorkerConfig {
        recipe: body.recipe,
        model: body.model,
    };
    match state.workers.spawn(body.id, config).await {
        Ok(id) => Ok((
            StatusCode::CREATED,
            Json(WorkerResponse {
                id,
                status: "running",
            }),
        )),
        Err(e) => Err((StatusCode::BAD_REQUEST, format!("Failed: {e}"))),
    }
}

pub async fn workers_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<WorkerResponse>, (StatusCode, String)> {
    match state.workers.remove(&id).await {
        Ok(()) => Ok(Json(WorkerResponse {
            id,
            status: "stopped",
        })),
        Err(e) => Err((StatusCode::NOT_FOUND, format!("Failed: {e}"))),
    }
}
