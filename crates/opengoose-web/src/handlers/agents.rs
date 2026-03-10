use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;

use super::AppError;
use crate::data::load_agent_detail_exact;
use crate::state::AppState;

/// JSON response item representing an agent profile name.
#[derive(Serialize)]
pub struct AgentItem {
    /// Profile name (e.g. "developer", "researcher").
    pub name: String,
}

#[derive(Serialize)]
pub struct AgentSetting {
    pub label: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct AgentExtension {
    pub name: String,
    pub kind: String,
    pub summary: String,
}

#[derive(Serialize)]
pub struct AgentRecentRun {
    pub title: String,
    pub detail: String,
    pub updated_at: String,
    pub status_label: String,
    pub status_tone: String,
    pub page_url: String,
}

#[derive(Serialize)]
pub struct AgentSession {
    pub title: String,
    pub detail: String,
    pub updated_at: String,
    pub badge: String,
    pub badge_tone: String,
    pub page_url: String,
}

#[derive(Serialize)]
pub struct AgentDetail {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub instructions_preview: String,
    pub settings: Vec<AgentSetting>,
    pub activities: Vec<String>,
    pub skills: Vec<String>,
    pub extensions: Vec<AgentExtension>,
    pub recent_runs: Vec<AgentRecentRun>,
    pub connected_sessions: Vec<AgentSession>,
    pub runtime_empty_hint: String,
    pub yaml: String,
}

/// GET /api/agents — list all installed agent profiles.
pub async fn list_agents(State(state): State<AppState>) -> Result<Json<Vec<AgentItem>>, AppError> {
    let names = state.profile_store.list()?;
    Ok(Json(
        names.into_iter().map(|name| AgentItem { name }).collect(),
    ))
}

/// GET /api/agents/:name — return detail for a single agent profile.
pub async fn get_agent(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<AgentDetail>, AppError> {
    let detail = load_agent_detail_exact(state.db, &name)?
        .ok_or_else(|| AppError::NotFound(format!("agent `{name}`")))?;

    Ok(Json(AgentDetail {
        title: detail.title,
        subtitle: detail.subtitle,
        source_label: detail.source_label,
        instructions_preview: detail.instructions_preview,
        settings: detail
            .settings
            .into_iter()
            .map(|row| AgentSetting {
                label: row.label,
                value: row.value,
            })
            .collect(),
        activities: detail.activities,
        skills: detail.skills,
        extensions: detail
            .extensions
            .into_iter()
            .map(|row| AgentExtension {
                name: row.name,
                kind: row.kind,
                summary: row.summary,
            })
            .collect(),
        recent_runs: detail
            .recent_runs
            .into_iter()
            .map(|run| AgentRecentRun {
                title: run.title,
                detail: run.detail,
                updated_at: run.updated_at,
                status_label: run.status_label,
                status_tone: run.status_tone.into(),
                page_url: run.page_url,
            })
            .collect(),
        connected_sessions: detail
            .connected_sessions
            .into_iter()
            .map(|session| AgentSession {
                title: session.title,
                detail: session.detail,
                updated_at: session.updated_at,
                badge: session.badge,
                badge_tone: session.badge_tone.into(),
                page_url: session.page_url,
            })
            .collect(),
        runtime_empty_hint: detail.runtime_empty_hint,
        yaml: detail.yaml,
    }))
}

#[cfg(test)]
mod tests {
    use axum::Json;
    use axum::extract::State;

    use super::list_agents;
    use crate::error::WebError;
    use crate::handlers::test_support::{
        make_state, make_state_with_dirs, sample_profile, unique_temp_dir, unique_temp_path,
    };

    #[tokio::test]
    async fn list_agents_returns_empty_initially() {
        let Json(agents) = list_agents(State(make_state()))
            .await
            .expect("list_agents should succeed");

        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn list_agents_returns_sorted_names() {
        let state = make_state();
        state
            .profile_store
            .save(&sample_profile("zeta"), false)
            .expect("first profile should be saved");
        state
            .profile_store
            .save(&sample_profile("alpha"), false)
            .expect("second profile should be saved");

        let Json(agents) = list_agents(State(state))
            .await
            .expect("list_agents should succeed");

        let names: Vec<String> = agents.into_iter().map(|agent| agent.name).collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
    }

    #[tokio::test]
    async fn list_agents_propagates_store_errors() {
        let invalid_profile_path = unique_temp_path("profiles-file");
        std::fs::write(&invalid_profile_path, "not a directory")
            .expect("profile file should be created");
        let state = make_state_with_dirs(invalid_profile_path, unique_temp_dir("teams"));

        let err = list_agents(State(state))
            .await
            .err()
            .expect("invalid profile store path should fail");

        assert!(matches!(err, WebError::Profile(_)));
    }
}
