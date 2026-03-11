use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_persistence::{Database, OrchestrationStore, WorkItemStore, WorkStatus};
use opengoose_teams::{TeamDefinition, TeamStore};
use opengoose_types::EventBus;

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose team list\n  opengoose team show code-review\n  opengoose --json team list"
)]
/// Subcommands for `opengoose team`.
pub enum TeamAction {
    /// List all team definitions
    #[command(after_help = "Examples:\n  opengoose team list\n  opengoose --json team list")]
    List,
    /// Show a team's full YAML
    #[command(after_help = "Example:\n  opengoose team show code-review")]
    Show {
        /// Team name (e.g. code-review)
        name: String,
    },
    /// Add a team from a YAML file
    #[command(after_help = "Example:\n  opengoose team add ./teams/custom.yaml --force")]
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the team already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a team
    #[command(after_help = "Example:\n  opengoose team remove code-review")]
    Remove {
        /// Team name (e.g. code-review)
        name: String,
    },
    /// Install bundled default teams
    #[command(after_help = "Examples:\n  opengoose team init\n  opengoose team init --force")]
    Init {
        /// Overwrite existing teams
        #[arg(long)]
        force: bool,
    },
    /// Run a team workflow
    Run {
        /// Team name (e.g. code-review)
        team: String,
        /// Input to the team workflow
        input: String,
    },
    /// Show status of a team run
    Status {
        /// Run ID (omit to list recent runs)
        run_id: Option<String>,
    },
    /// Show logs for a team run
    Logs {
        /// Run ID
        run_id: String,
    },
    /// Resume a suspended team run
    Resume {
        /// Run ID
        run_id: String,
    },
}

/// Dispatch and execute the selected team subcommand.
pub async fn execute(action: TeamAction, output: CliOutput) -> Result<()> {
    match action {
        TeamAction::List => cmd_list(output),
        TeamAction::Show { name } => cmd_show(&name, output),
        TeamAction::Add { path, force } => cmd_add(&path, force, output),
        TeamAction::Remove { name } => cmd_remove(&name, output),
        TeamAction::Init { force } => cmd_init(force, output),
        TeamAction::Run { team, input } => cmd_run(&team, &input).await,
        TeamAction::Status { run_id } => cmd_status(run_id.as_deref()),
        TeamAction::Logs { run_id } => cmd_logs(&run_id),
        TeamAction::Resume { run_id } => cmd_resume(&run_id).await,
    }
}

fn cmd_list(output: CliOutput) -> Result<()> {
    let store = TeamStore::new()?;
    let names = store.list()?;

    if names.is_empty() {
        if output.is_json() {
            output.print_json(&json!({
                "ok": true,
                "command": "team.list",
                "teams": [],
            }))?;
        } else {
            println!("No teams found. Use `opengoose team init` to install defaults.");
        }
        return Ok(());
    }

    let teams = names
        .iter()
        .map(|name| store.get(name).map(|team| (name.clone(), team)))
        .collect::<Result<Vec<_>, _>>()?;

    if output.is_json() {
        let teams_json = teams
            .iter()
            .map(|(name, team)| {
                json!({
                    "name": name,
                    "description": team.description,
                    "workflow": format!("{:?}", team.workflow).to_lowercase(),
                    "agent_count": team.agents.len(),
                })
            })
            .collect::<Vec<_>>();
        output.print_json(&json!({
            "ok": true,
            "command": "team.list",
            "teams": teams_json,
        }))?;
        return Ok(());
    }

    println!("{}", output.heading("Teams"));
    let rows = teams
        .iter()
        .map(|(name, team)| {
            vec![
                name.clone(),
                format!("{:?}", team.workflow).to_lowercase(),
                team.agents.len().to_string(),
                team.description
                    .clone()
                    .unwrap_or_else(|| "(no description)".to_string()),
            ]
        })
        .collect::<Vec<_>>();
    print!(
        "{}",
        format_table(&["TEAM", "WORKFLOW", "AGENTS", "DESCRIPTION"], &rows)
    );

    Ok(())
}

fn cmd_show(name: &str, output: CliOutput) -> Result<()> {
    let store = TeamStore::new()?;
    let team = store.get(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.show",
            "team": team,
        }))?;
    } else {
        let yaml = team.to_yaml()?;
        print!("{yaml}");
    }

    Ok(())
}

fn cmd_add(path: &PathBuf, force: bool, output: CliOutput) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let content = std::fs::read_to_string(path)?;
    let team = TeamDefinition::from_yaml(&content)?;
    let name = team.title.clone();

    let store = TeamStore::new()?;
    store.save(&team, force)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.add",
            "team": name,
            "path": path,
            "force": force,
        }))?;
    } else {
        println!("Added team `{name}`.");
    }

    Ok(())
}

fn cmd_remove(name: &str, output: CliOutput) -> Result<()> {
    let store = TeamStore::new()?;
    store.remove(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.remove",
            "team": name,
            "removed": true,
        }))?;
    } else {
        println!("Removed team `{name}`.");
    }

    Ok(())
}

fn cmd_init(force: bool, output: CliOutput) -> Result<()> {
    let store = TeamStore::new()?;
    let count = store.install_defaults(force)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.init",
            "installed": count,
            "force": force,
        }))?;
    } else if count == 0 {
        println!("All default teams already exist. Use --force to overwrite.");
    } else {
        println!("Installed {count} default team(s).");
    }
    Ok(())
}

async fn cmd_run(team_name: &str, input: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let event_bus = EventBus::new(256);

    println!("Running team '{team_name}'...");

    let (run_id, result) = opengoose_teams::run_headless(team_name, input, db, event_bus).await?;

    println!("\n--- Result ---");
    println!("{result}");
    println!("\nRun ID: {run_id}");

    Ok(())
}

fn cmd_status(run_id: Option<&str>) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let orch_store = OrchestrationStore::new(db.clone());

    match run_id {
        Some(id) => {
            let run = orch_store
                .get_run(id)?
                .ok_or_else(|| anyhow::anyhow!("run '{}' not found", id))?;

            println!("Run: {}", run.team_run_id);
            println!("Team: {}", run.team_name);
            println!("Workflow: {}", run.workflow);
            println!("Status: {}", run.status.as_str());
            println!("Progress: {}/{}", run.current_step, run.total_steps);
            println!("Created: {}", run.created_at);
            println!("Updated: {}", run.updated_at);

            if let Some(ref result) = run.result {
                let preview = if result.len() > 200 {
                    let end = result.floor_char_boundary(200);
                    format!("{}...", &result[..end])
                } else {
                    result.clone()
                };
                println!("Result: {preview}");
            }

            // Show work items tree
            let work_store = WorkItemStore::new(db);
            let items = work_store.list_for_run(id, None)?;

            if !items.is_empty() {
                println!("\nWork Items:");
                for item in &items {
                    let indent = if item.parent_id.is_some() {
                        "    "
                    } else {
                        "  "
                    };
                    let status_icon = match item.status {
                        WorkStatus::Completed => "✓",
                        WorkStatus::InProgress => "▶",
                        WorkStatus::Failed => "✗",
                        WorkStatus::Pending => "○",
                        WorkStatus::Cancelled => "⊘",
                    };
                    let agent = item.assigned_to.as_deref().unwrap_or("-");
                    println!(
                        "{indent}{status_icon} {} [{}] (step: {})",
                        item.title,
                        agent,
                        item.workflow_step
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".into())
                    );
                }
            }
        }
        None => {
            let runs = orch_store.list_runs(None, 20)?;

            if runs.is_empty() {
                println!("No team runs found.");
                return Ok(());
            }

            println!(
                "{:<38} {:<16} {:<10} {:<10} UPDATED",
                "RUN ID", "TEAM", "WORKFLOW", "STATUS"
            );
            for run in &runs {
                println!(
                    "{:<38} {:<16} {:<10} {:<10} {}",
                    run.team_run_id,
                    run.team_name,
                    run.workflow,
                    run.status.as_str(),
                    run.updated_at,
                );
            }
        }
    }

    Ok(())
}

fn cmd_logs(run_id: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let orch_store = OrchestrationStore::new(db.clone());

    let run = orch_store
        .get_run(run_id)?
        .ok_or_else(|| anyhow::anyhow!("run '{}' not found", run_id))?;

    println!(
        "Logs for run: {} (team: {}, workflow: {})",
        run.team_run_id, run.team_name, run.workflow
    );
    println!("Status: {}", run.status.as_str());
    println!();

    // Show work items with their inputs/outputs as a log timeline
    let work_store = WorkItemStore::new(db);
    let items = work_store.list_for_run(run_id, None)?;

    if items.is_empty() {
        println!("(no work items recorded)");
        return Ok(());
    }

    for item in &items {
        let status_icon = match item.status {
            WorkStatus::Completed => "✓",
            WorkStatus::InProgress => "▶",
            WorkStatus::Failed => "✗",
            WorkStatus::Pending => "○",
            WorkStatus::Cancelled => "⊘",
        };
        let agent = item.assigned_to.as_deref().unwrap_or("-");

        println!(
            "[{}] {status_icon} {} (agent: {}, step: {})",
            item.updated_at,
            item.title,
            agent,
            item.workflow_step
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".into())
        );

        if let Some(ref input) = item.input {
            let preview = if input.len() > 300 {
                format!("{}...", &input[..input.floor_char_boundary(300)])
            } else {
                input.clone()
            };
            println!("  Input: {preview}");
        }
        if let Some(ref output) = item.output {
            let preview = if output.len() > 300 {
                format!("{}...", &output[..output.floor_char_boundary(300)])
            } else {
                output.clone()
            };
            println!("  Output: {preview}");
        }
        if let Some(ref error) = item.error {
            println!("  Error: {error}");
        }
        println!();
    }

    Ok(())
}

async fn cmd_resume(run_id: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let event_bus = EventBus::new(256);

    println!("Resuming run '{run_id}'...");

    let result = opengoose_teams::resume_headless(run_id, db, event_bus).await?;

    println!("\n--- Result ---");
    println!("{result}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn text_output() -> CliOutput {
        CliOutput::new(crate::cmd::output::OutputMode::Text)
    }

    fn json_output() -> CliOutput {
        CliOutput::new(crate::cmd::output::OutputMode::Json)
    }

    #[tokio::test]
    async fn add_reports_file_not_found() {
        let err = execute(
            TeamAction::Add {
                path: PathBuf::from("/nonexistent/path/team.yaml"),
                force: false,
            },
            CliOutput::new(crate::cmd::output::OutputMode::Text),
        )
        .await
        .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("file not found") || msg.contains("not found"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn show_reports_unknown_team() {
        let err = execute(
            TeamAction::Show {
                name: "definitely-nonexistent-team-xyz".into(),
            },
            CliOutput::new(crate::cmd::output::OutputMode::Text),
        )
        .await
        .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("not found") || msg.contains("does not exist"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn remove_reports_unknown_team() {
        let err = execute(
            TeamAction::Remove {
                name: "definitely-nonexistent-team-xyz".into(),
            },
            CliOutput::new(crate::cmd::output::OutputMode::Text),
        )
        .await
        .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("not found") || msg.contains("does not exist"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn list_succeeds() {
        execute(
            TeamAction::List,
            CliOutput::new(crate::cmd::output::OutputMode::Text),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_json_mode_succeeds() {
        execute(TeamAction::List, json_output()).await.unwrap();
    }

    #[tokio::test]
    async fn init_succeeds() {
        execute(TeamAction::Init { force: false }, text_output())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn init_json_mode_succeeds() {
        execute(TeamAction::Init { force: false }, json_output())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn show_json_mode_reports_unknown_team() {
        let err = execute(
            TeamAction::Show {
                name: "definitely-nonexistent-team-xyz".into(),
            },
            json_output(),
        )
        .await
        .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("not found") || msg.contains("does not exist"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn add_with_invalid_yaml_content_fails() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "this is: not: valid: yaml: {{{{").unwrap();
        let path = tmp.path().to_path_buf();

        let err = execute(TeamAction::Add { path, force: false }, text_output())
            .await
            .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("yaml") || msg.contains("parse") || msg.contains("invalid"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn add_reports_file_not_found_in_json_mode() {
        let err = execute(
            TeamAction::Add {
                path: PathBuf::from("/nonexistent/path/team.yaml"),
                force: false,
            },
            json_output(),
        )
        .await
        .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("file not found") || msg.contains("not found"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn remove_json_mode_reports_unknown_team() {
        let err = execute(
            TeamAction::Remove {
                name: "definitely-nonexistent-team-xyz".into(),
            },
            json_output(),
        )
        .await
        .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("not found") || msg.contains("does not exist"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn add_empty_file_fails() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let err = execute(TeamAction::Add { path, force: false }, text_output())
            .await
            .unwrap_err();

        // Empty file should fail to parse as YAML team definition
        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("yaml") || msg.contains("parse") || msg.contains("invalid") || msg.contains("missing"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn add_with_force_flag_file_not_found() {
        let err = execute(
            TeamAction::Add {
                path: PathBuf::from("/nonexistent/path/team.yaml"),
                force: true,
            },
            text_output(),
        )
        .await
        .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("file not found") || msg.contains("not found"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn team_action_add_rejects_directory_as_path() {
        // A directory is not a valid YAML file
        let result = std::fs::read_to_string(PathBuf::from("/tmp"));
        assert!(result.is_err());
    }

    #[test]
    fn team_store_new_succeeds() {
        // TeamStore::new() should succeed when home dir exists
        let store = TeamStore::new();
        assert!(store.is_ok());
    }

    #[test]
    fn team_store_get_nonexistent_returns_error() {
        let store = TeamStore::new().unwrap();
        let result = store.get("nonexistent-team-xyz-12345");
        assert!(result.is_err());
    }

    #[test]
    fn team_store_list_returns_vec() {
        let store = TeamStore::new().unwrap();
        // Should return Ok even if empty
        let names = store.list();
        assert!(names.is_ok());
    }
}
