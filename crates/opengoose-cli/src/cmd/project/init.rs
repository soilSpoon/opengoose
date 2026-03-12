use std::path::Path;

use anyhow::{Result, bail};
use serde_json::json;

use crate::cmd::output::CliOutput;

pub(super) const SAMPLE_PROJECT_FILE: &str = "opengoose-project.yaml";
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

pub(super) fn run(force: bool, output: CliOutput) -> Result<()> {
    let cwd = std::env::current_dir()?;
    run_in_dir(&cwd, force, output)
}

pub(super) fn run_in_dir(dir: &Path, force: bool, output: CliOutput) -> Result<()> {
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
