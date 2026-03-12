use std::sync::Arc;

use opengoose_persistence::Database;
use opengoose_profiles::ProfileStore;
use opengoose_types::{EventBus, Platform, SessionKey};

use crate::runner::AgentOutput;
use crate::team::{OrchestrationPattern, TeamAgent, TeamDefinition};

use super::delegation::DelegationOutcome;
use super::helpers::{is_team_member, process_agent_communications};
use super::{MAX_DELEGATION_DEPTH, TeamOrchestrator};

fn test_team() -> TeamDefinition {
    TeamDefinition {
        version: "1.0.0".into(),
        title: "test-team".into(),
        description: None,
        goal: None,
        workflow: OrchestrationPattern::Chain,
        agents: vec![
            TeamAgent {
                profile: "coder".into(),
                role: Some("write code".into()),
            },
            TeamAgent {
                profile: "reviewer".into(),
                role: Some("review code".into()),
            },
        ],
        router: None,
        fan_out: None,
    }
}

fn test_ctx() -> crate::context::OrchestrationContext {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let bus = EventBus::new(16);
    let key = SessionKey::new(Platform::Discord, "g1", "ch1");
    let ctx = crate::context::OrchestrationContext::new("run-1".into(), key, db, bus);
    // Ensure session exists for FK constraints on message_queue
    ctx.sessions()
        .append_user_message(&ctx.session_key, "init", None)
        .unwrap();
    ctx
}

#[test]
fn test_is_team_member_found() {
    let team = test_team();
    assert!(is_team_member(&team, "coder"));
    assert!(is_team_member(&team, "reviewer"));
}

#[test]
fn test_is_team_member_not_found() {
    let team = test_team();
    assert!(!is_team_member(&team, "unknown"));
}

#[test]
fn test_process_agent_communications_broadcasts() {
    let team = test_team();
    let ctx = test_ctx();
    let output = AgentOutput {
        response: "done".into(),
        delegations: vec![],
        broadcasts: vec!["found bug".into()],
    };
    process_agent_communications(&team, &ctx, "sess1", "coder", &output);
    let broadcasts = ctx.read_broadcasts(None);
    assert_eq!(broadcasts.len(), 1);
    assert_eq!(broadcasts[0].content, "found bug");
}

#[test]
fn test_self_delegation_rejected() {
    let team = test_team();
    let ctx = test_ctx();
    let output = AgentOutput {
        response: "done".into(),
        delegations: vec![("coder".into(), "delegate to self".into())],
        broadcasts: vec![],
    };
    process_agent_communications(&team, &ctx, "sess1", "coder", &output);
    // Self-delegation should be rejected, so no messages in queue
    let msgs = ctx
        .queue()
        .dequeue_delegations(&ctx.team_run_id, 10)
        .unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_unknown_agent_delegation_rejected() {
    let team = test_team();
    let ctx = test_ctx();
    let output = AgentOutput {
        response: "done".into(),
        delegations: vec![("unknown_agent".into(), "do something".into())],
        broadcasts: vec![],
    };
    process_agent_communications(&team, &ctx, "sess1", "coder", &output);
    let msgs = ctx
        .queue()
        .dequeue_delegations(&ctx.team_run_id, 10)
        .unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_valid_delegation_enqueued() {
    let team = test_team();
    let ctx = test_ctx();
    let session_key = ctx.session_key.to_stable_id();
    let output = AgentOutput {
        response: "done".into(),
        delegations: vec![("reviewer".into(), "please review".into())],
        broadcasts: vec![],
    };
    process_agent_communications(&team, &ctx, &session_key, "coder", &output);
    let msgs = ctx
        .queue()
        .dequeue_delegations(&ctx.team_run_id, 10)
        .unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "please review");
    assert_eq!(msgs[0].recipient, "reviewer");
}

// ── Additional edge-case tests ──────────────────────────────────────

#[test]
fn test_multiple_delegations_mixed_valid_invalid() {
    let team = test_team();
    let ctx = test_ctx();
    let session_key = ctx.session_key.to_stable_id();
    let output = AgentOutput {
        response: "done".into(),
        delegations: vec![
            ("reviewer".into(), "valid delegation".into()),
            ("nonexistent".into(), "invalid delegation".into()),
            ("coder".into(), "self delegation".into()),
        ],
        broadcasts: vec![],
    };
    process_agent_communications(&team, &ctx, &session_key, "coder", &output);
    let msgs = ctx
        .queue()
        .dequeue_delegations(&ctx.team_run_id, 10)
        .unwrap();
    // Only the valid delegation to "reviewer" should be enqueued
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].recipient, "reviewer");
    assert_eq!(msgs[0].content, "valid delegation");
}

#[test]
fn test_empty_delegations_and_broadcasts() {
    let team = test_team();
    let ctx = test_ctx();
    let output = AgentOutput {
        response: "done".into(),
        delegations: vec![],
        broadcasts: vec![],
    };
    process_agent_communications(&team, &ctx, "sess1", "coder", &output);
    let msgs = ctx
        .queue()
        .dequeue_delegations(&ctx.team_run_id, 10)
        .unwrap();
    assert!(msgs.is_empty());
    let broadcasts = ctx.read_broadcasts(None);
    assert!(broadcasts.is_empty());
}

#[test]
fn test_multiple_broadcasts_all_recorded() {
    let team = test_team();
    let ctx = test_ctx();
    let output = AgentOutput {
        response: "done".into(),
        delegations: vec![],
        broadcasts: vec!["first update".into(), "second update".into()],
    };
    process_agent_communications(&team, &ctx, "sess1", "coder", &output);
    let broadcasts = ctx.read_broadcasts(None);
    assert_eq!(broadcasts.len(), 2);
}

#[test]
fn test_is_team_member_empty_team() {
    let team = TeamDefinition {
        version: "1.0.0".into(),
        title: "empty-team".into(),
        description: None,
        goal: None,
        workflow: OrchestrationPattern::Chain,
        agents: vec![],
        router: None,
        fan_out: None,
    };
    assert!(!is_team_member(&team, "anyone"));
}

// ── Dispatch & orchestrator construction tests ──────────────────────

fn test_profile_store() -> ProfileStore {
    let dir = std::env::temp_dir().join(format!("orch-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok();
    ProfileStore::with_dir(dir)
}

fn fan_out_team() -> TeamDefinition {
    TeamDefinition {
        version: "1.0.0".into(),
        title: "fan-out-team".into(),
        description: None,
        goal: None,
        workflow: OrchestrationPattern::FanOut,
        agents: vec![
            TeamAgent {
                profile: "a".into(),
                role: Some("role a".into()),
            },
            TeamAgent {
                profile: "b".into(),
                role: Some("role b".into()),
            },
        ],
        router: None,
        fan_out: None,
    }
}

fn router_team() -> TeamDefinition {
    TeamDefinition {
        version: "1.0.0".into(),
        title: "router-team".into(),
        description: None,
        goal: None,
        workflow: OrchestrationPattern::Router,
        agents: vec![TeamAgent {
            profile: "agent".into(),
            role: Some("handle".into()),
        }],
        router: None,
        fan_out: None,
    }
}

#[test]
fn test_orchestrator_new_default() {
    let team = test_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new(team, store);
    // Should construct without panic; internal pool starts empty
    drop(orch);
}

#[test]
fn test_orchestrator_new_with_model_override() {
    let team = test_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new_with_model_override(team, store, Some("gpt-4".to_string()));
    drop(orch);
}

#[test]
fn test_orchestrator_new_with_none_model_override() {
    let team = test_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new_with_model_override(team, store, None);
    drop(orch);
}

#[test]
fn test_delegation_outcome_default() {
    let outcome = DelegationOutcome::default();
    assert_eq!(outcome.succeeded, 0);
    assert_eq!(outcome.failed, 0);
}

#[test]
fn test_max_delegation_depth_is_reasonable() {
    // Ensure the constant hasn't been accidentally set to 0 or an unreasonably high value
    assert_ne!(MAX_DELEGATION_DEPTH, 0);
    assert!(
        MAX_DELEGATION_DEPTH <= 10,
        "depth {} exceeds expected max of 10",
        MAX_DELEGATION_DEPTH
    );
}

#[tokio::test]
async fn test_resume_rejects_fan_out_workflow() {
    let team = fan_out_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new(team, store);
    let ctx = test_ctx();

    let result = orch.resume(&ctx, 1).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("only chain workflows support resume"),
        "expected chain-only error, got: {err}"
    );
    assert!(
        err.contains("FanOut"),
        "error should mention the actual workflow type, got: {err}"
    );
}

#[tokio::test]
async fn test_resume_rejects_router_workflow() {
    let team = router_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new(team, store);
    let ctx = test_ctx();

    let result = orch.resume(&ctx, 1).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("only chain workflows support resume"),
        "expected chain-only error, got: {err}"
    );
    assert!(
        err.contains("Router"),
        "error should mention the actual workflow type, got: {err}"
    );
}

#[tokio::test]
async fn test_process_pending_delegations_empty_queue() {
    let team = test_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new(team, store);
    let ctx = test_ctx();

    // Create a parent work item for the delegation processing
    let session_key = ctx.session_key.to_stable_id();
    let parent_id = ctx
        .work_items()
        .create(&session_key, &ctx.team_run_id, "Test parent", None)
        .unwrap();

    let mut pool = std::collections::HashMap::new();
    let outcome = orch
        .process_pending_delegations(&ctx, parent_id, 0, &mut pool)
        .await
        .unwrap();

    assert_eq!(outcome.succeeded, 0);
    assert_eq!(outcome.failed, 0);
}

#[tokio::test]
async fn test_process_pending_delegations_max_depth_returns_empty() {
    let team = test_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new(team, store);
    let ctx = test_ctx();

    let session_key = ctx.session_key.to_stable_id();
    let parent_id = ctx
        .work_items()
        .create(&session_key, &ctx.team_run_id, "Test parent", None)
        .unwrap();

    let mut pool = std::collections::HashMap::new();
    // Call at max depth — should return immediately with empty outcome
    let outcome = orch
        .process_pending_delegations(&ctx, parent_id, MAX_DELEGATION_DEPTH, &mut pool)
        .await
        .unwrap();

    assert_eq!(outcome.succeeded, 0);
    assert_eq!(outcome.failed, 0);
}

#[tokio::test]
async fn test_execute_creates_run_record() {
    let team = test_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new(team, store);
    let ctx = test_ctx();

    // Execute will fail because profiles don't exist in the temp store,
    // but it should still create the orchestration run record before failing
    let _ = orch.execute("test input", &ctx).await;

    // The run record should exist (created at the start of execute)
    let run = ctx.orchestration().get_run(&ctx.team_run_id);
    assert!(
        run.is_ok(),
        "orchestration run should be created even if execution fails"
    );
}

#[tokio::test]
async fn test_execute_creates_parent_work_item() {
    let team = test_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new(team, store);
    let ctx = test_ctx();

    let _ = orch.execute("test input", &ctx).await;

    // A parent work item should have been created with the team name
    let items = ctx
        .work_items()
        .list_for_run(&ctx.team_run_id, None)
        .unwrap();
    assert!(
        !items.is_empty(),
        "at least one work item should be created"
    );
    let parent = &items[0];
    assert!(
        parent.title.contains("Team:"),
        "parent work item title should contain 'Team:'"
    );
}

#[tokio::test]
async fn test_execute_records_failure_on_missing_profile() {
    use opengoose_persistence::RunStatus;

    let team = test_team();
    let store = test_profile_store();
    let orch = TeamOrchestrator::new(team, store);
    let ctx = test_ctx();

    let result = orch.execute("test input", &ctx).await;
    assert!(
        result.is_err(),
        "execute should fail when profiles are missing"
    );

    // The run should be marked as failed
    let run = ctx.orchestration().get_run(&ctx.team_run_id).unwrap();
    assert!(run.is_some(), "run record should exist");
    let run = run.unwrap();
    assert!(
        matches!(run.status, RunStatus::Failed),
        "run should be marked failed, got: {:?}",
        run.status
    );
}
