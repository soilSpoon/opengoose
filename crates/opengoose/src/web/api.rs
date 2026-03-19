use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use opengoose_board::entity::stamp;
use opengoose_board::stamps::Severity;
use opengoose_board::{PostWorkItem, Priority, RigId, Status};
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::ColumnTrait;
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

#[derive(Serialize)]
pub struct DimensionScore {
    quality: f32,
    reliability: f32,
    helpfulness: f32,
}

#[derive(Serialize)]
pub struct StampInfo {
    work_item_id: i64,
    dimension: String,
    score: f32,
    severity: String,
    stamped_by: String,
    timestamp: String,
}

#[derive(Serialize)]
pub struct RigDetail {
    id: String,
    rig_type: String,
    recipe: Option<String>,
    tags: Option<String>,
    trust_level: String,
    trust_score: f32,
    dimensions: DimensionScore,
    stamps: Vec<StampInfo>,
    completed_items: Vec<opengoose_board::WorkItem>,
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
    if body.title.is_empty() || body.title.len() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.description.len() > 10_000 {
        return Err(StatusCode::BAD_REQUEST);
    }
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

pub async fn rig_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RigDetail>, StatusCode> {
    let rig = state
        .board
        .get_rig(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let trust_score = state.board.weighted_score(&id).await.unwrap_or(0.0);
    let trust_level = state.board.trust_level(&id).await.unwrap_or("L1").to_string();

    // Fetch stamps for per-dimension scores
    let stamps = stamp::Entity::find()
        .filter(stamp::Column::TargetRig.eq(&id))
        .all(state.board.db())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let now = chrono::Utc::now();
    let mut q_score = 0.0_f32;
    let mut r_score = 0.0_f32;
    let mut h_score = 0.0_f32;
    let mut stamp_infos = Vec::with_capacity(stamps.len());

    for s in &stamps {
        let days = (now - s.timestamp).num_seconds() as f32 / 86400.0;
        let decay = 0.5_f32.powf(days / 30.0);
        let weight = Severity::parse(&s.severity).unwrap_or(Severity::Leaf).weight();
        let weighted = weight * s.score * decay;
        match s.dimension.as_str() {
            "Quality" => q_score += weighted,
            "Reliability" => r_score += weighted,
            "Helpfulness" => h_score += weighted,
            _ => {}
        }
        stamp_infos.push(StampInfo {
            work_item_id: s.work_item_id,
            dimension: s.dimension.clone(),
            score: s.score,
            severity: s.severity.clone(),
            stamped_by: s.stamped_by.clone(),
            timestamp: s.timestamp.to_rfc3339(),
        });
    }

    // Completed items by this rig
    let all_items = state.board.list().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let completed_items: Vec<_> = all_items
        .into_iter()
        .filter(|i| i.status == Status::Done && i.claimed_by.as_ref().is_some_and(|r| r.0 == id))
        .collect();

    Ok(Json(RigDetail {
        id: rig.id,
        rig_type: rig.rig_type,
        recipe: rig.recipe,
        tags: rig.tags,
        trust_level,
        trust_score,
        dimensions: DimensionScore {
            quality: q_score,
            reliability: r_score,
            helpfulness: h_score,
        },
        stamps: stamp_infos,
        completed_items,
    }))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use opengoose_board::db_board::DbBoard;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    use super::*;

    fn test_app(board: Arc<DbBoard>) -> axum::Router {
        let (tx, _) = broadcast::channel::<()>(64);
        let state = AppState { board, tx };
        axum::Router::new()
            .route("/api/board", axum::routing::get(board_list).post(board_create))
            .route("/api/board/{id}", axum::routing::get(board_get))
            .route("/api/board/{id}/claim", axum::routing::post(board_claim))
            .route("/api/rigs", axum::routing::get(rigs_list))
            .route("/api/rigs/{id}", axum::routing::get(rig_detail))
            .with_state(state)
    }

    async fn new_board() -> Arc<DbBoard> {
        Arc::new(DbBoard::in_memory().await.unwrap())
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn board_list_empty() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(Request::get("/api/board").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn board_list_returns_posted_items() {
        let board = new_board().await;
        board.post(PostWorkItem {
            title: "Task A".into(),
            description: String::new(),
            created_by: RigId::new("test"),
            priority: Priority::P1,
            tags: vec![],
        }).await.unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(Request::get("/api/board").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let json = body_json(resp).await;
        let items = json.as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["title"], "Task A");
        assert_eq!(items[0]["status"], "Open");
    }

    #[tokio::test]
    async fn board_get_existing() {
        let board = new_board().await;
        let item = board.post(PostWorkItem {
            title: "Find me".into(),
            description: String::new(),
            created_by: RigId::new("test"),
            priority: Priority::P0,
            tags: vec![],
        }).await.unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(Request::get(&format!("/api/board/{}", item.id)).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["title"], "Find me");
        assert_eq!(json["priority"], "P0");
    }

    #[tokio::test]
    async fn board_get_not_found() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(Request::get("/api/board/999").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
