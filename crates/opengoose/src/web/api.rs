use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use opengoose_board::{PostWorkItem, Priority, RigId};
use serde::{Deserialize, Serialize};

use crate::skills::{evolve, load};

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

    // 단일 쿼리: stamps + dimension scores + total score
    let (stamps, dimensions, trust_score) = state
        .board
        .stamps_with_scores(&id)
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

    // SQL-filtered completed items instead of full table scan
    let completed_items = state
        .board
        .completed_by_rig(&id)
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

fn skill_dirs() -> (
    std::path::PathBuf,
    Option<std::path::PathBuf>,
    std::path::PathBuf,
) {
    let home = crate::home_dir();
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

struct SkillContext {
    project_dir: Option<std::path::PathBuf>,
    rigs_base: std::path::PathBuf,
    canon_rigs: Option<std::path::PathBuf>,
}

impl SkillContext {
    fn new() -> Self {
        let (_global_dir, project_dir, rigs_base) = skill_dirs();
        let canon_rigs = rigs_base.canonicalize().ok();
        Self {
            project_dir,
            rigs_base,
            canon_rigs,
        }
    }

    fn collect_all_skills(&self) -> Vec<load::LoadedSkill> {
        let mut skill_map: std::collections::HashMap<String, load::LoadedSkill> =
            load::load_skills_for(None, self.project_dir.as_deref())
                .into_iter()
                .map(|s| (s.name.clone(), s))
                .collect();

        if self.rigs_base.is_dir()
            && let Ok(entries) = std::fs::read_dir(&self.rigs_base)
        {
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let rig_id = entry.file_name().to_string_lossy().to_string();
                for skill in load::load_skills_for(Some(&rig_id), self.project_dir.as_deref()) {
                    skill_map.insert(skill.name.clone(), skill);
                }
            }
        }

        skill_map.into_values().collect()
    }

    fn determine_scope_level(&self, path: &std::path::Path) -> String {
        if let Some(canon_rigs) = &self.canon_rigs
            && let Ok(canon_path) = path.canonicalize()
            && canon_path.starts_with(canon_rigs)
            && let Some(rig_id) = canon_path
                .strip_prefix(canon_rigs)
                .ok()
                .and_then(|p| p.components().next())
                .map(|c| c.as_os_str().to_string_lossy().to_string())
        {
            return format!("rig:{rig_id}");
        }
        if let Some(proj) = &self.project_dir
            && path.starts_with(proj)
        {
            return "project".into();
        }
        "global".into()
    }

    fn to_info(&self, s: &load::LoadedSkill) -> SkillInfo {
        let meta = load::read_metadata(&s.path);
        let scope_level = self.determine_scope_level(&s.path);

        let lifecycle = if s.scope == load::SkillScope::Learned {
            meta.as_ref().map(|m| {
                match load::determine_lifecycle(&m.generated_at, m.last_included_at.as_deref()) {
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
}

pub async fn skills_list() -> Json<Vec<SkillInfo>> {
    let ctx = SkillContext::new();
    let all_skills = ctx.collect_all_skills();
    let result: Vec<SkillInfo> = all_skills.iter().map(|s| ctx.to_info(s)).collect();
    Json(result)
}

pub async fn skill_detail(Path(name): Path<String>) -> Result<Json<SkillDetail>, StatusCode> {
    let ctx = SkillContext::new();
    let all_skills = ctx.collect_all_skills();

    let skill = all_skills
        .into_iter()
        .find(|s| s.name == name)
        .ok_or(StatusCode::NOT_FOUND)?;

    let scope_level = ctx.determine_scope_level(&skill.path);
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
    match opengoose_skills::manage::promote::run(&crate::home_dir(), &name, &body.to, None, false) {
        Ok(()) => Ok(Json(serde_json::json!({"status": "promoted"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )),
    }
}

pub async fn skill_delete(Path(name): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let ctx = SkillContext::new();
    let all_skills = ctx.collect_all_skills();

    let skill = all_skills
        .into_iter()
        .find(|s| s.name == name)
        .ok_or(StatusCode::NOT_FOUND)?;

    tokio::fs::remove_dir_all(&skill.path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({"status": "deleted"})))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use opengoose_board::{AddStampParams, Board};
    use std::env;
    use std::ffi::OsString;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    use super::*;

    use crate::ENV_LOCK;

    fn with_isolated_paths(tmp: &std::path::Path) {
        unsafe {
            env::set_var("HOME", tmp);
        }
        env::set_current_dir(tmp).unwrap();
    }

    fn restore_env(home: Option<OsString>, cwd: std::path::PathBuf) {
        unsafe {
            match home {
                Some(v) => env::set_var("HOME", v),
                None => env::remove_var("HOME"),
            }
        }
        env::set_current_dir(cwd).unwrap();
    }

    fn skill_metadata_json() -> serde_json::Value {
        serde_json::json!({
            "generated_from": {
                "stamp_id": 1,
                "work_item_id": 100,
                "dimension": "Quality",
                "score": 0.75,
            },
            "generated_at": Utc::now().to_rfc3339(),
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": {
                "injected_count": 1,
                "subsequent_scores": [0.3, 0.4, 0.5],
            },
        })
    }

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
            .route("/api/rigs", axum::routing::get(rigs_list))
            .route("/api/rigs/{id}", axum::routing::get(rig_detail))
            .with_state(state)
    }

    async fn new_board() -> Arc<Board> {
        Arc::new(Board::in_memory().await.unwrap())
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
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
        board
            .post(PostWorkItem {
                title: "Task A".into(),
                description: String::new(),
                created_by: RigId::new("test"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

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
        let item = board
            .post(PostWorkItem {
                title: "Find me".into(),
                description: String::new(),
                created_by: RigId::new("test"),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::get(format!("/api/board/{}", item.id))
                    .body(Body::empty())
                    .unwrap(),
            )
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
                    .body(Body::from(
                        r#"{"title":"New task","priority":"P0","tags":["rust"]}"#,
                    ))
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
        let item = board
            .post(PostWorkItem {
                title: "Claim me".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::post(format!("/api/board/{}/claim", item.id))
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
        let item = board
            .post(PostWorkItem {
                title: "Taken".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board.claim(item.id, &RigId::new("first")).await.unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::post(format!("/api/board/{}/claim", item.id))
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
        board
            .register_rig("dev-01", "ai", Some("developer"), Some(&["rust".into()]))
            .await
            .unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(Request::get("/api/rigs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let json = body_json(resp).await;
        let rigs = json.as_array().unwrap();
        // 2 system rigs + 1 registered
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
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rig_detail_with_stamps_and_completed() {
        let board = new_board().await;
        board
            .register_rig("dev-01", "ai", Some("developer"), None)
            .await
            .unwrap();

        let item = board
            .post(PostWorkItem {
                title: "Done task".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board.claim(item.id, &RigId::new("dev-01")).await.unwrap();
        board.submit(item.id, &RigId::new("dev-01")).await.unwrap();

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
            .unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::get("/api/rigs/dev-01")
                    .body(Body::empty())
                    .unwrap(),
            )
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

    fn skill_test_app() -> axum::Router {
        axum::Router::new()
            .route("/api/skills", axum::routing::get(skills_list))
            .route(
                "/api/skills/{name}",
                axum::routing::get(skill_detail).delete(skill_delete),
            )
            .route(
                "/api/skills/{name}/promote",
                axum::routing::post(skill_promote),
            )
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn skills_list_returns_installed_skill() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/installed/alpha");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: alpha\ndescription: Use when testing\n---\nbody\n",
        )
        .unwrap();

        let resp = skill_test_app()
            .oneshot(Request::get("/api/skills").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(
            json.as_array()
                .unwrap()
                .iter()
                .any(|s| s["name"] == "alpha")
        );

        restore_env(home, cwd);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn skill_detail_returns_skill_and_not_found() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/installed/beta");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: beta\ndescription: Use when testing beta\n---\nbody\n",
        )
        .unwrap();

        let app = skill_test_app();
        let resp = app
            .clone()
            .oneshot(
                Request::get("/api/skills/beta")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["name"], "beta");
        assert_eq!(json["scope"], "installed");

        let resp2 = skill_test_app()
            .oneshot(
                Request::get("/api/skills/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp2.status(), StatusCode::NOT_FOUND);

        restore_env(home, cwd);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn skill_delete_removes_skill_and_not_found() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/installed/to-delete");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: to-delete\ndescription: Use when testing delete\n---\n",
        )
        .unwrap();

        let resp = skill_test_app()
            .oneshot(
                Request::delete("/api/skills/to-delete")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!skill_dir.exists());

        let resp2 = skill_test_app()
            .oneshot(
                Request::delete("/api/skills/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp2.status(), StatusCode::NOT_FOUND);

        restore_env(home, cwd);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn skill_promote_returns_error_for_nonexistent_skill() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let resp = skill_test_app()
            .oneshot(
                Request::post("/api/skills/no-skill/promote")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"to":"global"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        restore_env(home, cwd);
    }

    #[tokio::test]
    async fn board_create_with_p2_priority() {
        let app = test_app(new_board().await);
        let resp = app
            .oneshot(
                Request::post("/api/board")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"title":"P2 task","priority":"P2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
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
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn default_rig_is_web() {
        assert_eq!(default_rig(), "web");
    }

    #[test]
    fn determine_scope_level_classifies_rig_project_global() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let rigs_base = tmp.path().join(".opengoose/rigs");
        let global_dir = tmp.path().join(".opengoose/skills");
        let project_dir = tmp.path().join(".opengoose/project");
        std::fs::create_dir_all(rigs_base.join("worker-a/skills/learned").join("skill-a")).unwrap();
        std::fs::create_dir_all(project_dir.join("skill-b")).unwrap();
        std::fs::create_dir_all(global_dir.join("skill-c")).unwrap();

        let rig_skill = rigs_base.join("worker-a/skills/learned/skill-a");
        let project_skill = project_dir.join("skill-b");
        let global_skill = global_dir.join("skill-c");

        let ctx = SkillContext {
            project_dir: Some(project_dir.clone()),
            rigs_base: rigs_base.clone(),
            canon_rigs: rigs_base.canonicalize().ok(),
        };
        assert_eq!(ctx.determine_scope_level(&rig_skill), "rig:worker-a");
        assert_eq!(ctx.determine_scope_level(&project_skill), "project");
        assert_eq!(ctx.determine_scope_level(&global_skill), "global");

        // Non-existent path → canonicalize fails → covers line 309 (} of if let Ok(canon_path))
        let nonexistent = tmp.path().join("nonexistent-skill-path");
        assert_eq!(ctx.determine_scope_level(&nonexistent), "global");

        // Path exactly equal to rigs_base → strip_prefix returns empty → components().next() = None
        // → rig_id is None → covers line 306 (} of if let Some(rig_id))
        assert_eq!(ctx.determine_scope_level(&rigs_base), "global");

        restore_env(home, cwd);
    }

    #[test]
    fn skill_dirs_resolves_expected_directories() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());
        std::fs::create_dir_all(".opengoose/skills").unwrap();

        let (global_dir, project_dir, rigs_base) = skill_dirs();
        assert_eq!(global_dir, tmp.path().join(".opengoose/skills"));
        assert!(project_dir.is_some());
        assert_eq!(
            project_dir.unwrap(),
            std::path::PathBuf::from(".opengoose/skills")
        );
        assert_eq!(rigs_base, tmp.path().join(".opengoose/rigs"));

        restore_env(home, cwd);
    }

    #[test]
    fn loaded_to_info_maps_metadata_fields() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/learned/insight");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: insight\ndescription: Use when testing\n---\nbody\n",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string(&skill_metadata_json()).unwrap(),
        )
        .unwrap();

        let loaded = load::LoadedSkill {
            name: "insight".into(),
            description: "Use when testing".into(),
            path: skill_dir,
            content: "body".into(),
            scope: load::SkillScope::Learned,
        };
        let ctx = SkillContext {
            project_dir: None,
            rigs_base: tmp.path().join(".opengoose/rigs"),
            canon_rigs: tmp.path().join(".opengoose/rigs").canonicalize().ok(),
        };
        let info = ctx.to_info(&loaded);
        assert_eq!(info.scope, "learned");
        assert_eq!(info.scope_level, "global");
        assert_eq!(info.effectiveness.as_ref().unwrap().generation_score, 0.75);
        assert!(info.lifecycle.is_some());

        restore_env(home, cwd);
    }

    #[test]
    fn collect_all_skills_prefers_rig_over_global() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let global = tmp.path().join(".opengoose/skills/installed/shared");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: shared\ndescription: global\n---\n",
        )
        .unwrap();

        let rig = tmp
            .path()
            .join(".opengoose/rigs/worker/skills/learned/shared");
        std::fs::create_dir_all(&rig).unwrap();
        std::fs::write(
            rig.join("SKILL.md"),
            "---\nname: shared\ndescription: rig\n---\n",
        )
        .unwrap();

        let ctx = SkillContext::new();
        let skills = ctx.collect_all_skills();
        let shared_count = skills.iter().filter(|s| s.name == "shared").count();
        assert_eq!(shared_count, 1);
        let shared = skills.iter().find(|s| s.name == "shared").unwrap();
        assert!(
            shared.description.contains("rig"),
            "rig-scope skill should win over global"
        );

        restore_env(home, cwd);
    }

    #[test]
    fn skill_dirs_returns_none_project_dir_when_not_present() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());
        // Do NOT create .opengoose/skills → project_dir should be None

        let (global_dir, project_dir, rigs_base) = skill_dirs();
        assert!(project_dir.is_none());
        assert_eq!(global_dir, tmp.path().join(".opengoose/skills"));
        assert_eq!(rigs_base, tmp.path().join(".opengoose/rigs"));

        restore_env(home, cwd);
    }

    #[test]
    fn loaded_to_info_with_dormant_and_archived_lifecycle() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let dormant_dir = tmp.path().join(".opengoose/skills/learned/dormant-skill");
        std::fs::create_dir_all(&dormant_dir).unwrap();
        std::fs::write(
            dormant_dir.join("SKILL.md"),
            "---\nname: dormant-skill\ndescription: Use when dormant\n---\n",
        )
        .unwrap();
        let dormant_date = (Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        std::fs::write(
            dormant_dir.join("metadata.json"),
            serde_json::to_string(&serde_json::json!({
                "generated_from": {"stamp_id": 1, "work_item_id": 1, "dimension": "Q", "score": 0.2},
                "generated_at": dormant_date.clone(),
                "evolver_work_item_id": null,
                "last_included_at": dormant_date,
                "effectiveness": {"injected_count": 1, "subsequent_scores": []},
            })).unwrap(),
        ).unwrap();

        let archived_dir = tmp.path().join(".opengoose/skills/learned/archived-skill");
        std::fs::create_dir_all(&archived_dir).unwrap();
        std::fs::write(
            archived_dir.join("SKILL.md"),
            "---\nname: archived-skill\ndescription: Use when archived\n---\n",
        )
        .unwrap();
        std::fs::write(
            archived_dir.join("metadata.json"),
            serde_json::to_string(&serde_json::json!({
                "generated_from": {"stamp_id": 1, "work_item_id": 1, "dimension": "Q", "score": 0.2},
                "generated_at": "2000-01-01T00:00:00Z",
                "evolver_work_item_id": null,
                "last_included_at": null,
                "effectiveness": {"injected_count": 0, "subsequent_scores": []},
            })).unwrap(),
        ).unwrap();

        let rigs_base = tmp.path().join(".opengoose/rigs");

        let dormant_loaded = load::LoadedSkill {
            name: "dormant-skill".into(),
            description: "Use when dormant".into(),
            path: dormant_dir,
            content: String::new(),
            scope: load::SkillScope::Learned,
        };
        let archived_loaded = load::LoadedSkill {
            name: "archived-skill".into(),
            description: "Use when archived".into(),
            path: archived_dir,
            content: String::new(),
            scope: load::SkillScope::Learned,
        };

        let ctx = SkillContext {
            project_dir: None,
            rigs_base: rigs_base.clone(),
            canon_rigs: rigs_base.canonicalize().ok(),
        };
        let dormant_info = ctx.to_info(&dormant_loaded);
        let archived_info = ctx.to_info(&archived_loaded);

        assert_eq!(dormant_info.lifecycle.as_deref(), Some("dormant"));
        assert_eq!(archived_info.lifecycle.as_deref(), Some("archived"));

        restore_env(home, cwd);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn skill_detail_for_learned_skill_shows_learned_scope() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/learned/learned-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: learned-skill\ndescription: Use when learned\n---\nbody\n",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string(&skill_metadata_json()).unwrap(),
        )
        .unwrap();

        let resp = skill_test_app()
            .oneshot(
                Request::get("/api/skills/learned-skill")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["scope"], "learned");

        restore_env(home, cwd);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn skill_promote_success_returns_promoted_status() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        // Create a skill in a rig so promote::run can find it
        let rig_skill = tmp
            .path()
            .join(".opengoose/rigs/worker-1/skills/learned/promo-skill");
        std::fs::create_dir_all(&rig_skill).unwrap();
        std::fs::write(
            rig_skill.join("SKILL.md"),
            "---\nname: promo-skill\ndescription: Use when promoting\n---\nbody\n",
        )
        .unwrap();

        let resp = skill_test_app()
            .oneshot(
                Request::post("/api/skills/promo-skill/promote")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"to":"global"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["status"], "promoted");

        restore_env(home, cwd);
    }

    #[test]
    fn collect_all_skills_pushes_unique_rig_skill() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_paths(tmp.path());

        // Only a rig skill (no global/project skill with same name)
        let rig_skill = tmp
            .path()
            .join(".opengoose/rigs/worker-2/skills/learned/unique-rig-skill");
        std::fs::create_dir_all(&rig_skill).unwrap();
        std::fs::write(
            rig_skill.join("SKILL.md"),
            "---\nname: unique-rig-skill\ndescription: Use when unique\n---\n",
        )
        .unwrap();

        let skills = SkillContext::new().collect_all_skills();
        assert!(
            skills.iter().any(|s| s.name == "unique-rig-skill"),
            "rig-only skill should be pushed"
        );

        restore_env(home, cwd);
    }

    #[tokio::test]
    async fn board_claim_done_item_returns_bad_request() {
        // Claiming a Done item → InvalidTransition → 400 BAD_REQUEST (the _ arm)
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
            .unwrap();
        board.claim(item.id, &RigId::new("worker")).await.unwrap();
        board.submit(item.id, &RigId::new("worker")).await.unwrap();
        // item is now Done — claiming it again should fail with InvalidTransition

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::post(format!("/api/board/{}/claim", item.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"rig_id":"worker2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rig_detail_all_stamp_dimensions() {
        let board = new_board().await;
        board
            .register_rig("rig-dims", "ai", None, None)
            .await
            .unwrap();

        let item = board
            .post(PostWorkItem {
                title: "Dim task".into(),
                description: String::new(),
                created_by: RigId::new("poster"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board.claim(item.id, &RigId::new("rig-dims")).await.unwrap();
        board
            .submit(item.id, &RigId::new("rig-dims"))
            .await
            .unwrap();

        // Add stamps for all three dimensions + an unknown one
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
            .unwrap();
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
            .unwrap();
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
            .unwrap();

        let app = test_app(board);
        let resp = app
            .oneshot(
                Request::get("/api/rigs/rig-dims")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(json["dimensions"]["reliability"].as_f64().unwrap() > 0.0);
        assert!(json["dimensions"]["helpfulness"].as_f64().unwrap() > 0.0);
    }
}
