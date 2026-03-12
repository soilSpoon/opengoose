use anyhow::{Context, Result};
use urlencoding::encode;

use super::catalog::TeamCatalogEntry;
use super::editor::build_team_editor;
use crate::data::utils::{choose_selected_name, source_badge};
use crate::data::views::{TeamListItem, TeamsPageView};

pub(super) fn build_teams_page(
    teams: &[TeamCatalogEntry],
    selected: Option<String>,
) -> Result<TeamsPageView> {
    let selected_name = selected_team_name(teams, selected);
    let selected_entry = teams
        .iter()
        .find(|entry| entry.name == selected_name)
        .context("selected team missing")?;

    Ok(TeamsPageView {
        mode_label: page_mode_label(teams).into(),
        mode_tone: page_mode_tone(teams),
        teams: build_team_list(teams, &selected_name),
        selected: build_team_editor(selected_entry, None)?,
    })
}

fn selected_team_name(teams: &[TeamCatalogEntry], selected: Option<String>) -> String {
    choose_selected_name(
        teams.iter().map(|entry| entry.name.clone()).collect(),
        selected,
    )
}

fn build_team_list(teams: &[TeamCatalogEntry], selected_name: &str) -> Vec<TeamListItem> {
    teams
        .iter()
        .map(|entry| TeamListItem {
            title: entry.name.clone(),
            subtitle: entry
                .team
                .description
                .clone()
                .unwrap_or_else(|| "No team description provided.".into()),
            members: entry
                .team
                .agents
                .iter()
                .map(|agent| agent.profile.clone())
                .collect::<Vec<_>>()
                .join(" · "),
            source_label: entry.source_label.clone(),
            source_badge: source_badge(&entry.source_label),
            page_url: format!("/teams?team={}", encode(&entry.name)),
            active: entry.name == selected_name,
        })
        .collect()
}

fn page_mode_label(teams: &[TeamCatalogEntry]) -> &'static str {
    if using_defaults(teams) {
        "Bundled defaults"
    } else {
        "Installed catalog"
    }
}

fn page_mode_tone(teams: &[TeamCatalogEntry]) -> &'static str {
    if using_defaults(teams) {
        "neutral"
    } else {
        "success"
    }
}

fn using_defaults(teams: &[TeamCatalogEntry]) -> bool {
    teams.iter().all(|team| !team.is_live)
}

#[cfg(test)]
mod tests {
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition};

    use super::*;

    fn entry(name: &str, is_live: bool) -> TeamCatalogEntry {
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
            source_label: if is_live {
                "/tmp/opengoose/teams".into()
            } else {
                "Bundled default".into()
            },
            is_live,
        }
    }

    #[test]
    fn build_teams_page_uses_bundled_default_mode() {
        let page = build_teams_page(&[entry("alpha", false), entry("beta", false)], None).unwrap();

        assert_eq!(page.mode_label, "Bundled defaults");
        assert_eq!(page.mode_tone, "neutral");
        assert_eq!(page.selected.title, "alpha");
        assert!(page.teams[0].active);
    }

    #[test]
    fn build_teams_page_marks_requested_team_active() {
        let page = build_teams_page(
            &[entry("alpha", true), entry("beta", true)],
            Some("beta".into()),
        )
        .unwrap();

        assert_eq!(page.mode_label, "Installed catalog");
        assert_eq!(page.mode_tone, "success");
        assert_eq!(page.selected.title, "beta");
        assert!(!page.teams[0].active);
        assert!(page.teams[1].active);
    }

    #[test]
    fn build_teams_page_falls_back_to_first_team_when_selection_is_missing() {
        let page = build_teams_page(
            &[entry("alpha", true), entry("beta", true)],
            Some("gamma".into()),
        )
        .unwrap();

        assert_eq!(page.selected.title, "alpha");
        assert!(page.teams[0].active);
    }
}
