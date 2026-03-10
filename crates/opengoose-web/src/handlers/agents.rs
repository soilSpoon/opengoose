use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

/// JSON response item representing an agent profile name.
#[derive(Serialize)]
pub struct AgentItem {
    /// Profile name (e.g. "developer", "researcher").
    pub name: String,
}

/// GET /api/agents — list all installed agent profiles.
pub async fn list_agents(State(state): State<AppState>) -> Result<Json<Vec<AgentItem>>, AppError> {
    let names = state.profile_store.list()?;
    Ok(Json(
        names.into_iter().map(|name| AgentItem { name }).collect(),
    ))
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
