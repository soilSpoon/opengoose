use opengoose_persistence::{OrchestrationRun, Schedule, Trigger};
use opengoose_teams::TeamDefinition;

#[derive(Clone)]
pub(super) struct TeamCatalogEntry {
    pub(super) name: String,
    pub(super) team: TeamDefinition,
    pub(super) source_label: String,
    pub(super) is_live: bool,
}

#[derive(Clone)]
pub(super) struct WorkflowCatalogEntry {
    pub(super) name: String,
    pub(super) team: TeamDefinition,
    pub(super) source_label: String,
    pub(super) schedules: Vec<Schedule>,
    pub(super) triggers: Vec<Trigger>,
    pub(super) recent_runs: Vec<OrchestrationRun>,
}

pub(super) fn build_workflow_catalog(
    teams: &[TeamCatalogEntry],
    schedules: &[Schedule],
    triggers: &[Trigger],
    recent_runs: &[OrchestrationRun],
) -> Vec<WorkflowCatalogEntry> {
    teams
        .iter()
        .map(|entry| WorkflowCatalogEntry {
            name: entry.name.clone(),
            team: entry.team.clone(),
            source_label: entry.source_label.clone(),
            schedules: schedules
                .iter()
                .filter(|schedule| schedule.team_name == entry.name)
                .cloned()
                .collect(),
            triggers: triggers
                .iter()
                .filter(|trigger| trigger.team_name == entry.name)
                .cloned()
                .collect(),
            recent_runs: recent_runs
                .iter()
                .filter(|run| run.team_name == entry.name)
                .take(6)
                .cloned()
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use opengoose_persistence::RunStatus;
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition};

    use super::*;

    fn team_entry(name: &str) -> TeamCatalogEntry {
        TeamCatalogEntry {
            name: name.into(),
            team: TeamDefinition {
                version: "1.0.0".into(),
                title: name.into(),
                description: None,
                workflow: OrchestrationPattern::Chain,
                agents: vec![TeamAgent {
                    profile: format!("{name}-agent"),
                    role: None,
                }],
                router: None,
                fan_out: None,
                goal: None,
            },
            source_label: "Bundled default".into(),
            is_live: false,
        }
    }

    fn schedule(name: &str, team_name: &str) -> Schedule {
        Schedule {
            id: 1,
            name: name.into(),
            cron_expression: "0 0 * * *".into(),
            team_name: team_name.into(),
            input: String::new(),
            enabled: true,
            last_run_at: None,
            next_run_at: None,
            created_at: "2026-03-12 00:00:00".into(),
            updated_at: "2026-03-12 00:00:00".into(),
        }
    }

    fn trigger(name: &str, team_name: &str) -> Trigger {
        Trigger {
            id: 1,
            name: name.into(),
            trigger_type: "webhook_received".into(),
            condition_json: "{}".into(),
            team_name: team_name.into(),
            input: String::new(),
            enabled: true,
            last_fired_at: None,
            fire_count: 0,
            created_at: "2026-03-12 00:00:00".into(),
            updated_at: "2026-03-12 00:00:00".into(),
        }
    }

    fn run(team_run_id: &str, team_name: &str) -> OrchestrationRun {
        OrchestrationRun {
            team_run_id: team_run_id.into(),
            session_key: format!("session-{team_run_id}"),
            team_name: team_name.into(),
            workflow: "chain".into(),
            input: String::new(),
            status: RunStatus::Running,
            current_step: 1,
            total_steps: 2,
            result: None,
            created_at: "2026-03-12 00:00:00".into(),
            updated_at: "2026-03-12 00:00:00".into(),
        }
    }

    #[test]
    fn build_workflow_catalog_groups_records_and_caps_recent_runs() {
        let catalog = build_workflow_catalog(
            &[team_entry("alpha"), team_entry("beta")],
            &[
                schedule("nightly-alpha", "alpha"),
                schedule("nightly-beta", "beta"),
            ],
            &[
                trigger("alpha-webhook", "alpha"),
                trigger("beta-webhook", "beta"),
            ],
            &[
                run("alpha-1", "alpha"),
                run("alpha-2", "alpha"),
                run("alpha-3", "alpha"),
                run("alpha-4", "alpha"),
                run("alpha-5", "alpha"),
                run("alpha-6", "alpha"),
                run("alpha-7", "alpha"),
                run("beta-1", "beta"),
            ],
        );

        let alpha = catalog
            .iter()
            .find(|entry| entry.name == "alpha")
            .expect("alpha workflow should exist");
        let beta = catalog
            .iter()
            .find(|entry| entry.name == "beta")
            .expect("beta workflow should exist");

        assert_eq!(alpha.schedules.len(), 1);
        assert_eq!(alpha.triggers.len(), 1);
        assert_eq!(alpha.recent_runs.len(), 6);
        assert_eq!(alpha.recent_runs[0].team_run_id, "alpha-1");
        assert_eq!(alpha.recent_runs[5].team_run_id, "alpha-6");

        assert_eq!(beta.schedules.len(), 1);
        assert_eq!(beta.triggers.len(), 1);
        assert_eq!(beta.recent_runs.len(), 1);
        assert_eq!(beta.recent_runs[0].team_run_id, "beta-1");
    }
}
