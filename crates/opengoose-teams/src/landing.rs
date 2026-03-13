//! Landing Protocol — cleanup and reporting when an agent finishes its work.
//!
//! Called after a team step or run completes. The protocol:
//! 1. Identifies in-progress work items without output (warnings)
//! 2. Purges completed ephemeral wisps
//! 3. Builds a landing report
//! 4. Emits an `AgentLanding` event

use anyhow::Result;
use serde::Serialize;
use tracing::{debug, warn};

use opengoose_persistence::WorkStatus;
use opengoose_types::AppEventKind;

use crate::context::OrchestrationContext;

/// Summary produced when an agent lands (finishes its work).
#[derive(Debug, Clone, Serialize)]
pub struct LandingReport {
    /// Agent that landed.
    pub agent: String,
    /// Team the agent belongs to.
    pub team: String,
    /// Orchestration run ID.
    pub team_run_id: String,
    /// Titles of in-progress items without output (potential issues).
    pub incomplete_items: Vec<String>,
    /// Number of ephemeral wisps purged.
    pub wisps_purged: usize,
}

/// Execute the landing protocol for a single agent.
///
/// Should be called after each agent completes its step in a chain or fan-out.
pub fn land(
    ctx: &OrchestrationContext,
    team_name: &str,
    agent_name: &str,
) -> Result<LandingReport> {
    let work_items = ctx.work_items();

    // 1. Find in-progress items without output (potential abandoned work)
    let all_items = work_items.list_for_run(&ctx.team_run_id, None);
    let incomplete_items: Vec<String> = all_items
        .iter()
        .filter(|item| {
            item.status == WorkStatus::InProgress
                && item.output.is_none()
                && item.assigned_to.as_deref().is_some_and(|a| a == agent_name)
        })
        .map(|item| item.title.clone())
        .collect();

    if !incomplete_items.is_empty() {
        warn!(
            agent = agent_name,
            team = team_name,
            count = incomplete_items.len(),
            "agent landing with incomplete work items"
        );
    }

    // 2. Purge completed ephemeral wisps
    let wisps_purged = work_items.purge_ephemeral(&ctx.team_run_id);

    let report = LandingReport {
        agent: agent_name.to_string(),
        team: team_name.to_string(),
        team_run_id: ctx.team_run_id.clone(),
        incomplete_items,
        wisps_purged,
    };

    debug!(
        agent = agent_name,
        team = team_name,
        wisps_purged = report.wisps_purged,
        incomplete = report.incomplete_items.len(),
        "agent landed"
    );

    // 3. Emit landing event
    ctx.emit(AppEventKind::AgentLanding {
        team: team_name.to_string(),
        agent: agent_name.to_string(),
    });

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use opengoose_persistence::Database;
    use opengoose_types::{EventBus, Platform, SessionKey};

    fn test_ctx() -> OrchestrationContext {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let bus = EventBus::new(16);
        let key = SessionKey::new(Platform::Discord, "g1", "ch1");
        let ctx = OrchestrationContext::new("run-1".into(), key, db, bus);
        ctx.sessions()
            .append_user_message(&ctx.session_key, "init", None)
            .unwrap();
        ctx
    }

    #[test]
    fn landing_report_for_clean_agent() {
        let ctx = test_ctx();
        let report = land(&ctx, "my-team", "developer").unwrap();

        assert_eq!(report.agent, "developer");
        assert_eq!(report.team, "my-team");
        assert!(report.incomplete_items.is_empty());
        assert_eq!(report.wisps_purged, 0);
    }

    #[test]
    fn landing_detects_incomplete_items() {
        let ctx = test_ctx();
        let session_id = ctx.session_key.to_stable_id();

        // Create an in-progress item assigned to developer without output
        let id = ctx
            .work_items()
            .create(&session_id, &ctx.team_run_id, "Review code", None);
        ctx.work_items().assign(&id, "developer", Some(0));

        let report = land(&ctx, "my-team", "developer").unwrap();
        assert_eq!(report.incomplete_items.len(), 1);
        assert!(report.incomplete_items[0].contains("Review code"));
    }

    #[test]
    fn landing_emits_event() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let key = SessionKey::new(Platform::Discord, "g1", "ch1");
        let ctx = OrchestrationContext::new("run-1".into(), key, db, bus);

        let _report = land(&ctx, "my-team", "dev").unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let event = rx.recv().await.unwrap();
            assert!(matches!(
                event.kind,
                AppEventKind::AgentLanding { ref team, ref agent }
                if team == "my-team" && agent == "dev"
            ));
        });
    }
}
