use opengoose_profiles::ProfileStore;

use super::*;

#[test]
fn round_trip_chain_yaml() {
    let yaml = include_str!("../../teams/code-review.yaml");
    let team = TeamDefinition::from_yaml(yaml).unwrap();
    assert_eq!(team.name(), "code-review");
    assert_eq!(team.workflow, OrchestrationPattern::Chain);
    assert_eq!(team.agents.len(), 2);

    let serialized = team.to_yaml().unwrap();
    let reparsed = TeamDefinition::from_yaml(&serialized).unwrap();
    assert_eq!(reparsed.title, team.title);
}

#[test]
fn round_trip_fan_out_yaml() {
    let yaml = include_str!("../../teams/research-panel.yaml");
    let team = TeamDefinition::from_yaml(yaml).unwrap();
    assert_eq!(team.workflow, OrchestrationPattern::FanOut);
    assert!(team.fan_out.is_some());
}

#[test]
fn round_trip_router_yaml() {
    let yaml = include_str!("../../teams/smart-router.yaml");
    let team = TeamDefinition::from_yaml(yaml).unwrap();
    assert_eq!(team.workflow, OrchestrationPattern::Router);
    assert!(team.router.is_some());
}

#[test]
fn validation_rejects_empty_agents() {
    let yaml = r#"
version: "1.0.0"
title: "empty"
workflow: chain
agents: []
"#;
    let err = TeamDefinition::from_yaml(yaml).unwrap_err();
    assert!(err.to_string().contains("at least one agent"));
}

#[test]
fn validation_rejects_router_without_config() {
    let yaml = r#"
version: "1.0.0"
title: "bad-router"
workflow: router
agents:
  - profile: developer
"#;
    let err = TeamDefinition::from_yaml(yaml).unwrap_err();
    assert!(err.to_string().contains("router"));
}

#[test]
fn to_recipe_chain() {
    let (_tmp, store) = temp_store_with_defaults();
    let yaml = include_str!("../../teams/code-review.yaml");
    let team = TeamDefinition::from_yaml(yaml).unwrap();

    let recipe = team.to_recipe(&store);
    assert_eq!(recipe.title, "code-review");
    assert!(recipe.instructions.as_ref().unwrap().contains("sequence"));

    let subs = recipe.sub_recipes.unwrap();
    assert_eq!(subs.len(), 2);
    assert!(subs[0].sequential_when_repeated);

    // Must include summon extension
    let exts = recipe.extensions.unwrap();
    assert!(exts.iter().any(|e| e.name() == "summon"));
}

#[test]
fn to_recipe_fan_out() {
    let (_tmp, store) = temp_store_with_defaults();
    let yaml = include_str!("../../teams/research-panel.yaml");
    let team = TeamDefinition::from_yaml(yaml).unwrap();

    let recipe = team.to_recipe(&store);
    assert!(
        recipe
            .instructions
            .as_ref()
            .unwrap()
            .contains("simultaneously")
    );
    let subs = recipe.sub_recipes.unwrap();
    assert!(!subs[0].sequential_when_repeated);
}

#[test]
fn to_recipe_router() {
    let (_tmp, store) = temp_store_with_defaults();
    let yaml = include_str!("../../teams/smart-router.yaml");
    let team = TeamDefinition::from_yaml(yaml).unwrap();

    let recipe = team.to_recipe(&store);
    assert!(
        recipe
            .instructions
            .as_ref()
            .unwrap()
            .contains("most appropriate")
    );
}

#[test]
fn validation_rejects_empty_title() {
    let yaml = r#"
version: "1.0.0"
title: "   "
workflow: chain
agents:
  - profile: developer
"#;
    let err = TeamDefinition::from_yaml(yaml).unwrap_err();
    assert!(err.to_string().contains("title is required"));
}

#[test]
fn validation_rejects_empty_agent_profile() {
    let yaml = r#"
version: "1.0.0"
title: "test-team"
workflow: chain
agents:
  - profile: ""
"#;
    let err = TeamDefinition::from_yaml(yaml).unwrap_err();
    assert!(err.to_string().contains("profile name cannot be empty"));
}

#[test]
fn validation_rejects_fan_out_without_config() {
    let yaml = r#"
version: "1.0.0"
title: "bad-fanout"
workflow: fan-out
agents:
  - profile: developer
"#;
    let err = TeamDefinition::from_yaml(yaml).unwrap_err();
    assert!(err.to_string().contains("fan-out workflow requires"));
}

#[test]
fn test_name_returns_title() {
    let team = TeamDefinition {
        version: "1.0.0".into(),
        title: "my-team".into(),
        description: None,
        goal: None,
        workflow: OrchestrationPattern::Chain,
        agents: vec![TeamAgent {
            profile: "dev".into(),
            role: None,
        }],
        router: None,
        fan_out: None,
    };
    assert_eq!(team.name(), "my-team");
}

#[test]
fn test_file_name() {
    let team = TeamDefinition {
        version: "1.0.0".into(),
        title: "My Cool Team".into(),
        description: None,
        goal: None,
        workflow: OrchestrationPattern::Chain,
        agents: vec![TeamAgent {
            profile: "dev".into(),
            role: None,
        }],
        router: None,
        fan_out: None,
    };
    assert_eq!(team.file_name(), "my-cool-team.yaml");
}

#[test]
fn test_yaml_definition_trait_impl() {
    use opengoose_types::YamlDefinition;
    let yaml = include_str!("../../teams/code-review.yaml");
    let team = <TeamDefinition as YamlDefinition>::from_yaml(yaml).unwrap();
    assert_eq!(team.title(), "code-review");
    let roundtripped = team.to_yaml().unwrap();
    let reparsed = <TeamDefinition as YamlDefinition>::from_yaml(&roundtripped).unwrap();
    assert_eq!(reparsed.title(), team.title());
}

#[test]
fn test_orchestration_pattern_serde() {
    let yaml = r#"
version: "1.0.0"
title: "test"
workflow: fan-out
agents:
  - profile: dev
fan_out:
  merge_strategy: concatenate
"#;
    let team = TeamDefinition::from_yaml(yaml).unwrap();
    assert_eq!(team.workflow, OrchestrationPattern::FanOut);
}

#[test]
fn test_team_with_description() {
    let yaml = r#"
version: "1.0.0"
title: "described-team"
description: "A team for testing"
workflow: chain
agents:
  - profile: dev
    role: "develop features"
"#;
    let team = TeamDefinition::from_yaml(yaml).unwrap();
    assert_eq!(team.description, Some("A team for testing".into()));
    assert_eq!(team.agents[0].role, Some("develop features".into()));
}

fn temp_store_with_defaults() -> (tempfile::TempDir, ProfileStore) {
    let tmp = tempfile::tempdir().unwrap();
    let store = ProfileStore::with_dir(tmp.path().to_path_buf());
    store.install_defaults(false).unwrap();
    (tmp, store)
}
