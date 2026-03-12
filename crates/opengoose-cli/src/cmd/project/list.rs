use anyhow::Result;
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_projects::{ProjectDefinition, ProjectStore};

pub(super) fn run(store: &ProjectStore, output: CliOutput) -> Result<()> {
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

    let projects = load_projects(store, &names)?;

    if output.is_json() {
        let projects_json = projects
            .iter()
            .map(|(name, project)| {
                json!({
                    "name": name,
                    "description": project.description,
                    "goal": project.goal,
                    "cwd": project.cwd,
                    "default_team": project.default_team,
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
        .map(|(name, project)| {
            vec![
                name.clone(),
                project.default_team.clone().unwrap_or_else(|| "-".into()),
                project.cwd.clone().unwrap_or_else(|| "-".into()),
                project
                    .description
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

fn load_projects(
    store: &ProjectStore,
    names: &[String],
) -> Result<Vec<(String, ProjectDefinition)>> {
    Ok(names
        .iter()
        .map(|name| store.get(name).map(|project| (name.clone(), project)))
        .collect::<Result<Vec<_>, _>>()?)
}
