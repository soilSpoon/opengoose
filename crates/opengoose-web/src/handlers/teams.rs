use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

/// JSON response item representing a team definition name.
#[derive(Serialize)]
pub struct TeamItem {
    /// Team name (e.g. "code-review", "feature-dev").
    pub name: String,
}

/// GET /api/teams — list all installed team definitions.
pub async fn list_teams(State(state): State<AppState>) -> Result<Json<Vec<TeamItem>>, AppError> {
    let names = state.team_store.list()?;
    Ok(Json(
        names.into_iter().map(|name| TeamItem { name }).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use axum::Json;
    use axum::extract::State;

    use super::list_teams;
    use crate::error::WebError;
    use crate::handlers::test_support::{
        make_state, make_state_with_dirs, sample_team, unique_temp_dir, unique_temp_path,
    };

    #[tokio::test]
    async fn list_teams_returns_empty_initially() {
        let Json(teams) = list_teams(State(make_state()))
            .await
            .expect("list_teams should succeed");

        assert!(teams.is_empty());
    }

    #[tokio::test]
    async fn list_teams_returns_sorted_names() {
        let state = make_state();
        state
            .team_store
            .save(&sample_team("zeta", "developer"), false)
            .expect("first team should be saved");
        state
            .team_store
            .save(&sample_team("alpha", "developer"), false)
            .expect("second team should be saved");

        let Json(teams) = list_teams(State(state))
            .await
            .expect("list_teams should succeed");

        let names: Vec<String> = teams.into_iter().map(|team| team.name).collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
    }

    #[tokio::test]
    async fn list_teams_propagates_store_errors() {
        let invalid_team_path = unique_temp_path("teams-file");
        std::fs::write(&invalid_team_path, "not a directory").expect("team file should be created");
        let state = make_state_with_dirs(unique_temp_dir("profiles"), invalid_team_path);

        let err = list_teams(State(state))
            .await
            .err()
            .expect("invalid team store path should fail");

        assert!(matches!(err, WebError::Team(_)));
    }
}
