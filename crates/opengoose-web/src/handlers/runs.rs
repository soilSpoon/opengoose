use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use super::AppError;
use crate::data::load_run_detail_exact;
use crate::state::AppState;

/// JSON response item for a single orchestration run.
#[derive(Serialize)]
pub struct RunItem {
    pub team_run_id: String,
    pub session_key: String,
    pub team_name: String,
    pub workflow: String,
    pub status: String,
    pub current_step: i32,
    pub total_steps: i32,
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct RunMetaItem {
    pub label: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct RunWorkItem {
    pub title: String,
    pub detail: String,
    pub status_label: String,
    pub status_tone: String,
    pub step_label: String,
    pub indent_class: String,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct RunBroadcastItem {
    pub sender: String,
    pub created_at: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct RunDetail {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub queue_page_url: String,
    pub meta: Vec<RunMetaItem>,
    pub work_items: Vec<RunWorkItem>,
    pub broadcasts: Vec<RunBroadcastItem>,
    pub input: String,
    pub result: String,
    pub empty_hint: String,
}

/// Query parameters for `GET /api/runs`.
#[derive(Deserialize)]
pub struct ListQuery {
    /// Optional status filter (e.g. "running", "completed", "failed", "suspended").
    pub status: Option<String>,
    /// Maximum number of runs to return (default 50, max 1000).
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// GET /api/runs — list orchestration runs with optional status filter.
pub async fn list_runs(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<RunItem>>, AppError> {
    use opengoose_persistence::RunStatus;
    if q.limit <= 0 || q.limit > 1000 {
        return Err(AppError::UnprocessableEntity(format!(
            "`limit` must be between 1 and 1000, got {}",
            q.limit
        )));
    }
    let status = if let Some(s) = q.status.as_deref() {
        Some(RunStatus::parse(s).map_err(|_| {
            AppError::UnprocessableEntity(format!(
                "unknown status `{s}`. Valid: running, completed, failed, suspended"
            ))
        })?)
    } else {
        None
    };
    let runs = state
        .orchestration_store
        .list_runs(status.as_ref(), q.limit)?;
    Ok(Json(
        runs.into_iter()
            .map(|r| RunItem {
                team_run_id: r.team_run_id,
                session_key: r.session_key,
                team_name: r.team_name,
                workflow: r.workflow,
                status: r.status.as_str().to_string(),
                current_step: r.current_step,
                total_steps: r.total_steps,
                result: r.result,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect(),
    ))
}

/// GET /api/runs/:run_id — return detail for a single orchestration run.
pub async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<RunDetail>, AppError> {
    let detail = load_run_detail_exact(state.db, &run_id)?
        .ok_or_else(|| AppError::NotFound(format!("run `{run_id}`")))?;

    Ok(Json(RunDetail {
        title: detail.title,
        subtitle: detail.subtitle,
        source_label: detail.source_label,
        queue_page_url: detail.queue_page_url,
        meta: detail
            .meta
            .into_iter()
            .map(|row| RunMetaItem {
                label: row.label,
                value: row.value,
            })
            .collect(),
        work_items: detail
            .work_items
            .into_iter()
            .map(|item| RunWorkItem {
                title: item.title,
                detail: item.detail,
                status_label: item.status_label,
                status_tone: item.status_tone.into(),
                step_label: item.step_label,
                indent_class: item.indent_class.into(),
                output: item.output,
                error: item.error,
            })
            .collect(),
        broadcasts: detail
            .broadcasts
            .into_iter()
            .map(|message| RunBroadcastItem {
                sender: message.sender,
                created_at: message.created_at,
                content: message.content,
            })
            .collect(),
        input: detail.input,
        result: detail.result,
        empty_hint: detail.empty_hint,
    }))
}

#[cfg(test)]
mod tests {
    use axum::Json;
    use axum::extract::{Query, State};
    use opengoose_persistence::RunStatus;

    use super::{ListQuery, list_runs};
    use crate::error::WebError;
    use crate::handlers::test_support::make_state;

    #[tokio::test]
    async fn list_runs_returns_empty_initially() {
        let state = make_state();

        let Json(runs) = list_runs(
            State(state),
            Query(ListQuery {
                status: None,
                limit: 50,
            }),
        )
        .await
        .expect("list_runs should succeed");

        assert!(runs.is_empty());
    }

    #[tokio::test]
    async fn list_runs_maps_run_fields_from_store() {
        let state = make_state();
        state
            .orchestration_store
            .create_run(
                "run-1",
                "session-1",
                "code-review",
                "chain",
                "review this",
                3,
            )
            .expect("run should be created");
        state
            .orchestration_store
            .advance_step("run-1", 2)
            .expect("step should advance");
        state
            .orchestration_store
            .complete_run("run-1", "approved")
            .expect("run should complete");

        let Json(runs) = list_runs(
            State(state),
            Query(ListQuery {
                status: Some("completed".into()),
                limit: 50,
            }),
        )
        .await
        .expect("list_runs should succeed");

        assert_eq!(runs.len(), 1);
        let run = &runs[0];
        assert_eq!(run.team_run_id, "run-1");
        assert_eq!(run.session_key, "session-1");
        assert_eq!(run.team_name, "code-review");
        assert_eq!(run.workflow, "chain");
        assert_eq!(run.status, RunStatus::Completed.as_str());
        assert_eq!(run.current_step, 2);
        assert_eq!(run.total_steps, 3);
        assert_eq!(run.result.as_deref(), Some("approved"));
        assert!(!run.created_at.is_empty());
        assert!(!run.updated_at.is_empty());
    }

    #[tokio::test]
    async fn list_runs_filters_by_status() {
        let state = make_state();
        state
            .orchestration_store
            .create_run(
                "run-running",
                "session-1",
                "frontend",
                "chain",
                "build ui",
                2,
            )
            .expect("running run should be created");
        state
            .orchestration_store
            .create_run(
                "run-completed",
                "session-2",
                "backend",
                "fan_out",
                "ship api",
                4,
            )
            .expect("completed run should be created");
        state
            .orchestration_store
            .complete_run("run-completed", "done")
            .expect("run should complete");

        let Json(runs) = list_runs(
            State(state),
            Query(ListQuery {
                status: Some("running".into()),
                limit: 50,
            }),
        )
        .await
        .expect("list_runs should succeed");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].team_run_id, "run-running");
        assert_eq!(runs[0].status, RunStatus::Running.as_str());
    }

    #[tokio::test]
    async fn list_runs_respects_limit() {
        let state = make_state();
        for i in 0..5 {
            state
                .orchestration_store
                .create_run(
                    &format!("run-{i}"),
                    &format!("session-{i}"),
                    "ops",
                    "chain",
                    "input",
                    1,
                )
                .expect("run should be created");
        }

        let Json(runs) = list_runs(
            State(state),
            Query(ListQuery {
                status: None,
                limit: 3,
            }),
        )
        .await
        .expect("list_runs should succeed");

        assert_eq!(runs.len(), 3);
    }

    #[tokio::test]
    async fn list_runs_rejects_out_of_range_limits() {
        let zero_limit = list_runs(
            State(make_state()),
            Query(ListQuery {
                status: None,
                limit: 0,
            }),
        )
        .await
        .err()
        .expect("zero limit should be rejected");
        assert!(
            matches!(zero_limit, WebError::UnprocessableEntity(message) if message.contains("between 1 and 1000"))
        );

        let too_large_limit = list_runs(
            State(make_state()),
            Query(ListQuery {
                status: None,
                limit: 1001,
            }),
        )
        .await
        .err()
        .expect("too-large limit should be rejected");
        assert!(
            matches!(too_large_limit, WebError::UnprocessableEntity(message) if message.contains("between 1 and 1000"))
        );
    }

    #[tokio::test]
    async fn list_runs_rejects_unknown_status() {
        let err = list_runs(
            State(make_state()),
            Query(ListQuery {
                status: Some("bogus".into()),
                limit: 50,
            }),
        )
        .await
        .err()
        .expect("unknown status should be rejected");

        assert!(
            matches!(err, WebError::UnprocessableEntity(message) if message.contains("unknown status"))
        );
    }
}
