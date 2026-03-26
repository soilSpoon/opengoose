use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
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

pub async fn rigs_list(State(state): State<AppState>) -> Result<Json<Vec<RigInfo>>, StatusCode> {
    let rigs = state
        .board
        .list_rigs()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let scores = state
        .board
        .batch_rig_scores()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let result = rigs
        .into_iter()
        .map(|rig| {
            let (trust_score, trust_level) = scores.get(&rig.id).copied().unwrap_or((0.0, "L1"));
            RigInfo {
                id: rig.id,
                rig_type: rig.rig_type,
                recipe: rig.recipe,
                tags: rig.tags,
                trust_level: trust_level.to_string(),
                trust_score,
            }
        })
        .collect();
    Ok(Json(result))
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

    let rig_id = opengoose_board::RigId::new(&id);
    let (stamps, dimensions, trust_score) = state
        .board
        .stamps_with_scores(&rig_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let trust_level = opengoose_board::TrustLevel::from_score(trust_score)
        .as_str()
        .to_string();

    let stamp_infos: Vec<StampInfo> = stamps
        .iter()
        .map(|s| StampInfo {
            work_item_id: s.work_item_id,
            dimension: s.dimension.clone(),
            score: s.score,
            severity: s.severity.clone(),
            stamped_by: s.stamped_by.clone(),
            timestamp: s.timestamp.to_rfc3339(),
        })
        .collect();

    let completed_items = state
        .board
        .completed_by(&rig_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RigDetail {
        id: rig.id,
        rig_type: rig.rig_type,
        recipe: rig.recipe,
        tags: rig.tags,
        trust_level,
        trust_score,
        dimensions: DimensionScore {
            quality: dimensions.quality,
            reliability: dimensions.reliability,
            helpfulness: dimensions.helpfulness,
        },
        stamps: stamp_infos,
        completed_items,
    }))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use opengoose_board::{AddStampParams, Board, PostWorkItem, Priority, RigId};
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    use super::*;

    fn test_app(board: Arc<Board>) -> axum::Router {
        let (tx, _) = broadcast::channel::<()>(64);
        let state = AppState { board, tx };
        axum::Router::new()
            .route("/api/rigs", axum::routing::get(rigs_list))
            .route("/api/rigs/{id}", axum::routing::get(rig_detail))
            .with_state(state)
    }

    async fn new_board() -> Arc<Board> {
        Arc::new(
            Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        )
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("into_body should succeed");
        serde_json::from_slice(&bytes).expect("JSON parse should succeed")
    }

    #[tokio::test]
    async fn rigs_list_empty() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::get("/api/rigs")
                    .body(Body::empty())
                    .expect("body should succeed"),
            )
            .await
            .expect("HTTP request should succeed");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json.as_array().expect("as_array should succeed").len(), 2);
    }

    #[tokio::test]
    async fn rigs_list_with_registered_rig() {
        let board = new_board().await;
        board
            .register_rig("dev-01", "ai", Some("developer"), Some(&["rust".into()]))
            .await
            .expect("register_rig should succeed");

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::get("/api/rigs")
                    .body(Body::empty())
                    .expect("body should succeed"),
            )
            .await
            .expect("HTTP request should succeed");
        let json = body_json(resp).await;
        let rigs = json.as_array().expect("as_array should succeed");
        assert_eq!(rigs.len(), 3);
        assert!(
            rigs.iter()
                .any(|r| r["id"] == "dev-01" && r["rig_type"] == "ai")
        );
    }

    #[tokio::test]
    async fn rig_detail_not_found() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::get("/api/rigs/nonexistent")
                    .body(Body::empty())
                    .expect("body should succeed"),
            )
            .await
            .expect("HTTP request should succeed");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rig_detail_with_stamps_and_completed() {
        let board = new_board().await;
        board
            .register_rig("dev-01", "ai", Some("developer"), None)
            .await
            .expect("register_rig should succeed");

        let item = board
            .post(PostWorkItem {
                title: "Done task".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("board operation should succeed");
        board
            .claim(item.id, &RigId::new("dev-01"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("dev-01"))
            .await
            .expect("submit should succeed");

        board
            .add_stamp(AddStampParams {
                target_rig: "dev-01",
                work_item_id: item.id,
                dimension: "Quality",
                score: 0.8,
                severity: "Leaf",
                stamped_by: "reviewer",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .expect("board operation should succeed");

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::get("/api/rigs/dev-01")
                    .body(Body::empty())
                    .expect("body should succeed"),
            )
            .await
            .expect("HTTP request should succeed");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["id"], "dev-01");
        assert_eq!(
            json["completed_items"]
                .as_array()
                .expect("as_array should succeed")
                .len(),
            1
        );
        assert_eq!(
            json["stamps"]
                .as_array()
                .expect("as_array should succeed")
                .len(),
            1
        );
        assert!(
            json["dimensions"]["quality"]
                .as_f64()
                .expect("as_f64 should succeed")
                > 0.0
        );
        assert_eq!(
            json["dimensions"]["reliability"]
                .as_f64()
                .expect("as_f64 should succeed"),
            0.0
        );
    }

    #[tokio::test]
    async fn rig_detail_all_stamp_dimensions() {
        let board = new_board().await;
        board
            .register_rig("rig-dims", "ai", None, None)
            .await
            .expect("register_rig should succeed");

        let item = board
            .post(PostWorkItem {
                title: "Dim task".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("board operation should succeed");
        board
            .claim(item.id, &RigId::new("rig-dims"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("rig-dims"))
            .await
            .expect("submit should succeed");

        board
            .add_stamp(AddStampParams {
                target_rig: "rig-dims",
                work_item_id: item.id,
                dimension: "Reliability",
                score: 0.8,
                severity: "Leaf",
                stamped_by: "reviewer",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .expect("board operation should succeed");
        board
            .add_stamp(AddStampParams {
                target_rig: "rig-dims",
                work_item_id: item.id,
                dimension: "Helpfulness",
                score: 0.7,
                severity: "Leaf",
                stamped_by: "reviewer",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .expect("board operation should succeed");
        board
            .add_stamp(AddStampParams {
                target_rig: "rig-dims",
                work_item_id: item.id,
                dimension: "UnknownDim",
                score: 0.5,
                severity: "Leaf",
                stamped_by: "reviewer",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .expect("board operation should succeed");

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::get("/api/rigs/rig-dims")
                    .body(Body::empty())
                    .expect("body should succeed"),
            )
            .await
            .expect("HTTP request should succeed");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(
            json["dimensions"]["reliability"]
                .as_f64()
                .expect("as_f64 should succeed")
                > 0.0
        );
        assert!(
            json["dimensions"]["helpfulness"]
                .as_f64()
                .expect("as_f64 should succeed")
                > 0.0
        );
    }
}
