use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::error;

use super::AppError;
use crate::state::AppState;
use opengoose_persistence::{OrchestrationRun, Schedule, Trigger};
use opengoose_teams::TeamDefinition;

#[derive(Serialize)]
pub struct WorkflowItem {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub workflow: String,
    pub agent_count: usize,
    pub schedule_count: usize,
    pub enabled_schedule_count: usize,
    pub trigger_count: usize,
    pub enabled_trigger_count: usize,
    pub last_run_status: Option<String>,
    pub last_run_at: Option<String>,
}

#[derive(Serialize)]
pub struct WorkflowStep {
    pub profile: String,
    pub role: Option<String>,
}

#[derive(Serialize)]
pub struct WorkflowAutomation {
    pub kind: &'static str,
    pub name: String,
    pub enabled: bool,
    pub detail: String,
    pub note: String,
}

#[derive(Serialize)]
pub struct WorkflowRun {
    pub team_run_id: String,
    pub status: String,
    pub current_step: i32,
    pub total_steps: i32,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct WorkflowDetail {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub workflow: String,
    pub source_label: String,
    pub yaml: String,
    pub steps: Vec<WorkflowStep>,
    pub automations: Vec<WorkflowAutomation>,
    pub recent_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize, Default)]
pub struct TriggerWorkflowRequest {
    pub input: Option<String>,
}

#[derive(Serialize)]
pub struct TriggerWorkflowResponse {
    pub workflow: String,
    pub accepted: bool,
    pub input: String,
}

/// GET /api/workflows — list all workflow definitions with automation summary.
pub async fn list_workflows(
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkflowItem>>, AppError> {
    let names = state.team_store.list()?;
    let runs = state.orchestration_store.list_runs(None, 200)?;
    let schedules = state.schedule_store.list()?;
    let triggers = state.trigger_store.list()?;

    let workflows = names
        .into_iter()
        .map(|name| {
            let team = state.team_store.get(&name)?;
            Ok(build_workflow_item(
                &name, &team, &schedules, &triggers, &runs,
            ))
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    Ok(Json(workflows))
}

/// GET /api/workflows/:name — return a single workflow definition plus automation and run history.
pub async fn get_workflow(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<WorkflowDetail>, AppError> {
    let team = state.team_store.get(&name)?;
    let schedules = state.schedule_store.list()?;
    let triggers = state.trigger_store.list()?;
    let runs = state.orchestration_store.list_runs(None, 200)?;

    Ok(Json(build_workflow_detail(
        &name, &team, &schedules, &triggers, &runs, &state,
    )?))
}

/// POST /api/workflows/:name/trigger — enqueue a background run for a workflow.
pub async fn trigger_workflow(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<TriggerWorkflowRequest>>,
) -> Result<(StatusCode, Json<TriggerWorkflowResponse>), AppError> {
    let team = state.team_store.get(&name)?;
    let input = body
        .and_then(|Json(payload)| payload.input)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Manual run requested from the web dashboard for {name}"));

    let db = state.db.clone();
    let event_bus = state.event_bus.clone();
    let workflow_name = name.clone();
    let workflow_input = input.clone();
    tokio::spawn(async move {
        if let Err(error) =
            opengoose_teams::run_headless(&workflow_name, &workflow_input, db, event_bus).await
        {
            error!(workflow = %workflow_name, %error, "manual workflow trigger failed");
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(TriggerWorkflowResponse {
            workflow: team.title,
            accepted: true,
            input,
        }),
    ))
}

fn build_workflow_item(
    name: &str,
    team: &TeamDefinition,
    schedules: &[Schedule],
    triggers: &[Trigger],
    runs: &[OrchestrationRun],
) -> WorkflowItem {
    let last_run = runs.iter().find(|run| run.team_name == name);
    let workflow_schedules: Vec<_> = schedules
        .iter()
        .filter(|schedule| schedule.team_name == name)
        .collect();
    let workflow_triggers: Vec<_> = triggers
        .iter()
        .filter(|trigger| trigger.team_name == name)
        .collect();

    WorkflowItem {
        name: name.to_string(),
        title: team.title.clone(),
        description: team.description.clone(),
        workflow: workflow_name(team),
        agent_count: team.agents.len(),
        schedule_count: workflow_schedules.len(),
        enabled_schedule_count: workflow_schedules
            .iter()
            .filter(|schedule| schedule.enabled)
            .count(),
        trigger_count: workflow_triggers.len(),
        enabled_trigger_count: workflow_triggers
            .iter()
            .filter(|trigger| trigger.enabled)
            .count(),
        last_run_status: last_run.map(|run| status_label(run.status.as_str())),
        last_run_at: last_run.map(|run| run.updated_at.clone()),
    }
}

fn build_workflow_detail(
    name: &str,
    team: &TeamDefinition,
    schedules: &[Schedule],
    triggers: &[Trigger],
    runs: &[OrchestrationRun],
    state: &AppState,
) -> Result<WorkflowDetail, AppError> {
    let automations = schedules
        .iter()
        .filter(|schedule| schedule.team_name == name)
        .map(|schedule| WorkflowAutomation {
            kind: "schedule",
            name: schedule.name.clone(),
            enabled: schedule.enabled,
            detail: schedule.cron_expression.clone(),
            note: match (&schedule.last_run_at, &schedule.next_run_at) {
                (Some(last_run), Some(next_run)) => format!("last {last_run} · next {next_run}"),
                (Some(last_run), None) => format!("last {last_run}"),
                (None, Some(next_run)) => format!("next {next_run}"),
                (None, None) => "no executions recorded".into(),
            },
        })
        .chain(
            triggers
                .iter()
                .filter(|trigger| trigger.team_name == name)
                .map(|trigger| WorkflowAutomation {
                    kind: "trigger",
                    name: trigger.name.clone(),
                    enabled: trigger.enabled,
                    detail: trigger.trigger_type.clone(),
                    note: trigger
                        .last_fired_at
                        .as_ref()
                        .map(|last_fired| {
                            format!(
                                "last fired {last_fired} · {} total fire(s)",
                                trigger.fire_count
                            )
                        })
                        .unwrap_or_else(|| format!("{} total fire(s)", trigger.fire_count)),
                }),
        )
        .collect();
    let recent_runs = runs
        .iter()
        .filter(|run| run.team_name == name)
        .take(6)
        .map(|run| WorkflowRun {
            team_run_id: run.team_run_id.clone(),
            status: status_label(run.status.as_str()),
            current_step: run.current_step,
            total_steps: run.total_steps,
            updated_at: run.updated_at.clone(),
        })
        .collect();

    Ok(WorkflowDetail {
        name: name.to_string(),
        title: team.title.clone(),
        description: team.description.clone(),
        workflow: workflow_name(team),
        source_label: format!("{}", state.team_store.dir().display()),
        yaml: team.to_yaml()?,
        steps: team
            .agents
            .iter()
            .map(|agent| WorkflowStep {
                profile: agent.profile.clone(),
                role: agent.role.clone(),
            })
            .collect(),
        automations,
        recent_runs,
    })
}

fn workflow_name(team: &TeamDefinition) -> String {
    match team.workflow {
        opengoose_teams::OrchestrationPattern::Chain => "chain".into(),
        opengoose_teams::OrchestrationPattern::FanOut => "fan-out".into(),
        opengoose_teams::OrchestrationPattern::Router => "router".into(),
    }
}

fn status_label(value: &str) -> String {
    value
        .split('_')
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut label = first.to_uppercase().collect::<String>();
                    label.push_str(chars.as_str());
                    label
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::Json;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use opengoose_teams::{
        FanOutConfig, MergeStrategy, OrchestrationPattern, TeamAgent, TeamDefinition,
    };

    use super::{TriggerWorkflowRequest, get_workflow, list_workflows, trigger_workflow};
    use crate::error::WebError;
    use crate::handlers::test_support::{
        make_state_with_dirs, sample_team as shared_sample_team, unique_temp_dir, unique_temp_path,
    };
    use crate::state::AppState;

    struct TestContext {
        state: AppState,
        teams_dir: PathBuf,
    }

    fn make_context() -> TestContext {
        let teams_dir = unique_temp_dir("workflows-teams");
        let profiles_dir = unique_temp_dir("workflows-profiles");
        let state = make_state_with_dirs(profiles_dir, teams_dir.clone());

        TestContext { state, teams_dir }
    }

    fn sample_team(name: &str, workflow: OrchestrationPattern) -> TeamDefinition {
        let mut team = shared_sample_team(name, "planner");
        let fan_out = if matches!(&workflow, OrchestrationPattern::FanOut) {
            Some(FanOutConfig {
                merge_strategy: MergeStrategy::Summary,
            })
        } else {
            None
        };

        team.version = "1.0.0".into();
        team.workflow = workflow;
        team.agents = vec![
            TeamAgent {
                profile: "planner".into(),
                role: Some("Plan".into()),
            },
            TeamAgent {
                profile: "builder".into(),
                role: Some("Build".into()),
            },
        ];
        team.fan_out = fan_out;
        team
    }

    fn save_team(state: &AppState, team: &TeamDefinition) {
        state
            .team_store
            .save(team, false)
            .expect("team should be saved");
    }

    #[tokio::test]
    async fn list_workflows_summarizes_automation_counts_and_last_run() {
        let ctx = make_context();
        save_team(
            &ctx.state,
            &sample_team("feature-dev", OrchestrationPattern::Chain),
        );
        save_team(
            &ctx.state,
            &sample_team("ops-review", OrchestrationPattern::FanOut),
        );

        ctx.state
            .schedule_store
            .create("nightly", "0 0 * * *", "feature-dev", "", None)
            .expect("schedule should be created");
        ctx.state
            .schedule_store
            .create("paused", "0 12 * * *", "feature-dev", "", None)
            .expect("schedule should be created");
        ctx.state
            .schedule_store
            .set_enabled("paused", false)
            .expect("schedule should be disabled");

        ctx.state
            .trigger_store
            .create("on-pr", "webhook_received", "{}", "feature-dev", "")
            .expect("trigger should be created");
        ctx.state
            .trigger_store
            .create(
                "disabled-trigger",
                "webhook_received",
                "{}",
                "feature-dev",
                "",
            )
            .expect("trigger should be created");
        ctx.state
            .trigger_store
            .set_enabled("disabled-trigger", false)
            .expect("trigger should be disabled");

        ctx.state
            .orchestration_store
            .create_run(
                "run-feature",
                "sess-feature",
                "feature-dev",
                "chain",
                "ship it",
                2,
            )
            .expect("run should be created");
        ctx.state
            .orchestration_store
            .complete_run("run-feature", "done")
            .expect("run should be completed");

        let Json(items) = list_workflows(State(ctx.state))
            .await
            .expect("list workflows should succeed");

        assert_eq!(items.len(), 2);

        let feature = items
            .iter()
            .find(|item| item.name == "feature-dev")
            .expect("feature-dev should be listed");
        assert_eq!(feature.workflow, "chain");
        assert_eq!(feature.agent_count, 2);
        assert_eq!(feature.schedule_count, 2);
        assert_eq!(feature.enabled_schedule_count, 1);
        assert_eq!(feature.trigger_count, 2);
        assert_eq!(feature.enabled_trigger_count, 1);
        assert_eq!(feature.last_run_status.as_deref(), Some("Completed"));

        let ops_review = items
            .iter()
            .find(|item| item.name == "ops-review")
            .expect("ops-review should be listed");
        assert_eq!(ops_review.workflow, "fan-out");
        assert_eq!(ops_review.agent_count, 2);
    }

    #[tokio::test]
    async fn get_workflow_returns_yaml_steps_automations_and_recent_runs() {
        let ctx = make_context();
        let teams_dir = ctx.teams_dir.clone();
        save_team(
            &ctx.state,
            &sample_team("feature-dev", OrchestrationPattern::Chain),
        );

        ctx.state
            .schedule_store
            .create(
                "nightly",
                "0 0 * * *",
                "feature-dev",
                "",
                Some("2026-01-01 00:00:00"),
            )
            .expect("schedule should be created");
        ctx.state
            .schedule_store
            .mark_run("nightly", Some("2026-01-02 00:00:00"))
            .expect("schedule should record a run");

        ctx.state
            .trigger_store
            .create("on-pr", "webhook_received", "{}", "feature-dev", "")
            .expect("trigger should be created");
        ctx.state
            .trigger_store
            .mark_fired("on-pr")
            .expect("trigger should record a firing");

        ctx.state
            .orchestration_store
            .create_run(
                "run-feature",
                "sess-feature",
                "feature-dev",
                "chain",
                "ship it",
                2,
            )
            .expect("run should be created");
        ctx.state
            .orchestration_store
            .advance_step("run-feature", 1)
            .expect("run should advance");

        let Json(detail) = get_workflow(State(ctx.state), Path("feature-dev".into()))
            .await
            .expect("workflow detail should succeed");

        assert_eq!(detail.name, "feature-dev");
        assert_eq!(detail.title, "feature-dev");
        assert_eq!(detail.workflow, "chain");
        assert_eq!(detail.source_label, teams_dir.display().to_string());
        assert!(detail.yaml.contains("title: feature-dev"));
        assert_eq!(detail.steps.len(), 2);
        assert_eq!(detail.steps[0].profile, "planner");
        assert_eq!(detail.steps[0].role.as_deref(), Some("Plan"));
        assert_eq!(detail.recent_runs.len(), 1);
        assert_eq!(detail.recent_runs[0].status, "Running");
        assert_eq!(detail.recent_runs[0].current_step, 1);
        assert_eq!(detail.recent_runs[0].total_steps, 2);

        let schedule = detail
            .automations
            .iter()
            .find(|automation| automation.kind == "schedule")
            .expect("schedule automation should be present");
        assert!(schedule.note.contains("last "));
        assert!(schedule.note.contains("next 2026-01-02 00:00:00"));

        let trigger = detail
            .automations
            .iter()
            .find(|automation| automation.kind == "trigger")
            .expect("trigger automation should be present");
        assert!(trigger.note.contains("last fired"));
        assert!(trigger.note.contains("1 total fire(s)"));
    }

    #[tokio::test]
    async fn trigger_workflow_trims_explicit_input() {
        let ctx = make_context();
        save_team(
            &ctx.state,
            &sample_team("feature-dev", OrchestrationPattern::Chain),
        );

        let (status, Json(response)) = trigger_workflow(
            State(ctx.state),
            Path("feature-dev".into()),
            Some(Json(TriggerWorkflowRequest {
                input: Some("  ship it  ".into()),
            })),
        )
        .await
        .expect("manual trigger should succeed");

        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(response.workflow, "feature-dev");
        assert_eq!(response.input, "ship it");
        assert!(response.accepted);
    }

    #[tokio::test]
    async fn trigger_workflow_uses_default_input_for_missing_or_blank_body() {
        let ctx = make_context();
        save_team(
            &ctx.state,
            &sample_team("feature-dev", OrchestrationPattern::Chain),
        );

        let (_, Json(defaulted)) =
            trigger_workflow(State(ctx.state.clone()), Path("feature-dev".into()), None)
                .await
                .expect("manual trigger should succeed without a body");
        assert_eq!(
            defaulted.input,
            "Manual run requested from the web dashboard for feature-dev"
        );

        let (_, Json(blank)) = trigger_workflow(
            State(ctx.state),
            Path("feature-dev".into()),
            Some(Json(TriggerWorkflowRequest {
                input: Some("   ".into()),
            })),
        )
        .await
        .expect("manual trigger should succeed with blank input");
        assert_eq!(
            blank.input,
            "Manual run requested from the web dashboard for feature-dev"
        );
    }

    #[tokio::test]
    async fn list_workflows_propagates_team_store_errors() {
        let invalid_teams_path = unique_temp_path("workflows-teams-file");
        std::fs::write(&invalid_teams_path, "not a directory")
            .expect("team file should be created");
        let state = make_state_with_dirs(unique_temp_dir("profiles"), invalid_teams_path);

        let err = list_workflows(State(state))
            .await
            .err()
            .expect("invalid team store path should fail");

        assert!(matches!(err, WebError::Team(_)));
    }

    #[tokio::test]
    async fn get_workflow_returns_not_found_for_missing_team() {
        let err = get_workflow(State(make_context().state), Path("missing".into()))
            .await
            .err()
            .expect("missing workflow should fail");

        assert_eq!(err.into_response().status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn trigger_workflow_returns_not_found_for_missing_team() {
        let err = trigger_workflow(State(make_context().state), Path("missing".into()), None)
            .await
            .err()
            .expect("missing workflow should fail");

        assert_eq!(err.into_response().status(), StatusCode::NOT_FOUND);
    }
}
