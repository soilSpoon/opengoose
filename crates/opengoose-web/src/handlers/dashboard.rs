use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

/// Aggregate counts returned by the dashboard JSON endpoint.
#[derive(Serialize)]
pub struct DashboardStats {
    /// Total number of chat sessions.
    pub session_count: i64,
    /// Total number of stored messages across all sessions.
    pub message_count: i64,
    /// Total number of orchestration runs.
    pub run_count: i64,
    /// Number of installed agent profiles.
    pub agent_count: usize,
    /// Number of installed team definitions.
    pub team_count: usize,
}

/// GET /api/dashboard — return aggregate system statistics.
pub async fn get_dashboard(
    State(state): State<AppState>,
) -> Result<Json<DashboardStats>, AppError> {
    let session_stats = state.session_store.stats()?;
    let runs = state.orchestration_store.list_runs(None, i64::MAX)?;
    let agent_count = state.profile_store.list().map(|v| v.len()).unwrap_or(0);
    let team_count = state.team_store.list().map(|v| v.len()).unwrap_or(0);

    Ok(Json(DashboardStats {
        session_count: session_stats.session_count,
        message_count: session_stats.message_count,
        run_count: runs.len() as i64,
        agent_count,
        team_count,
    }))
}

#[cfg(test)]
mod tests {
    use axum::Json;
    use axum::extract::State;
    use opengoose_types::SessionKey;

    use super::get_dashboard;
    use crate::handlers::test_support::{
        make_state, make_state_with_dirs, sample_profile, sample_team, unique_temp_path,
    };

    #[tokio::test]
    async fn get_dashboard_returns_zero_counts_initially() {
        let Json(stats) = get_dashboard(State(make_state()))
            .await
            .expect("dashboard should succeed");

        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.message_count, 0);
        assert_eq!(stats.run_count, 0);
        assert_eq!(stats.agent_count, 0);
        assert_eq!(stats.team_count, 0);
    }

    #[tokio::test]
    async fn get_dashboard_aggregates_counts_from_stores() {
        let state = make_state();
        let key = SessionKey::from_stable_id("discord:ns:guild:channel");
        state
            .session_store
            .append_user_message(&key, "hello", Some("alice"))
            .expect("user message should be stored");
        state
            .session_store
            .append_assistant_message(&key, "hi there")
            .expect("assistant message should be stored");

        state
            .orchestration_store
            .create_run(
                "run-1",
                "discord:ns:guild:channel",
                "code-review",
                "chain",
                "input",
                2,
            )
            .expect("first run should be created");
        state
            .orchestration_store
            .create_run(
                "run-2",
                "discord:ns:guild:channel",
                "feature-dev",
                "fan_out",
                "input",
                3,
            )
            .expect("second run should be created");

        state
            .profile_store
            .save(&sample_profile("alpha"), false)
            .expect("first profile should be saved");
        state
            .profile_store
            .save(&sample_profile("beta"), false)
            .expect("second profile should be saved");
        state
            .team_store
            .save(&sample_team("frontend-team", "alpha"), false)
            .expect("team should be saved");

        let Json(stats) = get_dashboard(State(state))
            .await
            .expect("dashboard should succeed");

        assert_eq!(stats.session_count, 1);
        assert_eq!(stats.message_count, 2);
        assert_eq!(stats.run_count, 2);
        assert_eq!(stats.agent_count, 2);
        assert_eq!(stats.team_count, 1);
    }

    #[tokio::test]
    async fn get_dashboard_defaults_agent_and_team_counts_to_zero_on_list_errors() {
        let profile_path = unique_temp_path("profiles-file");
        let team_path = unique_temp_path("teams-file");
        std::fs::write(&profile_path, "not a directory").expect("profile file should be created");
        std::fs::write(&team_path, "not a directory").expect("team file should be created");

        let state = make_state_with_dirs(profile_path, team_path);
        let key = SessionKey::from_stable_id("slack:ns:team:channel");
        state
            .session_store
            .append_user_message(&key, "hello", None)
            .expect("message should be stored");
        state
            .orchestration_store
            .create_run("run-1", "slack:ns:team:channel", "ops", "chain", "input", 1)
            .expect("run should be created");

        let Json(stats) = get_dashboard(State(state))
            .await
            .expect("dashboard should succeed");

        assert_eq!(stats.session_count, 1);
        assert_eq!(stats.message_count, 1);
        assert_eq!(stats.run_count, 1);
        assert_eq!(stats.agent_count, 0);
        assert_eq!(stats.team_count, 0);
    }
}
