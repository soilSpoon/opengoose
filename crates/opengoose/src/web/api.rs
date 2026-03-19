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

use crate::skills::{load, evolve};

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

// ---------------------------------------------------------------------------
// Skills API
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SkillInfo {
    name: String,
    description: String,
    scope: String,
    scope_level: String,
    lifecycle: Option<String>,
    effectiveness: Option<EffectivenessInfo>,
}

#[derive(Serialize)]
pub struct EffectivenessInfo {
    subsequent_scores: Vec<f32>,
    generation_score: f32,
    is_effective: Option<bool>,
}

#[derive(Serialize)]
pub struct SkillDetail {
    name: String,
    description: String,
    content: String,
    scope: String,
    scope_level: String,
    metadata: Option<evolve::SkillMetadata>,
}

#[derive(Deserialize)]
pub struct PromoteBody {
    to: String,
}

fn skill_dirs() -> (std::path::PathBuf, Option<std::path::PathBuf>, std::path::PathBuf) {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let global_dir = home.join(".opengoose/skills");
    let rigs_base = home.join(".opengoose/rigs");
    let project_dir_path = std::path::PathBuf::from(".opengoose/skills");
    let project_dir = if project_dir_path.is_dir() {
        Some(project_dir_path)
    } else {
        None
    };
    (global_dir, project_dir, rigs_base)
}

fn determine_scope_level(
    path: &std::path::Path,
    _global_dir: &std::path::Path,
    project_dir: Option<&std::path::Path>,
    rigs_base: &std::path::Path,
) -> String {
    if let Ok(canon_path) = path.canonicalize() {
        if let Ok(canon_rigs) = rigs_base.canonicalize() {
            if canon_path.starts_with(&canon_rigs) {
                if let Some(rig_id) = canon_path
                    .strip_prefix(&canon_rigs)
                    .ok()
                    .and_then(|p| p.components().next())
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                {
                    return format!("rig:{rig_id}");
                }
            }
        }
    }
    if let Some(proj) = project_dir {
        if path.starts_with(proj) {
            return "project".into();
        }
    }
    "global".into()
}

fn loaded_to_info(
    s: &load::LoadedSkill,
    global_dir: &std::path::Path,
    project_dir: Option<&std::path::Path>,
    rigs_base: &std::path::Path,
) -> SkillInfo {
    let meta = load::read_metadata(&s.path);
    let scope_level = determine_scope_level(&s.path, global_dir, project_dir, rigs_base);

    let lifecycle = if s.scope == load::SkillScope::Learned {
        meta.as_ref().map(|m| {
            let lc = load::determine_lifecycle(&m.generated_at, m.last_included_at.as_deref());
            match lc {
                load::Lifecycle::Active => "active",
                load::Lifecycle::Dormant => "dormant",
                load::Lifecycle::Archived => "archived",
            }
            .to_string()
        })
    } else {
        None
    };

    let effectiveness = meta.as_ref().map(|m| EffectivenessInfo {
        subsequent_scores: m.effectiveness.subsequent_scores.clone(),
        generation_score: m.generated_from.score,
        is_effective: load::is_effective(m),
    });

    SkillInfo {
        name: s.name.clone(),
        description: s.description.clone(),
        scope: match s.scope {
            load::SkillScope::Installed => "installed".into(),
            load::SkillScope::Learned => "learned".into(),
        },
        scope_level,
        lifecycle,
        effectiveness,
    }
}

fn collect_all_skills() -> Vec<load::LoadedSkill> {
    let (global_dir, project_dir, rigs_base) = skill_dirs();

    let mut all_skills =
        load::load_skills_3_scope(&global_dir, project_dir.as_deref(), None, &rigs_base);

    // Also scan all rig directories for rig-specific skills
    if rigs_base.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&rigs_base) {
            for entry in entries.flatten() {
                let rig_id = entry.file_name().to_string_lossy().to_string();
                let rig_skills = load::load_skills_3_scope(
                    &global_dir,
                    project_dir.as_deref(),
                    Some(&rig_id),
                    &rigs_base,
                );
                for skill in rig_skills {
                    if !all_skills.iter().any(|s| s.name == skill.name) {
                        all_skills.push(skill);
                    }
                }
            }
        }
    }

    all_skills
}

pub async fn skills_list() -> Json<Vec<SkillInfo>> {
    let (global_dir, project_dir, rigs_base) = skill_dirs();
    let all_skills = collect_all_skills();

    let result: Vec<SkillInfo> = all_skills
        .iter()
        .map(|s| loaded_to_info(s, &global_dir, project_dir.as_deref(), &rigs_base))
        .collect();

    Json(result)
}

pub async fn skill_detail(
    Path(name): Path<String>,
) -> Result<Json<SkillDetail>, StatusCode> {
    let (global_dir, project_dir, rigs_base) = skill_dirs();
    let all_skills = collect_all_skills();

    let skill = all_skills
        .into_iter()
        .find(|s| s.name == name)
        .ok_or(StatusCode::NOT_FOUND)?;

    let scope_level = determine_scope_level(&skill.path, &global_dir, project_dir.as_deref(), &rigs_base);
    let metadata = load::read_metadata(&skill.path);

    Ok(Json(SkillDetail {
        name: skill.name,
        description: skill.description,
        content: skill.content,
        scope: match skill.scope {
            load::SkillScope::Installed => "installed".into(),
            load::SkillScope::Learned => "learned".into(),
        },
        scope_level,
        metadata,
    }))
}

pub async fn skill_promote(
    Path(name): Path<String>,
    Json(body): Json<PromoteBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match crate::skills::promote::run(&name, &body.to, None, false) {
        Ok(()) => Ok(Json(serde_json::json!({"status": "promoted"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )),
    }
}

pub async fn skill_delete(
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let all_skills = collect_all_skills();

    let skill = all_skills
        .into_iter()
        .find(|s| s.name == name)
        .ok_or(StatusCode::NOT_FOUND)?;

    std::fs::remove_dir_all(&skill.path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({"status": "deleted"})))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use opengoose_board::Board;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    use super::*;

    fn test_app(board: Arc<Board>) -> axum::Router {
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

    async fn new_board() -> Arc<Board> {
        Arc::new(Board::in_memory().await.unwrap())
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

    #[tokio::test]
    async fn board_create_success() {
        let board = new_board().await;
        let app = test_app(board.clone());
        let resp = app
            .oneshot(
                Request::post("/api/board")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title":"New task","priority":"P0","tags":["rust"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["title"], "New task");
        assert_eq!(json["priority"], "P0");
        assert_eq!(json["created_by"], "web");

        let items = board.list().await.unwrap();
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
                    .unwrap(),
            )
            .await
            .unwrap();
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
                    .unwrap(),
            )
            .await
            .unwrap();
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
                    .unwrap(),
            )
            .await
            .unwrap();
        let json = body_json(resp).await;
        assert_eq!(json["priority"], "P1");
        assert_eq!(json["created_by"], "web");
        assert_eq!(json["description"], "");
    }

    #[tokio::test]
    async fn board_claim_success() {
        let board = new_board().await;
        let item = board.post(PostWorkItem {
            title: "Claim me".into(),
            description: String::new(),
            created_by: RigId::new("poster"),
            priority: Priority::P1,
            tags: vec![],
        }).await.unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::post(&format!("/api/board/{}/claim", item.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rig_id":"worker-01"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["status"], "Claimed");
        assert_eq!(json["claimed_by"], "worker-01");
    }

    #[tokio::test]
    async fn board_claim_already_claimed_returns_409() {
        let board = new_board().await;
        let item = board.post(PostWorkItem {
            title: "Taken".into(),
            description: String::new(),
            created_by: RigId::new("poster"),
            priority: Priority::P1,
            tags: vec![],
        }).await.unwrap();
        board.claim(item.id, &RigId::new("first")).await.unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::post(&format!("/api/board/{}/claim", item.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rig_id":"second"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
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
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rigs_list_empty() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(Request::get("/api/rigs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        // system rigs (human, evolver) are auto-created on connect
        assert_eq!(json.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn rigs_list_with_registered_rig() {
        let board = new_board().await;
        board.register_rig("dev-01", "ai", Some("developer"), Some(&["rust".into()])).await.unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(Request::get("/api/rigs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let json = body_json(resp).await;
        let rigs = json.as_array().unwrap();
        // 2 system rigs + 1 registered
        assert_eq!(rigs.len(), 3);
        assert!(rigs.iter().any(|r| r["id"] == "dev-01" && r["rig_type"] == "ai"));
    }

    #[tokio::test]
    async fn rig_detail_not_found() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(Request::get("/api/rigs/nonexistent").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rig_detail_with_stamps_and_completed() {
        let board = new_board().await;
        board.register_rig("dev-01", "ai", Some("developer"), None).await.unwrap();

        let item = board.post(PostWorkItem {
            title: "Done task".into(),
            description: String::new(),
            created_by: RigId::new("poster"),
            priority: Priority::P1,
            tags: vec![],
        }).await.unwrap();
        board.claim(item.id, &RigId::new("dev-01")).await.unwrap();
        board.submit(item.id, &RigId::new("dev-01")).await.unwrap();

        board.add_stamp("dev-01", item.id, "Quality", 0.8, "Leaf", "reviewer", None).await.unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(Request::get("/api/rigs/dev-01").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["id"], "dev-01");
        assert_eq!(json["completed_items"].as_array().unwrap().len(), 1);
        assert_eq!(json["stamps"].as_array().unwrap().len(), 1);
        assert!(json["dimensions"]["quality"].as_f64().unwrap() > 0.0);
        assert_eq!(json["dimensions"]["reliability"].as_f64().unwrap(), 0.0);
    }
}
