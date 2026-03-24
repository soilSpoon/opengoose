use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::skills::{evolve, load};

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

pub(crate) fn skill_dirs() -> (
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

pub(crate) struct SkillContext {
    project_dir: Option<std::path::PathBuf>,
    rigs_base: std::path::PathBuf,
    canon_rigs: Option<std::path::PathBuf>,
}

impl SkillContext {
    pub(crate) fn new() -> Self {
        let (_global_dir, project_dir, rigs_base) = skill_dirs();
        let canon_rigs = rigs_base.canonicalize().ok();
        Self {
            project_dir,
            rigs_base,
            canon_rigs,
        }
    }

    pub(crate) fn collect_all_skills(&self) -> Vec<load::LoadedSkill> {
        let base = load::load_skills_for(None, self.project_dir.as_deref());

        let overrides = if self.rigs_base.is_dir()
            && let Ok(entries) = std::fs::read_dir(&self.rigs_base)
        {
            entries
                .flatten()
                .collect::<Vec<_>>()
                .tap_sort_by_key(|e| e.file_name())
                .into_iter()
                .flat_map(|entry| {
                    let rig_id = entry.file_name().to_string_lossy().to_string();
                    load::load_skills_for(Some(&rig_id), self.project_dir.as_deref())
                })
                .collect()
        } else {
            Vec::new()
        };

        merge_skill_sources(base, overrides)
    }

    pub(crate) fn determine_scope_level(&self, path: &std::path::Path) -> String {
        let canon_path = path.canonicalize().ok();
        let canon_project = self.project_dir.as_ref().and_then(|p| p.canonicalize().ok());
        classify_scope(
            canon_path.as_deref().unwrap_or(path),
            self.canon_rigs.as_deref(),
            canon_project.as_deref().or(self.project_dir.as_deref()),
        )
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

/// Merge two skill lists with last-writer-wins semantics: `overrides` take priority over `base`.
fn merge_skill_sources(
    base: Vec<load::LoadedSkill>,
    overrides: Vec<load::LoadedSkill>,
) -> Vec<load::LoadedSkill> {
    let mut map: std::collections::HashMap<String, load::LoadedSkill> =
        base.into_iter().map(|s| (s.name.clone(), s)).collect();
    for skill in overrides {
        map.insert(skill.name.clone(), skill); // last-writer-wins: override takes priority
    }
    map.into_values().collect()
}

/// Pure classification of a canonicalized path into a scope string.
fn classify_scope(
    canon_path: &std::path::Path,
    canon_rigs: Option<&std::path::Path>,
    project_dir: Option<&std::path::Path>,
) -> String {
    if let Some(rigs) = canon_rigs
        && let Ok(rel) = canon_path.strip_prefix(rigs)
        && let Some(rig_id) = rel.components().next()
    {
        return format!("rig:{}", rig_id.as_os_str().to_string_lossy());
    }
    if let Some(pd) = project_dir
        && canon_path.starts_with(pd)
    {
        return "project".into();
    }
    "global".into()
}

/// Helper trait for in-place sort that returns `self` for chaining.
trait TapSort {
    fn tap_sort_by_key<K: Ord>(self, f: impl FnMut(&std::fs::DirEntry) -> K) -> Self;
}

impl TapSort for Vec<std::fs::DirEntry> {
    fn tap_sort_by_key<K: Ord>(mut self, f: impl FnMut(&std::fs::DirEntry) -> K) -> Self {
        self.sort_by_key(f);
        self
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
    use std::env;
    use std::ffi::OsString;
    use tower::ServiceExt;

    use super::*;
    use crate::ENV_LOCK;

    fn with_isolated_paths(tmp: &std::path::Path) {
        unsafe {
            env::set_var("HOME", tmp);
        }
        env::set_current_dir(tmp).expect("set cwd to temp dir");
    }

    fn restore_env(home: Option<OsString>, cwd: std::path::PathBuf) {
        unsafe {
            match home {
                Some(v) => env::set_var("HOME", v),
                None => env::remove_var("HOME"),
            }
        }
        env::set_current_dir(cwd).expect("restore original cwd");
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

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read response body");
        serde_json::from_slice(&bytes).expect("parse response as JSON")
    }

    #[tokio::test]
    async fn skills_list_returns_installed_skill() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/installed/alpha");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: alpha\ndescription: Use when testing\n---\nbody\n",
        )
        .expect("write SKILL.md");

        let resp = skill_test_app()
            .oneshot(Request::get("/api/skills").body(Body::empty()).expect("build request"))
            .await
            .expect("send request");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(
            json.as_array()
                .expect("response is array")
                .iter()
                .any(|s| s["name"] == "alpha")
        );

        restore_env(home, cwd);
    }

    #[tokio::test]
    async fn skill_detail_returns_skill_and_not_found() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/installed/beta");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: beta\ndescription: Use when testing beta\n---\nbody\n",
        )
        .expect("write SKILL.md");

        let app = skill_test_app();
        let resp = app
            .clone()
            .oneshot(
                Request::get("/api/skills/beta")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("send request");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["name"], "beta");
        assert_eq!(json["scope"], "installed");

        let resp2 = skill_test_app()
            .oneshot(
                Request::get("/api/skills/nonexistent")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("send request");
        assert_eq!(resp2.status(), StatusCode::NOT_FOUND);

        restore_env(home, cwd);
    }

    #[tokio::test]
    async fn skill_delete_removes_skill_and_not_found() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/installed/to-delete");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: to-delete\ndescription: Use when testing delete\n---\n",
        )
        .expect("write SKILL.md");

        let resp = skill_test_app()
            .oneshot(
                Request::delete("/api/skills/to-delete")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("send request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!skill_dir.exists());

        let resp2 = skill_test_app()
            .oneshot(
                Request::delete("/api/skills/nonexistent")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("send request");
        assert_eq!(resp2.status(), StatusCode::NOT_FOUND);

        restore_env(home, cwd);
    }

    #[tokio::test]
    async fn skill_promote_returns_error_for_nonexistent_skill() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let resp = skill_test_app()
            .oneshot(
                Request::post("/api/skills/no-skill/promote")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"to":"global"}"#))
                    .expect("build request"),
            )
            .await
            .expect("send request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        restore_env(home, cwd);
    }

    #[test]
    fn determine_scope_level_classifies_rig_project_global() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let rigs_base = tmp.path().join(".opengoose/rigs");
        let global_dir = tmp.path().join(".opengoose/skills");
        let project_dir = tmp.path().join(".opengoose/project");
        std::fs::create_dir_all(rigs_base.join("worker-a/skills/learned").join("skill-a")).expect("create rig skill dir");
        std::fs::create_dir_all(project_dir.join("skill-b")).expect("create project skill dir");
        std::fs::create_dir_all(global_dir.join("skill-c")).expect("create global skill dir");

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

        let nonexistent = tmp.path().join("nonexistent-skill-path");
        assert_eq!(ctx.determine_scope_level(&nonexistent), "global");

        assert_eq!(ctx.determine_scope_level(&rigs_base), "global");

        restore_env(home, cwd);
    }

    #[test]
    fn skill_dirs_resolves_expected_directories() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());
        std::fs::create_dir_all(".opengoose/skills").expect("create skills dir");

        let (global_dir, project_dir, rigs_base) = skill_dirs();
        assert_eq!(global_dir, tmp.path().join(".opengoose/skills"));
        assert!(project_dir.is_some());
        assert_eq!(
            project_dir.expect("project_dir should be Some"),
            std::path::PathBuf::from(".opengoose/skills")
        );
        assert_eq!(rigs_base, tmp.path().join(".opengoose/rigs"));

        restore_env(home, cwd);
    }

    #[test]
    fn loaded_to_info_maps_metadata_fields() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/learned/insight");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: insight\ndescription: Use when testing\n---\nbody\n",
        )
        .expect("write SKILL.md");
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string(&skill_metadata_json()).expect("serialize metadata"),
        )
        .expect("write metadata.json");

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
        assert_eq!(info.effectiveness.as_ref().expect("effectiveness present").generation_score, 0.75);
        assert!(info.lifecycle.is_some());

        restore_env(home, cwd);
    }

    #[test]
    fn collect_all_skills_prefers_rig_over_global() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let global = tmp.path().join(".opengoose/skills/installed/shared");
        std::fs::create_dir_all(&global).expect("create global skill dir");
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: shared\ndescription: global\n---\n",
        )
        .expect("write global SKILL.md");

        let rig = tmp
            .path()
            .join(".opengoose/rigs/worker/skills/learned/shared");
        std::fs::create_dir_all(&rig).expect("create rig skill dir");
        std::fs::write(
            rig.join("SKILL.md"),
            "---\nname: shared\ndescription: rig\n---\n",
        )
        .expect("write rig SKILL.md");

        let ctx = SkillContext::new();
        let skills = ctx.collect_all_skills();
        let shared_count = skills.iter().filter(|s| s.name == "shared").count();
        assert_eq!(shared_count, 1);
        let shared = skills.iter().find(|s| s.name == "shared").expect("find shared skill");
        assert!(
            shared.description.contains("rig"),
            "rig-scope skill should win over global"
        );

        restore_env(home, cwd);
    }

    #[test]
    fn skill_dirs_returns_none_project_dir_when_not_present() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let (global_dir, project_dir, rigs_base) = skill_dirs();
        assert!(project_dir.is_none());
        assert_eq!(global_dir, tmp.path().join(".opengoose/skills"));
        assert_eq!(rigs_base, tmp.path().join(".opengoose/rigs"));

        restore_env(home, cwd);
    }

    #[test]
    fn loaded_to_info_with_dormant_and_archived_lifecycle() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let dormant_dir = tmp.path().join(".opengoose/skills/learned/dormant-skill");
        std::fs::create_dir_all(&dormant_dir).expect("create dormant skill dir");
        std::fs::write(
            dormant_dir.join("SKILL.md"),
            "---\nname: dormant-skill\ndescription: Use when dormant\n---\n",
        )
        .expect("write dormant SKILL.md");
        let dormant_date = (Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        std::fs::write(
            dormant_dir.join("metadata.json"),
            serde_json::to_string(&serde_json::json!({
                "generated_from": {"stamp_id": 1, "work_item_id": 1, "dimension": "Q", "score": 0.2},
                "generated_at": dormant_date.clone(),
                "evolver_work_item_id": null,
                "last_included_at": dormant_date,
                "effectiveness": {"injected_count": 1, "subsequent_scores": []},
            })).expect("serialize dormant metadata"),
        ).expect("write dormant metadata.json");

        let archived_dir = tmp.path().join(".opengoose/skills/learned/archived-skill");
        std::fs::create_dir_all(&archived_dir).expect("create archived skill dir");
        std::fs::write(
            archived_dir.join("SKILL.md"),
            "---\nname: archived-skill\ndescription: Use when archived\n---\n",
        )
        .expect("write archived SKILL.md");
        std::fs::write(
            archived_dir.join("metadata.json"),
            serde_json::to_string(&serde_json::json!({
                "generated_from": {"stamp_id": 1, "work_item_id": 1, "dimension": "Q", "score": 0.2},
                "generated_at": "2000-01-01T00:00:00Z",
                "evolver_work_item_id": null,
                "last_included_at": null,
                "effectiveness": {"injected_count": 0, "subsequent_scores": []},
            })).expect("serialize archived metadata"),
        ).expect("write archived metadata.json");

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
    async fn skill_detail_for_learned_skill_shows_learned_scope() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/skills/learned/learned-skill");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: learned-skill\ndescription: Use when learned\n---\nbody\n",
        )
        .expect("write SKILL.md");
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string(&skill_metadata_json()).expect("serialize metadata"),
        )
        .expect("write metadata.json");

        let resp = skill_test_app()
            .oneshot(
                Request::get("/api/skills/learned-skill")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("send request");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["scope"], "learned");

        restore_env(home, cwd);
    }

    #[tokio::test]
    async fn skill_promote_success_returns_promoted_status() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let rig_skill = tmp
            .path()
            .join(".opengoose/rigs/worker-1/skills/learned/promo-skill");
        std::fs::create_dir_all(&rig_skill).expect("create rig skill dir");
        std::fs::write(
            rig_skill.join("SKILL.md"),
            "---\nname: promo-skill\ndescription: Use when promoting\n---\nbody\n",
        )
        .expect("write SKILL.md");

        let resp = skill_test_app()
            .oneshot(
                Request::post("/api/skills/promo-skill/promote")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"to":"global"}"#))
                    .expect("build request"),
            )
            .await
            .expect("send request");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["status"], "promoted");

        restore_env(home, cwd);
    }

    #[test]
    fn collect_all_skills_pushes_unique_rig_skill() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("get current dir");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("create temp dir");
        with_isolated_paths(tmp.path());

        let rig_skill = tmp
            .path()
            .join(".opengoose/rigs/worker-2/skills/learned/unique-rig-skill");
        std::fs::create_dir_all(&rig_skill).expect("create rig skill dir");
        std::fs::write(
            rig_skill.join("SKILL.md"),
            "---\nname: unique-rig-skill\ndescription: Use when unique\n---\n",
        )
        .expect("write SKILL.md");

        let skills = SkillContext::new().collect_all_skills();
        assert!(
            skills.iter().any(|s| s.name == "unique-rig-skill"),
            "rig-only skill should be pushed"
        );

        restore_env(home, cwd);
    }

    // --- Tests for extracted pure functions ---

    fn make_skill(name: &str, desc: &str) -> load::LoadedSkill {
        load::LoadedSkill {
            name: name.into(),
            description: desc.into(),
            path: std::path::PathBuf::from(format!("/tmp/{name}")),
            content: String::new(),
            scope: load::SkillScope::Installed,
        }
    }

    #[test]
    fn merge_skill_sources_override_wins() {
        let base = vec![make_skill("alpha", "base-alpha")];
        let overrides = vec![make_skill("alpha", "override-alpha")];
        let merged = merge_skill_sources(base, overrides);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].description, "override-alpha");
    }

    #[test]
    fn merge_skill_sources_preserves_unique() {
        let base = vec![make_skill("a", "a"), make_skill("b", "b")];
        let overrides = vec![make_skill("c", "c")];
        let merged = merge_skill_sources(base, overrides);
        assert_eq!(merged.len(), 3);
        let names: std::collections::HashSet<_> = merged.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains("a"));
        assert!(names.contains("b"));
        assert!(names.contains("c"));
    }

    #[test]
    fn merge_skill_sources_empty_override() {
        let base = vec![make_skill("x", "x-desc"), make_skill("y", "y-desc")];
        let merged = merge_skill_sources(base, Vec::new());
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn classify_scope_rig_path_includes_rig_id() {
        let rigs = std::path::Path::new("/home/user/.opengoose/rigs");
        let path = std::path::Path::new("/home/user/.opengoose/rigs/worker-a/skills/learned/s1");
        assert_eq!(classify_scope(path, Some(rigs), None), "rig:worker-a");
    }

    #[test]
    fn classify_scope_project_path() {
        let project = std::path::Path::new("/repo/.opengoose/skills");
        let path = std::path::Path::new("/repo/.opengoose/skills/installed/s1");
        assert_eq!(classify_scope(path, None, Some(project)), "project");
    }

    #[test]
    fn classify_scope_global_fallback() {
        let rigs = std::path::Path::new("/home/user/.opengoose/rigs");
        let project = std::path::Path::new("/repo/.opengoose/skills");
        let path = std::path::Path::new("/home/user/.opengoose/skills/installed/s1");
        assert_eq!(classify_scope(path, Some(rigs), Some(project)), "global");
    }

    #[test]
    fn classify_scope_no_dirs_returns_global() {
        let path = std::path::Path::new("/any/path");
        assert_eq!(classify_scope(path, None, None), "global");
    }
}
