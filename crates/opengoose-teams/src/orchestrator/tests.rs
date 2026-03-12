use std::sync::Arc;

use opengoose_persistence::Database;
use opengoose_types::{EventBus, Platform, SessionKey};

use crate::runner::AgentOutput;
use crate::team::{CommunicationMode, OrchestrationPattern, TeamAgent, TeamDefinition};

use super::helpers::{is_team_member, process_agent_communications};

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
        communication_mode: CommunicationMode::default(),
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
        communication_mode: CommunicationMode::default(),
    };
    assert!(!is_team_member(&team, "anyone"));
}
