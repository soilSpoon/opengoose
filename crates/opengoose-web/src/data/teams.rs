use anyhow::{Context, Result, anyhow};
use opengoose_teams::{TeamDefinition, TeamStore, all_defaults as default_teams};
use urlencoding::encode;

use crate::data::utils::choose_selected_name;
use crate::data::views::{Notice, TeamEditorView, TeamListItem, TeamsPageView};

#[derive(Clone)]
struct TeamCatalogEntry {
    team: TeamDefinition,
    source_label: String,
    is_live: bool,
}

/// Load the teams page view-model, optionally selecting a team by name.
pub fn load_teams_page(selected: Option<String>) -> Result<TeamsPageView> {
    let teams = load_teams_catalog()?;
    let using_defaults = teams.iter().all(|team| !team.is_live);
    let selected_name = choose_selected_name(
        teams.iter().map(|item| item.team.title.clone()).collect(),
        selected,
    );

    Ok(TeamsPageView {
        mode_label: if using_defaults {
            "Bundled defaults".into()
        } else {
            "Installed catalog".into()
        },
        mode_tone: if using_defaults { "neutral" } else { "success" },
        teams: teams
            .iter()
            .map(|entry| TeamListItem {
                title: entry.team.title.clone(),
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
                page_url: format!("/teams?team={}", encode(&entry.team.title)),
                active: entry.team.title == selected_name,
            })
            .collect(),
        selected: build_team_editor(
            teams
                .iter()
                .find(|entry| entry.team.title == selected_name)
                .context("selected team missing")?,
            None,
        )?,
    })
}

/// Load the YAML editor panel for a single team definition.
pub fn load_team_editor(selected: Option<String>) -> Result<TeamEditorView> {
    Ok(load_teams_page(selected)?.selected)
}

/// Save edited team YAML and return the refreshed editor view.
pub fn save_team_yaml(original_name: String, yaml: String) -> Result<TeamEditorView> {
    let parsed = TeamDefinition::from_yaml(&yaml);
    match parsed {
        Ok(team) => {
            let store = TeamStore::new()?;
            if team.title != original_name
                && let Err(error) = store.remove(&original_name)
                && !error.to_string().contains("not found")
            {
                return Err(anyhow!(error));
            }
            store.save(&team, true)?;
            let entry = TeamCatalogEntry {
                team,
                source_label: format!("Saved in {}", store.dir().display()),
                is_live: true,
            };
            build_team_editor(
                &entry,
                Some(Notice {
                    text: "Team definition saved.".into(),
                    tone: "success",
                }),
            )
        }
        Err(error) => Ok(TeamEditorView {
            title: original_name.clone(),
            subtitle: "Fix the YAML validation error and try again.".into(),
            source_label: "Editor draft".into(),
            workflow_label: "Unparsed".into(),
            members_text: "No members parsed".into(),
            original_name,
            yaml,
            notice: Some(Notice {
                text: error.to_string(),
                tone: "danger",
            }),
        }),
    }
}

fn load_teams_catalog() -> Result<Vec<TeamCatalogEntry>> {
    let store = TeamStore::new()?;
    let names = store.list()?;
    if names.is_empty() {
        return Ok(default_teams()
            .into_iter()
            .map(|team| TeamCatalogEntry {
                team,
                source_label: "Bundled default".into(),
                is_live: false,
            })
            .collect());
    }

    names
        .into_iter()
        .map(|name| {
            let team = store.get(&name)?;
            Ok(TeamCatalogEntry {
                team,
                source_label: format!("{}", store.dir().display()),
                is_live: true,
            })
        })
        .collect()
}

fn build_team_editor(entry: &TeamCatalogEntry, notice: Option<Notice>) -> Result<TeamEditorView> {
    Ok(TeamEditorView {
        title: entry.team.title.clone(),
        subtitle: entry
            .team
            .description
            .clone()
            .unwrap_or_else(|| "No team description provided.".into()),
        source_label: entry.source_label.clone(),
        workflow_label: entry.team.workflow_name(),
        members_text: entry
            .team
            .agents
            .iter()
            .map(|agent| agent.profile.clone())
            .collect::<Vec<_>>()
            .join(", "),
        original_name: entry.team.title.clone(),
        yaml: entry.team.to_yaml()?,
        notice,
    })
}

#[cfg(test)]
mod tests {
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition};

    use super::*;

    fn minimal_team(title: &str, workflow: OrchestrationPattern) -> TeamDefinition {
        TeamDefinition {
            version: "1.0.0".into(),
            title: title.into(),
            description: None,
            workflow,
            agents: vec![TeamAgent {
                profile: "agent-a".into(),
                role: None,
            }],
            router: None,
            fan_out: None,
        }
    }

    // --- WorkflowName trait ---

    #[test]
    fn workflow_name_chain() {
        let team = minimal_team("t", OrchestrationPattern::Chain);
        assert_eq!(team.workflow_name(), "Chain");
    }

    #[test]
    fn workflow_name_fan_out() {
        let team = minimal_team("t", OrchestrationPattern::FanOut);
        assert_eq!(team.workflow_name(), "Fan-out");
    }

    #[test]
    fn workflow_name_router() {
        let team = minimal_team("t", OrchestrationPattern::Router);
        assert_eq!(team.workflow_name(), "Router");
    }

    // --- build_team_editor ---

    #[test]
    fn build_team_editor_title_and_workflow_label() {
        let entry = TeamCatalogEntry {
            team: minimal_team("my-team", OrchestrationPattern::Chain),
            source_label: "Bundled default".into(),
            is_live: false,
        };
        let view = build_team_editor(&entry, None).unwrap();
        assert_eq!(view.title, "my-team");
        assert_eq!(view.workflow_label, "Chain");
    }

    #[test]
    fn build_team_editor_members_text_joined() {
        let mut team = minimal_team("my-team", OrchestrationPattern::FanOut);
        team.agents.push(TeamAgent {
            profile: "agent-b".into(),
            role: None,
        });
        let entry = TeamCatalogEntry {
            team,
            source_label: "Bundled default".into(),
            is_live: false,
        };
        let view = build_team_editor(&entry, None).unwrap();
        assert!(view.members_text.contains("agent-a"));
        assert!(view.members_text.contains("agent-b"));
    }

    #[test]
    fn build_team_editor_description_fallback() {
        let entry = TeamCatalogEntry {
            team: minimal_team("t", OrchestrationPattern::Chain),
            source_label: "Bundled default".into(),
            is_live: false,
        };
        let view = build_team_editor(&entry, None).unwrap();
        assert!(view.subtitle.contains("No team description"));
    }

    #[test]
    fn build_team_editor_description_shown_when_provided() {
        let mut team = minimal_team("t", OrchestrationPattern::Chain);
        team.description = Some("Does things".into());
        let entry = TeamCatalogEntry {
            team,
            source_label: "Bundled default".into(),
            is_live: false,
        };
        let view = build_team_editor(&entry, None).unwrap();
        assert_eq!(view.subtitle, "Does things");
    }

    #[test]
    fn build_team_editor_passes_notice_through() {
        let entry = TeamCatalogEntry {
            team: minimal_team("t", OrchestrationPattern::Chain),
            source_label: "Bundled default".into(),
            is_live: false,
        };
        let notice = Notice {
            text: "Saved!".into(),
            tone: "success",
        };
        let view = build_team_editor(&entry, Some(notice)).unwrap();
        let n = view.notice.unwrap();
        assert_eq!(n.text, "Saved!");
        assert_eq!(n.tone, "success");
    }

    #[test]
    fn build_team_editor_no_notice_is_none() {
        let entry = TeamCatalogEntry {
            team: minimal_team("t", OrchestrationPattern::Chain),
            source_label: "Bundled default".into(),
            is_live: false,
        };
        let view = build_team_editor(&entry, None).unwrap();
        assert!(view.notice.is_none());
    }

    #[test]
    fn build_team_editor_original_name_matches_title() {
        let entry = TeamCatalogEntry {
            team: minimal_team("ops-team", OrchestrationPattern::Router),
            source_label: "Installed catalog".into(),
            is_live: true,
        };
        let view = build_team_editor(&entry, None).unwrap();
        assert_eq!(view.original_name, "ops-team");
    }
}

trait WorkflowName {
    fn workflow_name(&self) -> String;
}

impl WorkflowName for TeamDefinition {
    fn workflow_name(&self) -> String {
        match self.workflow {
            opengoose_teams::OrchestrationPattern::Chain => "Chain".into(),
            opengoose_teams::OrchestrationPattern::FanOut => "Fan-out".into(),
            opengoose_teams::OrchestrationPattern::Router => "Router".into(),
        }
    }
}
