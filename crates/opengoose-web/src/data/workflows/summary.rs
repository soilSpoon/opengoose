use opengoose_teams::{OrchestrationPattern, TeamDefinition};

use super::catalog::WorkflowCatalogEntry;
use crate::data::utils::run_tone;

pub(super) fn workflow_status(entry: &WorkflowCatalogEntry) -> (String, &'static str) {
    if let Some(run) = entry.recent_runs.first() {
        return (
            display_status_label(run.status.as_str()),
            run_tone(&run.status),
        );
    }

    if entry.schedules.iter().any(|schedule| schedule.enabled)
        || entry.triggers.iter().any(|trigger| trigger.enabled)
    {
        return ("Armed".into(), "amber");
    }

    ("Manual".into(), "neutral")
}

pub(super) fn automation_summary(entry: &WorkflowCatalogEntry) -> String {
    let enabled_schedules = entry
        .schedules
        .iter()
        .filter(|schedule| schedule.enabled)
        .count();
    let enabled_triggers = entry
        .triggers
        .iter()
        .filter(|trigger| trigger.enabled)
        .count();

    match (entry.schedules.len(), entry.triggers.len()) {
        (0, 0) => "Manual only".into(),
        _ => format!(
            "{} · {}",
            enabled_total_label(enabled_schedules, entry.schedules.len()),
            enabled_total_label(enabled_triggers, entry.triggers.len()),
        ),
    }
}

pub(super) fn team_agent_summary(team: &TeamDefinition) -> String {
    team.agents
        .iter()
        .map(|agent| agent.profile.clone())
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(super) fn enabled_total_label(enabled: usize, total: usize) -> String {
    if total == 0 {
        "0 configured".into()
    } else {
        format!("{enabled}/{total} enabled")
    }
}

pub(super) fn display_status_label(value: &str) -> String {
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

pub(super) fn step_prefix(pattern: &OrchestrationPattern, index: usize) -> String {
    match pattern {
        OrchestrationPattern::Chain => format!("Step {}", index + 1),
        OrchestrationPattern::FanOut => format!("Branch {}", index + 1),
        OrchestrationPattern::Router => format!("Route {}", index + 1),
    }
}

pub(super) fn step_badge(pattern: &OrchestrationPattern) -> &'static str {
    match pattern {
        OrchestrationPattern::Chain => "Sequential",
        OrchestrationPattern::FanOut => "Parallel",
        OrchestrationPattern::Router => "Candidate",
    }
}

pub(super) fn step_badge_tone(pattern: &OrchestrationPattern) -> &'static str {
    match pattern {
        OrchestrationPattern::Chain => "cyan",
        OrchestrationPattern::FanOut => "amber",
        OrchestrationPattern::Router => "sage",
    }
}

pub(super) trait WorkflowName {
    fn workflow_name(&self) -> String;
}

impl WorkflowName for TeamDefinition {
    fn workflow_name(&self) -> String {
        match self.workflow {
            OrchestrationPattern::Chain => "Chain".into(),
            OrchestrationPattern::FanOut => "Fan-out".into(),
            OrchestrationPattern::Router => "Router".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition};

    use super::*;
    use crate::data::workflows::catalog::WorkflowCatalogEntry;

    fn minimal_team(title: &str) -> TeamDefinition {
        TeamDefinition {
            version: "1.0.0".into(),
            title: title.into(),
            description: None,
            workflow: OrchestrationPattern::Chain,
            agents: vec![TeamAgent {
                profile: "agent-a".into(),
                role: None,
            }],
            router: None,
            fan_out: None,
            goal: None,
        }
    }

    fn minimal_entry() -> WorkflowCatalogEntry {
        WorkflowCatalogEntry {
            name: "test".into(),
            team: minimal_team("test"),
            source_label: "Bundled default".into(),
            schedules: vec![],
            triggers: vec![],
            recent_runs: vec![],
        }
    }

    #[test]
    fn display_status_label_single_word() {
        assert_eq!(display_status_label("running"), "Running");
    }

    #[test]
    fn display_status_label_underscored() {
        assert_eq!(display_status_label("in_progress"), "In Progress");
    }

    #[test]
    fn display_status_label_empty() {
        assert_eq!(display_status_label(""), "");
    }

    #[test]
    fn enabled_total_label_zero() {
        assert_eq!(enabled_total_label(0, 0), "0 configured");
    }

    #[test]
    fn enabled_total_label_some() {
        assert_eq!(enabled_total_label(2, 3), "2/3 enabled");
    }

    #[test]
    fn step_prefix_chain() {
        assert_eq!(step_prefix(&OrchestrationPattern::Chain, 0), "Step 1");
    }

    #[test]
    fn step_prefix_fan_out() {
        assert_eq!(step_prefix(&OrchestrationPattern::FanOut, 2), "Branch 3");
    }

    #[test]
    fn step_prefix_router() {
        assert_eq!(step_prefix(&OrchestrationPattern::Router, 0), "Route 1");
    }

    #[test]
    fn step_badge_labels() {
        assert_eq!(step_badge(&OrchestrationPattern::Chain), "Sequential");
        assert_eq!(step_badge(&OrchestrationPattern::FanOut), "Parallel");
        assert_eq!(step_badge(&OrchestrationPattern::Router), "Candidate");
    }

    #[test]
    fn step_badge_tone_values() {
        assert_eq!(step_badge_tone(&OrchestrationPattern::Chain), "cyan");
        assert_eq!(step_badge_tone(&OrchestrationPattern::FanOut), "amber");
        assert_eq!(step_badge_tone(&OrchestrationPattern::Router), "sage");
    }

    #[test]
    fn workflow_status_manual_when_no_runs_or_automations() {
        let (label, tone) = workflow_status(&minimal_entry());
        assert_eq!(label, "Manual");
        assert_eq!(tone, "neutral");
    }

    #[test]
    fn automation_summary_manual_only() {
        assert_eq!(automation_summary(&minimal_entry()), "Manual only");
    }

    #[test]
    fn team_agent_summary_joins_profiles() {
        let mut team = minimal_team("test");
        team.agents.push(TeamAgent {
            profile: "agent-b".into(),
            role: None,
        });
        assert_eq!(team_agent_summary(&team), "agent-a · agent-b");
    }
}
