use anyhow::Result;
use opengoose_teams::{OrchestrationPattern, TeamDefinition, TeamError, TeamStore};

use super::catalog::TeamCatalogEntry;
use crate::data::views::{Notice, TeamEditorView};

pub(super) fn save_team_yaml(original_name: String, yaml: String) -> Result<TeamEditorView> {
    let store = TeamStore::new()?;
    save_team_yaml_with_store(&store, original_name, yaml)
}

pub(super) fn build_team_editor(
    entry: &TeamCatalogEntry,
    notice: Option<Notice>,
) -> Result<TeamEditorView> {
    Ok(TeamEditorView {
        title: entry.name.clone(),
        subtitle: team_subtitle(&entry.team),
        source_label: entry.source_label.clone(),
        workflow_label: workflow_label(&entry.team).into(),
        members_text: team_members_text(&entry.team, ", "),
        original_name: entry.name.clone(),
        yaml: entry.team.to_yaml()?,
        notice,
    })
}

fn save_team_yaml_with_store(
    store: &TeamStore,
    original_name: String,
    yaml: String,
) -> Result<TeamEditorView> {
    match TeamDefinition::from_yaml(&yaml) {
        Ok(team) => persist_team_yaml(store, &original_name, team),
        Err(error) => Ok(validation_error_view(
            original_name,
            yaml,
            error.to_string(),
        )),
    }
}

fn persist_team_yaml(
    store: &TeamStore,
    original_name: &str,
    team: TeamDefinition,
) -> Result<TeamEditorView> {
    remove_renamed_team(store, original_name, &team)?;
    store.save(&team, true)?;

    let entry = TeamCatalogEntry {
        name: team.name().to_string(),
        team,
        source_label: format!("Saved in {}", store.dir().display()),
        is_live: true,
    };

    build_team_editor(&entry, Some(saved_notice()))
}

fn remove_renamed_team(
    store: &TeamStore,
    original_name: &str,
    team: &TeamDefinition,
) -> Result<()> {
    if team.name() == original_name {
        return Ok(());
    }

    match store.remove(original_name) {
        Ok(()) | Err(TeamError::NotFound(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validation_error_view(original_name: String, yaml: String, error: String) -> TeamEditorView {
    TeamEditorView {
        title: original_name.clone(),
        subtitle: "Fix the YAML validation error and try again.".into(),
        source_label: "Editor draft".into(),
        workflow_label: "Unparsed".into(),
        members_text: "No members parsed".into(),
        original_name,
        yaml,
        notice: Some(Notice {
            text: error,
            tone: "danger",
        }),
    }
}

fn saved_notice() -> Notice {
    Notice {
        text: "Team definition saved.".into(),
        tone: "success",
    }
}

fn team_subtitle(team: &TeamDefinition) -> String {
    team.description
        .clone()
        .unwrap_or_else(|| "No team description provided.".into())
}

fn team_members_text(team: &TeamDefinition, separator: &str) -> String {
    team.agents
        .iter()
        .map(|agent| agent.profile.clone())
        .collect::<Vec<_>>()
        .join(separator)
}

fn workflow_label(team: &TeamDefinition) -> &'static str {
    match team.workflow {
        OrchestrationPattern::Chain => "Chain",
        OrchestrationPattern::FanOut => "Fan-out",
        OrchestrationPattern::Router => "Router",
    }
}

#[cfg(test)]
mod tests {
    use opengoose_teams::{CommunicationMode, TeamAgent};

    use super::*;

    fn minimal_team(title: &str, workflow: OrchestrationPattern) -> TeamDefinition {
        TeamDefinition {
            version: "1.0.0".into(),
            title: title.into(),
            description: None,
            workflow,
            communication_mode: CommunicationMode::default(),
            agents: vec![TeamAgent {
                profile: "agent-a".into(),
                role: None,
            }],
            router: None,
            fan_out: None,
            goal: None,
        }
    }

    fn entry(team: TeamDefinition) -> TeamCatalogEntry {
        TeamCatalogEntry {
            name: team.name().to_string(),
            team,
            source_label: "Bundled default".into(),
            is_live: false,
        }
    }

    fn temp_store() -> (tempfile::TempDir, TeamStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = TeamStore::with_dir(tmp.path().to_path_buf());
        (tmp, store)
    }

    #[test]
    fn workflow_label_chain() {
        let team = minimal_team("t", OrchestrationPattern::Chain);
        assert_eq!(workflow_label(&team), "Chain");
    }

    #[test]
    fn workflow_label_fan_out() {
        let team = minimal_team("t", OrchestrationPattern::FanOut);
        assert_eq!(workflow_label(&team), "Fan-out");
    }

    #[test]
    fn workflow_label_router() {
        let team = minimal_team("t", OrchestrationPattern::Router);
        assert_eq!(workflow_label(&team), "Router");
    }

    #[test]
    fn build_team_editor_title_and_workflow_label() {
        let view = build_team_editor(
            &entry(minimal_team("my-team", OrchestrationPattern::Chain)),
            None,
        )
        .unwrap();
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

        let view = build_team_editor(&entry(team), None).unwrap();
        assert!(view.members_text.contains("agent-a"));
        assert!(view.members_text.contains("agent-b"));
    }

    #[test]
    fn build_team_editor_description_fallback() {
        let view = build_team_editor(&entry(minimal_team("t", OrchestrationPattern::Chain)), None)
            .unwrap();
        assert!(view.subtitle.contains("No team description"));
    }

    #[test]
    fn build_team_editor_description_shown_when_provided() {
        let mut team = minimal_team("t", OrchestrationPattern::Chain);
        team.description = Some("Does things".into());

        let view = build_team_editor(&entry(team), None).unwrap();
        assert_eq!(view.subtitle, "Does things");
    }

    #[test]
    fn build_team_editor_passes_notice_through() {
        let notice = Notice {
            text: "Saved!".into(),
            tone: "success",
        };

        let view = build_team_editor(
            &entry(minimal_team("t", OrchestrationPattern::Chain)),
            Some(notice),
        )
        .unwrap();

        let notice = view.notice.unwrap();
        assert_eq!(notice.text, "Saved!");
        assert_eq!(notice.tone, "success");
    }

    #[test]
    fn build_team_editor_no_notice_is_none() {
        let view = build_team_editor(&entry(minimal_team("t", OrchestrationPattern::Chain)), None)
            .unwrap();
        assert!(view.notice.is_none());
    }

    #[test]
    fn build_team_editor_original_name_matches_title() {
        let view = build_team_editor(
            &entry(minimal_team("ops-team", OrchestrationPattern::Router)),
            None,
        )
        .unwrap();
        assert_eq!(view.original_name, "ops-team");
    }

    #[test]
    fn save_team_yaml_returns_validation_notice_for_invalid_yaml() {
        let (_tmp, store) = temp_store();
        let view = save_team_yaml_with_store(&store, "broken".into(), "title:".into()).unwrap();

        let notice = view.notice.expect("invalid yaml should surface a notice");
        assert_eq!(notice.tone, "danger");
        assert_eq!(view.title, "broken");
        assert_eq!(view.workflow_label, "Unparsed");
    }

    #[test]
    fn save_team_yaml_with_store_renames_existing_team() {
        let (_tmp, store) = temp_store();
        let team = minimal_team("alpha", OrchestrationPattern::Chain);
        store.save(&team, true).unwrap();

        let mut renamed = team.clone();
        renamed.title = "beta".into();

        let view =
            save_team_yaml_with_store(&store, "alpha".into(), renamed.to_yaml().unwrap()).unwrap();

        assert!(store.get("alpha").is_err());
        assert_eq!(store.get("beta").unwrap().title, "beta");
        assert_eq!(view.title, "beta");
        assert_eq!(view.notice.unwrap().tone, "success");
    }
}
