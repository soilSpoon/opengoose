use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use opengoose_persistence::{Database, OrchestrationStore, RunStatus, WorkItemStore, WorkStatus};

use crate::data::views::{AgentMapAgentView, AgentMapView, MetricCard};

/// Load all data needed for the agent map page from the database.
pub fn load_agent_map(db: Arc<Database>) -> Result<AgentMapView> {
    let run_store = OrchestrationStore::new(db.clone());
    let work_store = WorkItemStore::new(db);

    let recent_runs = run_store.list_runs(None, 50)?;
    let running_count = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Running)
        .count();

    // Collect unique agent names from running runs
    let mut active_agents: Vec<String> = Vec::new();
    for run in &recent_runs {
        if run.status == RunStatus::Running {
            let items = work_store.list_for_run(&run.team_run_id, None)?;
            for item in &items {
                if item.status == WorkStatus::InProgress {
                    if let Some(agent) = &item.assigned_to {
                        if !active_agents.contains(agent) {
                            active_agents.push(agent.clone());
                        }
                    }
                }
            }
        }
    }

    let completed_count = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Completed)
        .count();
    let failed_count = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Failed)
        .count();
    let terminal = completed_count + failed_count;
    let success_rate = if terminal > 0 {
        (completed_count * 100) / terminal
    } else {
        0
    };

    let using_mock = recent_runs.is_empty();

    let agents: Vec<AgentMapAgentView> = if using_mock {
        mock_agents()
    } else {
        active_agents
            .iter()
            .map(|name| AgentMapAgentView {
                name: name.clone(),
                team: recent_runs
                    .iter()
                    .find(|r| r.status == RunStatus::Running)
                    .map(|r| r.team_name.clone())
                    .unwrap_or_default(),
                state_label: "Working".into(),
                state_tone: "cyan",
                elapsed: "—".into(),
            })
            .collect()
    };

    Ok(AgentMapView {
        mode_label: if using_mock {
            "Mock preview".into()
        } else {
            "Live runtime".into()
        },
        mode_tone: if using_mock { "neutral" } else { "success" },
        stream_summary: if using_mock {
            "The agent map is rendering seeded data so the monitoring layout can be reviewed before live traffic exists.".into()
        } else {
            "Server-sent events stream agent state snapshots from the runtime every two seconds."
                .into()
        },
        snapshot_label: format!("Snapshot {}", Utc::now().format("%H:%M:%S UTC")),
        metrics: vec![
            MetricCard {
                label: "Active runs".into(),
                value: if using_mock {
                    "2".into()
                } else {
                    running_count.to_string()
                },
                note: format!("{completed_count} completed recently"),
                tone: "cyan",
            },
            MetricCard {
                label: "Tracked agents".into(),
                value: if using_mock {
                    "3".into()
                } else {
                    agents.len().to_string()
                },
                note: "Currently monitored".into(),
                tone: "amber",
            },
            MetricCard {
                label: "Success rate".into(),
                value: if using_mock || terminal == 0 {
                    "—".into()
                } else {
                    format!("{success_rate}%")
                },
                note: format!("{completed_count} ok / {failed_count} failed"),
                tone: "sage",
            },
            MetricCard {
                label: "Witness".into(),
                value: "Active".into(),
                note: "Stuck: 300s / Zombie: 600s".into(),
                tone: "rose",
            },
        ],
        agents,
    })
}

fn mock_agents() -> Vec<AgentMapAgentView> {
    vec![
        AgentMapAgentView {
            name: "architect".into(),
            team: "feature-dev".into(),
            state_label: "Working".into(),
            state_tone: "cyan",
            elapsed: "2m 14s".into(),
        },
        AgentMapAgentView {
            name: "developer".into(),
            team: "feature-dev".into(),
            state_label: "Idle".into(),
            state_tone: "neutral",
            elapsed: "—".into(),
        },
        AgentMapAgentView {
            name: "reviewer".into(),
            team: "code-review".into(),
            state_label: "Working".into(),
            state_tone: "cyan",
            elapsed: "45s".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().expect("db should open"))
    }

    #[test]
    fn load_agent_map_returns_mock_for_empty_db() {
        let view = load_agent_map(test_db()).unwrap();
        assert_eq!(view.mode_label, "Mock preview");
        assert_eq!(view.agents.len(), 3);
        assert_eq!(view.metrics.len(), 4);
    }
}
