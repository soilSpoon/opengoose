use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use opengoose_board::{PostWorkItem, Priority, RigId};
use serde::Deserialize;

use super::AppState;

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

#[derive(Deserialize)]
pub struct ClaimBody {
    pub rig_id: String,
}

/// Validate a board create request, returning BAD_REQUEST on failure.
fn validate_create_request(req: &CreateItem) -> Result<Priority, StatusCode> {
    if req.title.is_empty() || req.title.len() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.description.len() > 10_000 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let priority = match req.priority.as_str() {
        "P0" => Priority::P0,
        "P2" => Priority::P2,
        _ => Priority::P1,
    };
    Ok(priority)
}

pub async fn board_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<opengoose_board::WorkItem>>, StatusCode> {
    let items = state
        .board
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

pub async fn board_create(
    State(state): State<AppState>,
    Json(body): Json<CreateItem>,
) -> Result<Json<opengoose_board::WorkItem>, StatusCode> {
    let priority = validate_create_request(&body)?;
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

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use opengoose_board::{Board, PostWorkItem, Priority, RigId};
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    use super::*;

    fn test_app(board: Arc<Board>) -> axum::Router {
        let (tx, _) = broadcast::channel::<()>(64);
        let state = AppState { board, tx };
        axum::Router::new()
            .route(
                "/api/board",
                axum::routing::get(board_list).post(board_create),
            )
            .route("/api/board/{id}", axum::routing::get(board_get))
            .route("/api/board/{id}/claim", axum::routing::post(board_claim))
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
            .expect("operation should succeed");
        serde_json::from_slice(&bytes).expect("operation should succeed")
    }

    #[tokio::test]
    async fn board_list_empty() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::get("/api/board")
                    .body(Body::empty())
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json.as_array().expect("operation should succeed").len(), 0);
    }

    #[tokio::test]
    async fn board_list_returns_posted_items() {
        let board = new_board().await;
        board
            .post(PostWorkItem {
                title: "Task A".into(),
                description: String::new(),
                created_by: RigId::new("test"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("operation should succeed");

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::get("/api/board")
                    .body(Body::empty())
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        let json = body_json(resp).await;
        let items = json.as_array().expect("operation should succeed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["title"], "Task A");
        assert_eq!(items[0]["status"], "Open");
    }

    #[tokio::test]
    async fn board_get_existing() {
        let board = new_board().await;
        let item = board
            .post(PostWorkItem {
                title: "Find me".into(),
                description: String::new(),
                created_by: RigId::new("test"),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .expect("operation should succeed");

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::get(format!("/api/board/{}", item.id))
                    .body(Body::empty())
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["title"], "Find me");
        assert_eq!(json["priority"], "P0");
    }

    #[tokio::test]
    async fn board_get_not_found() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::get("/api/board/999")
                    .body(Body::empty())
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn board_create_success() {
        let board = new_board().await;
        let app = test_app(board.clone());
        let resp = app
            .oneshot(
                Request::post("/api/board")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"title":"New task","priority":"P0","tags":["rust"]}"#,
                    ))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["title"], "New task");
        assert_eq!(json["priority"], "P0");
        assert_eq!(json["created_by"], "web");

        let items = board.list().await.expect("list should succeed");
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn board_create_empty_title_rejected() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::post("/api/board")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title":""}"#))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn board_create_title_too_long_rejected() {
        let long_title = "x".repeat(501);
        let body = serde_json::json!({"title": long_title});
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::post("/api/board")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn board_create_defaults() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::post("/api/board")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title":"Minimal"}"#))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        let json = body_json(resp).await;
        assert_eq!(json["priority"], "P1");
        assert_eq!(json["created_by"], "web");
        assert_eq!(json["description"], "");
    }

    #[tokio::test]
    async fn board_claim_success() {
        let board = new_board().await;
        let item = board
            .post(PostWorkItem {
                title: "Claim me".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("operation should succeed");

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::post(format!("/api/board/{}/claim", item.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rig_id":"worker-01"}"#))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["status"], "Claimed");
        assert_eq!(json["claimed_by"], "worker-01");
    }

    #[tokio::test]
    async fn board_claim_already_claimed_returns_409() {
        let board = new_board().await;
        let item = board
            .post(PostWorkItem {
                title: "Taken".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("operation should succeed");
        board
            .claim(item.id, &RigId::new("first"))
            .await
            .expect("claim should succeed");

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::post(format!("/api/board/{}/claim", item.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rig_id":"second"}"#))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn board_claim_not_found_returns_404() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::post("/api/board/999/claim")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rig_id":"worker"}"#))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn board_create_with_p2_priority() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::post("/api/board")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title":"P2 task","priority":"P2"}"#))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["priority"], "P2");
    }

    #[tokio::test]
    async fn board_create_description_too_long_rejected() {
        let long_desc = "x".repeat(10_001);
        let body = serde_json::json!({"title": "ok", "description": long_desc});
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::post("/api/board")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn default_rig_is_web() {
        assert_eq!(default_rig(), "web");
    }

    #[tokio::test]
    async fn board_claim_done_item_returns_bad_request() {
        let board = new_board().await;
        let item = board
            .post(PostWorkItem {
                title: "Already done".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("operation should succeed");
        board
            .claim(item.id, &RigId::new("worker"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("worker"))
            .await
            .expect("submit should succeed");

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::post(format!("/api/board/{}/claim", item.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rig_id":"worker2"}"#))
                    .expect("operation should succeed"),
            )
            .await
            .expect("operation should succeed");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_create_request_rejects_empty_title() {
        let req = CreateItem {
            title: String::new(),
            description: String::new(),
            created_by: "web".into(),
            priority: String::new(),
            tags: vec![],
        };
        assert_eq!(validate_create_request(&req), Err(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn validate_create_request_rejects_long_title() {
        let req = CreateItem {
            title: "x".repeat(501),
            description: String::new(),
            created_by: "web".into(),
            priority: String::new(),
            tags: vec![],
        };
        assert_eq!(validate_create_request(&req), Err(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn validate_create_request_rejects_long_description() {
        let req = CreateItem {
            title: "ok".into(),
            description: "x".repeat(10_001),
            created_by: "web".into(),
            priority: String::new(),
            tags: vec![],
        };
        assert_eq!(validate_create_request(&req), Err(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn validate_create_request_maps_priorities() {
        let p0 = CreateItem {
            title: "ok".into(),
            description: String::new(),
            created_by: "web".into(),
            priority: "P0".into(),
            tags: vec![],
        };
        assert_eq!(validate_create_request(&p0), Ok(Priority::P0));

        let p2 = CreateItem {
            title: "ok".into(),
            description: String::new(),
            created_by: "web".into(),
            priority: "P2".into(),
            tags: vec![],
        };
        assert_eq!(validate_create_request(&p2), Ok(Priority::P2));

        let default = CreateItem {
            title: "ok".into(),
            description: String::new(),
            created_by: "web".into(),
            priority: "unknown".into(),
            tags: vec![],
        };
        assert_eq!(validate_create_request(&default), Ok(Priority::P1));
    }
}
