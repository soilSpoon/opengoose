use std::fs;

use serde_json::Value;

use crate::support::{CliHarness, assert_runtime_error_message, stdout, stdout_json};

#[test]
fn profile_commands_work_end_to_end() {
    let harness = CliHarness::new();

    let empty_list = harness.run(&["profile", "list"]);
    assert!(empty_list.status.success());
    assert!(stdout(&empty_list).contains("No profiles found."));

    let init = harness.run(&["profile", "init"]);
    assert!(init.status.success());
    assert!(stdout(&init).contains("default profile(s)."));

    let second_init = harness.run(&["profile", "init"]);
    assert!(second_init.status.success());
    assert!(stdout(&second_init).contains("All default profiles already exist."));

    let list = harness.run(&["profile", "list"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("Profiles"));
    assert!(list_stdout.contains("developer"));
    assert!(list_stdout.contains("reviewer"));

    let show = harness.run(&["profile", "show", "developer"]);
    assert!(show.status.success());
    let show_stdout = stdout(&show);
    assert!(show_stdout.contains("title: developer"));
    assert!(show_stdout.contains("description:"));

    let remove = harness.run(&["profile", "remove", "developer"]);
    assert!(remove.status.success());
    assert!(stdout(&remove).contains("Removed profile `developer`."));

    let missing = harness.run(&["profile", "show", "developer"]);
    assert!(!missing.status.success());
    assert!(crate::support::stderr(&missing).contains("profile `developer` not found"));
}

#[test]
fn profile_add_loads_custom_yaml_file() {
    let harness = CliHarness::new();
    let profile_path = harness.home().join("custom-profile.yaml");
    fs::write(
        &profile_path,
        r#"version: "1.0.0"
title: "custom-profile"
description: "Custom profile"
prompt: "Be useful"
"#,
    )
    .unwrap();

    let add = harness.run(&["profile", "add", profile_path.to_str().unwrap()]);
    assert!(add.status.success());
    assert!(stdout(&add).contains("Added profile `custom-profile`."));

    let show = harness.run(&["profile", "show", "custom-profile"]);
    assert!(show.status.success());
    assert!(stdout(&show).contains("title: custom-profile"));
}

#[test]
fn team_commands_work_end_to_end() {
    let harness = CliHarness::new();

    let empty_list = harness.run(&["team", "list"]);
    assert!(empty_list.status.success());
    assert!(stdout(&empty_list).contains("No teams found."));

    let init = harness.run(&["team", "init"]);
    assert!(init.status.success());
    assert!(stdout(&init).contains("default team(s)."));

    let second_init = harness.run(&["team", "init"]);
    assert!(second_init.status.success());
    assert!(stdout(&second_init).contains("All default teams already exist."));

    let list = harness.run(&["team", "list"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("Teams"));
    assert!(list_stdout.contains("code-review"));
    assert!(list_stdout.contains("smart-router"));

    let show = harness.run(&["team", "show", "code-review"]);
    assert!(show.status.success());
    let show_stdout = stdout(&show);
    assert!(show_stdout.contains("title: code-review"));
    assert!(show_stdout.contains("workflow: chain"));

    let remove = harness.run(&["team", "remove", "code-review"]);
    assert!(remove.status.success());
    assert!(stdout(&remove).contains("Removed team `code-review`."));

    let missing = harness.run(&["team", "show", "code-review"]);
    assert!(!missing.status.success());
    assert!(crate::support::stderr(&missing).contains("team `code-review` not found"));
}

#[test]
fn team_add_loads_custom_yaml_file() {
    let harness = CliHarness::new();
    let team_path = harness.home().join("custom-team.yaml");
    fs::write(
        &team_path,
        r#"version: "1.0.0"
title: "custom-team"
description: "Custom team"
workflow: chain
agents:
  - profile: developer
    role: "Implement the change"
"#,
    )
    .unwrap();

    let add = harness.run(&["team", "add", team_path.to_str().unwrap()]);
    assert!(add.status.success());
    assert!(stdout(&add).contains("Added team `custom-team`."));

    let show = harness.run(&["team", "show", "custom-team"]);
    assert!(show.status.success());
    assert!(stdout(&show).contains("title: custom-team"));
}

#[test]
fn team_show_json_reports_router_configuration() {
    let harness = CliHarness::new();
    let team_path = harness.home().join("router-team.yaml");
    fs::write(
        &team_path,
        r#"version: "1.0.0"
title: "router-team"
description: "Route prompts to the best agent"
workflow: router
agents:
  - profile: triager
    role: "Select the specialist"
router:
  strategy: content-based
"#,
    )
    .unwrap();

    let add = harness.run(&["team", "add", team_path.to_str().unwrap()]);
    assert!(add.status.success());

    let show = harness.run(&["--json", "team", "show", "router-team"]);
    assert!(show.status.success());

    let payload = stdout_json(&show);
    assert_eq!(payload["ok"], Value::Bool(true));
    assert_eq!(payload["team"]["title"], Value::from("router-team"));
    assert_eq!(payload["team"]["workflow"], Value::from("router"));
    assert_eq!(
        payload["team"]["router"]["strategy"],
        Value::from("content-based")
    );
}

#[test]
fn team_show_missing_team_reports_structured_error() {
    let harness = CliHarness::new();

    let output = harness.run(&["--json", "team", "show", "missing-team"]);
    assert_runtime_error_message(&output, "not_found", "team `missing-team` not found");
}

#[test]
fn team_remove_json_reports_removed_flag_for_existing_team() {
    let harness = CliHarness::new();

    let init = harness.run(&["team", "init"]);
    assert!(init.status.success());

    let remove = harness.run(&["--json", "team", "remove", "code-review"]);
    assert!(remove.status.success());

    let payload = stdout_json(&remove);
    assert_eq!(payload["ok"], Value::Bool(true));
    assert_eq!(payload["command"], Value::from("team.remove"));
    assert_eq!(payload["team"], Value::from("code-review"));
    assert_eq!(payload["removed"], Value::Bool(true));
}

#[test]
fn team_add_missing_file_reports_structured_error() {
    let harness = CliHarness::new();

    let output = harness.run(&["--json", "team", "add", "/definitely/missing/team.yaml"]);
    assert_runtime_error_message(&output, "not_found", "file not found");
}

#[test]
fn json_output_supports_profile_and_team_commands() {
    let harness = CliHarness::new();

    let empty_profiles = harness.run(&["--json", "profile", "list"]);
    assert!(empty_profiles.status.success());
    let empty_profiles_json = stdout_json(&empty_profiles);
    assert_eq!(empty_profiles_json["ok"], Value::Bool(true));
    assert_eq!(empty_profiles_json["profiles"], Value::Array(vec![]));

    let init_profiles = harness.run(&["--json", "profile", "init"]);
    assert!(init_profiles.status.success());
    assert_eq!(stdout_json(&init_profiles)["installed"], Value::from(9));

    let show_profile = harness.run(&["--json", "profile", "show", "developer"]);
    assert!(show_profile.status.success());
    let show_profile_json = stdout_json(&show_profile);
    assert_eq!(
        show_profile_json["profile"]["title"],
        Value::from("developer")
    );

    let init_teams = harness.run(&["--json", "team", "init"]);
    assert!(init_teams.status.success());
    assert_eq!(stdout_json(&init_teams)["installed"], Value::from(7));

    let list_teams = harness.run(&["--json", "team", "list"]);
    assert!(list_teams.status.success());
    let list_teams_json = stdout_json(&list_teams);
    let teams = list_teams_json["teams"].as_array().unwrap();
    assert!(teams.iter().any(|team| team["name"] == "code-review"));
}
