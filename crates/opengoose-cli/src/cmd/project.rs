use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_persistence::Database;
use opengoose_projects::{ProjectContext, ProjectStore};
use opengoose_teams::run_headless_with_project;
use opengoose_types::EventBus;

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose project list\n  opengoose project show opengoose-dev\n  opengoose project add ./opengoose-project.yaml\n  opengoose project run opengoose-dev \"fix the login bug\" --team code-review\n  opengoose --json project list"
)]
/// Subcommands for `opengoose project`.
pub enum ProjectAction {
    /// List all project definitions
    #[command(after_help = "Examples:\n  opengoose project list\n  opengoose --json project list")]
    List,
    /// Show a project's full YAML
    #[command(after_help = "Example:\n  opengoose project show opengoose-dev")]
    Show {
        /// Project name (e.g. opengoose-dev)
        name: String,
    },
    /// Add a project from a YAML file
    #[command(after_help = "Example:\n  opengoose project add ./opengoose-project.yaml --force")]
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the project already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a project
    #[command(after_help = "Example:\n  opengoose project remove opengoose-dev")]
    Remove {
        /// Project name (e.g. opengoose-dev)
        name: String,
    },
    /// Create a sample project YAML in the current directory
    #[command(after_help = "Examples:\n  opengoose project init\n  opengoose project init --force")]
    Init {
        /// Overwrite existing file
        #[arg(long)]
        force: bool,
    },
    /// Run a project's default team with the given input
    #[command(
        after_help = "Examples:\n  opengoose project run opengoose-dev \"fix the login bug\"\n  opengoose project run opengoose-dev \"review the PR\" --team code-review"
    )]
    Run {
        /// Project name (e.g. opengoose-dev)
        project: String,
        /// Input to the team workflow
        input: String,
        /// Team to run (defaults to the project's `default_team`)
        #[arg(long)]
        team: Option<String>,
    },
}

/// Dispatch and execute the selected project subcommand.
pub async fn execute(action: ProjectAction, output: CliOutput) -> Result<()> {
    let store = ProjectStore::new()?;
    execute_with_store(action, store, output).await
}

pub async fn execute_with_store(
    action: ProjectAction,
    store: ProjectStore,
    output: CliOutput,
) -> Result<()> {
    match action {
        ProjectAction::List => cmd_list(&store, output),
        ProjectAction::Show { name } => cmd_show(&name, &store, output),
        ProjectAction::Add { path, force } => cmd_add(&path, force, &store, output),
        ProjectAction::Remove { name } => cmd_remove(&name, &store, output),
        ProjectAction::Init { force } => cmd_init(force, output),
        ProjectAction::Run {
            project,
            input,
            team,
        } => cmd_run(&project, &input, team.as_deref(), &store).await,
    }
}

fn cmd_list(store: &ProjectStore, output: CliOutput) -> Result<()> {
    let names = store.list()?;

    if names.is_empty() {
        if output.is_json() {
            output.print_json(&json!({
                "ok": true,
                "command": "project.list",
                "projects": [],
            }))?;
        } else {
            println!(
                "No projects found. Use `opengoose project init` to create a sample project file."
            );
        }
        return Ok(());
    }

    let projects = names
        .iter()
        .map(|name| store.get(name).map(|p| (name.clone(), p)))
        .collect::<Result<Vec<_>, _>>()?;

    if output.is_json() {
        let projects_json = projects
            .iter()
            .map(|(name, p)| {
                json!({
                    "name": name,
                    "description": p.description,
                    "goal": p.goal,
                    "cwd": p.cwd,
                    "default_team": p.default_team,
                })
            })
            .collect::<Vec<_>>();
        output.print_json(&json!({
            "ok": true,
            "command": "project.list",
            "projects": projects_json,
        }))?;
        return Ok(());
    }

    println!("{}", output.heading("Projects"));
    let rows = projects
        .iter()
        .map(|(name, p)| {
            vec![
                name.clone(),
                p.default_team.clone().unwrap_or_else(|| "-".into()),
                p.cwd.clone().unwrap_or_else(|| "-".into()),
                p.description
                    .clone()
                    .unwrap_or_else(|| "(no description)".to_string()),
            ]
        })
        .collect::<Vec<_>>();
    print!(
        "{}",
        format_table(&["PROJECT", "DEFAULT TEAM", "CWD", "DESCRIPTION"], &rows)
    );

    Ok(())
}

fn cmd_show(name: &str, store: &ProjectStore, output: CliOutput) -> Result<()> {
    let project = store.get(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "project.show",
            "project": project,
        }))?;
    } else {
        let yaml = project.to_yaml()?;
        print!("{yaml}");
    }

    Ok(())
}

fn cmd_add(path: &PathBuf, force: bool, store: &ProjectStore, output: CliOutput) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let project = store.add_from_path(path, force)?;
    let name = project.title.clone();

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "project.add",
            "project": name,
            "path": path,
            "force": force,
        }))?;
    } else {
        println!("Added project `{name}`.");
    }

    Ok(())
}

fn cmd_remove(name: &str, store: &ProjectStore, output: CliOutput) -> Result<()> {
    store.remove(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "project.remove",
            "project": name,
            "removed": true,
        }))?;
    } else {
        println!("Removed project `{name}`.");
    }

    Ok(())
}

const SAMPLE_PROJECT_FILE: &str = "opengoose-project.yaml";
const SAMPLE_PROJECT_YAML: &str = r#"version: "1.0.0"
title: "my-project"
description: "Describe what this project is about"
goal: "Describe the high-level goal shared by all agents in this project"
# cwd: "/path/to/project"   # Defaults to this file's directory when omitted
# context_files:
#   - README.md              # Files whose content is injected into agent system prompts
#   - docs/architecture.md
# default_team: code-review  # Used by `opengoose project run my-project "input"`
# settings:
#   max_turns: 20
#   message_retention_days: 30
"#;

fn cmd_init(force: bool, output: CliOutput) -> Result<()> {
    let cwd = std::env::current_dir()?;
    cmd_init_in_dir(&cwd, force, output)
}

fn cmd_init_in_dir(dir: &Path, force: bool, output: CliOutput) -> Result<()> {
    let path = dir.join(SAMPLE_PROJECT_FILE);
    if path.exists() && !force {
        bail!(
            "'{}' already exists. Use --force to overwrite.",
            SAMPLE_PROJECT_FILE
        );
    }

    std::fs::write(&path, SAMPLE_PROJECT_YAML)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "project.init",
            "path": SAMPLE_PROJECT_FILE,
        }))?;
    } else {
        println!(
            "Created '{SAMPLE_PROJECT_FILE}'. Edit it, then register with:\n  opengoose project add {SAMPLE_PROJECT_FILE}"
        );
    }

    Ok(())
}

async fn cmd_run(
    project_name: &str,
    input: &str,
    team_override: Option<&str>,
    store: &ProjectStore,
) -> Result<()> {
    let project_def = store.get(project_name)?;

    let team_name = team_override
        .or(project_def.default_team.as_deref())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "project '{}' has no default_team configured; specify one with --team",
                project_name
            )
        })?
        .to_string();

    let store_dir = store.dir().to_path_buf();
    let project_ctx = Arc::new(ProjectContext::from_definition(
        &project_def,
        Some(&store_dir),
    ));

    println!(
        "Running project '{}' with team '{team_name}' (cwd: {})...",
        project_ctx.title,
        project_ctx.cwd.display()
    );

    let db = Arc::new(Database::open()?);
    let event_bus = EventBus::new(256);
    let (run_id, result) =
        run_headless_with_project(&team_name, input, db, event_bus, project_ctx).await?;

    println!("\n--- Result ---");
    println!("{result}");
    println!("\nRun ID: {run_id}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::output::OutputMode;

    async fn test_execute(action: ProjectAction, output: CliOutput) -> Result<()> {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::with_dir(tmp.path().to_path_buf());
        execute_with_store(action, store, output).await
    }

    fn text_output() -> CliOutput {
        CliOutput::new(OutputMode::Text)
    }

    fn json_output() -> CliOutput {
        CliOutput::new(OutputMode::Json)
    }

    #[tokio::test]
    async fn list_succeeds() {
        test_execute(ProjectAction::List, text_output())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_json_mode_succeeds() {
        test_execute(ProjectAction::List, json_output())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn add_reports_file_not_found() {
        let err = test_execute(
            ProjectAction::Add {
                path: PathBuf::from("/nonexistent/path/project.yaml"),
                force: false,
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

    #[tokio::test]
    async fn show_reports_unknown_project() {
        let err = test_execute(
            ProjectAction::Show {
                name: "definitely-nonexistent-project-xyz".into(),
            },
            text_output(),
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
    async fn remove_reports_unknown_project() {
        let err = test_execute(
            ProjectAction::Remove {
                name: "definitely-nonexistent-project-xyz".into(),
            },
            text_output(),
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
    async fn show_json_mode_reports_unknown_project() {
        let err = test_execute(
            ProjectAction::Show {
                name: "definitely-nonexistent-project-xyz".into(),
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
    async fn remove_json_mode_reports_unknown_project() {
        let err = test_execute(
            ProjectAction::Remove {
                name: "definitely-nonexistent-project-xyz".into(),
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
    async fn run_reports_unknown_project() {
        let err = test_execute(
            ProjectAction::Run {
                project: "definitely-nonexistent-project-xyz".into(),
                input: "hello".into(),
                team: None,
            },
            text_output(),
        )
        .await
        .unwrap_err();

        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("not found") || msg.contains("does not exist"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn init_creates_sample_file() {
        let tmp = tempfile::tempdir().unwrap();
        cmd_init_in_dir(tmp.path(), false, text_output()).unwrap();
        assert!(tmp.path().join(SAMPLE_PROJECT_FILE).exists());
    }

    #[test]
    fn init_force_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        cmd_init_in_dir(tmp.path(), false, text_output()).unwrap();
        // Second init with force should not fail
        cmd_init_in_dir(tmp.path(), true, text_output()).unwrap();
    }

    #[test]
    fn project_store_new_succeeds() {
        let store = ProjectStore::new();
        assert!(store.is_ok());
    }

    #[test]
    fn project_store_list_returns_vec() {
        let store = ProjectStore::new().unwrap();
        let names = store.list();
        assert!(names.is_ok());
    }
}
