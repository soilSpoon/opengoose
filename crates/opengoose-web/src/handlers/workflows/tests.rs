use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use opengoose_teams::{
    FanOutConfig, MergeStrategy, OrchestrationPattern, TeamAgent, TeamDefinition,
};

use super::history::{status_label, workflow_name};
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
async fn list_workflows_returns_empty_for_empty_team_store() {
    let Json(items) = list_workflows(State(make_context().state))
        .await
        .expect("empty team store should succeed");

    assert!(items.is_empty());
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
async fn get_workflow_returns_empty_sections_for_plain_workflows() {
    let ctx = make_context();
    save_team(
        &ctx.state,
        &sample_team("triage", OrchestrationPattern::Chain),
    );

    let Json(detail) = get_workflow(State(ctx.state), Path("triage".into()))
        .await
        .expect("workflow detail should succeed");

    assert_eq!(detail.workflow, "chain");
    assert!(detail.automations.is_empty());
    assert!(detail.recent_runs.is_empty());
}

#[tokio::test]
async fn get_workflow_limits_recent_runs_to_six_entries() {
    let ctx = make_context();
    save_team(
        &ctx.state,
        &sample_team("feature-dev", OrchestrationPattern::Chain),
    );

    for idx in 0..7 {
        ctx.state
            .orchestration_store
            .create_run(
                &format!("run-{idx}"),
                "sess-feature",
                "feature-dev",
                "chain",
                "ship it",
                2,
            )
            .expect("run should be created");
    }

    let Json(detail) = get_workflow(State(ctx.state), Path("feature-dev".into()))
        .await
        .expect("workflow detail should succeed");

    assert_eq!(detail.recent_runs.len(), 6);
}

#[tokio::test]
async fn get_workflow_reports_disabled_automations_without_history() {
    let ctx = make_context();
    save_team(
        &ctx.state,
        &sample_team("feature-dev", OrchestrationPattern::Chain),
    );

    ctx.state
        .schedule_store
        .create("nightly", "0 0 * * *", "feature-dev", "", None)
        .expect("schedule should be created");
    ctx.state
        .schedule_store
        .set_enabled("nightly", false)
        .expect("schedule should be disabled");

    ctx.state
        .trigger_store
        .create("on-pr", "webhook_received", "{}", "feature-dev", "")
        .expect("trigger should be created");
    ctx.state
        .trigger_store
        .set_enabled("on-pr", false)
        .expect("trigger should be disabled");

    let Json(detail) = get_workflow(State(ctx.state), Path("feature-dev".into()))
        .await
        .expect("workflow detail should succeed");

    let schedule = detail
        .automations
        .iter()
        .find(|automation| automation.kind == "schedule")
        .expect("schedule automation should be present");
    assert!(!schedule.enabled);
    assert_eq!(schedule.note, "no executions recorded");

    let trigger = detail
        .automations
        .iter()
        .find(|automation| automation.kind == "trigger")
        .expect("trigger automation should be present");
    assert!(!trigger.enabled);
    assert_eq!(trigger.note, "0 total fire(s)");
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
async fn trigger_workflow_returns_team_title_in_response() {
    let ctx = make_context();
    let mut team = sample_team("Feature Delivery", OrchestrationPattern::Chain);
    team.title = "Feature Delivery".into();
    save_team(&ctx.state, &team);

    let (_, Json(response)) =
        trigger_workflow(State(ctx.state), Path("Feature Delivery".into()), None)
            .await
            .expect("manual trigger should succeed");

    assert_eq!(response.workflow, "Feature Delivery");
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
async fn trigger_workflow_uses_default_input_when_body_omits_input() {
    let ctx = make_context();
    save_team(
        &ctx.state,
        &sample_team("feature-dev", OrchestrationPattern::Chain),
    );

    let (_, Json(response)) = trigger_workflow(
        State(ctx.state),
        Path("feature-dev".into()),
        Some(Json(TriggerWorkflowRequest::default())),
    )
    .await
    .expect("manual trigger should succeed when input is omitted");

    assert_eq!(
        response.input,
        "Manual run requested from the web dashboard for feature-dev"
    );
}

#[test]
fn workflow_name_maps_supported_patterns() {
    assert_eq!(
        workflow_name(&sample_team("chain-team", OrchestrationPattern::Chain)),
        "chain"
    );
    assert_eq!(
        workflow_name(&sample_team("fan-team", OrchestrationPattern::FanOut)),
        "fan-out"
    );
    assert_eq!(
        workflow_name(&sample_team("router-team", OrchestrationPattern::Router)),
        "router"
    );
}

#[test]
fn status_label_title_cases_multiple_words() {
    assert_eq!(status_label("completed"), "Completed");
    assert_eq!(status_label("awaiting_review"), "Awaiting Review");
    assert_eq!(status_label(""), "");
}

#[tokio::test]
async fn list_workflows_propagates_team_store_errors() {
    let invalid_teams_path = unique_temp_path("workflows-teams-file");
    std::fs::write(&invalid_teams_path, "not a directory").expect("team file should be created");
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
