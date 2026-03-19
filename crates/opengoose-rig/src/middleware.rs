// Middleware — pre_hydrate / post_execute hooks
//
// pre_hydrate: 작업 시작 전 시스템 프롬프트 확장 (AGENTS.md, 스킬 카탈로그, Board prime)
// post_execute: 작업 완료 후 자동 실행 (lint/test)

use goose::agents::Agent;
use std::path::Path;

/// pre_hydrate: 작업 시작 전 시스템 프롬프트에 컨텍스트 주입.
pub async fn pre_hydrate(agent: &Agent, work_dir: &Path) {
    // AGENTS.md 주입
    if let Some(agents_md) = load_agents_md(work_dir) {
        agent
            .extend_system_prompt("agents-md".to_string(), agents_md)
            .await;
    }

    // 스킬 카탈로그 주입
    let catalog = load_skill_catalog();
    if !catalog.is_empty() {
        agent
            .extend_system_prompt("skill-catalog".to_string(), catalog)
            .await;
    }
}

/// post_execute: 작업 완료 후 자동 액션.
/// 코드 작업인 경우 lint/test 자동 실행 결과를 반환.
pub async fn post_execute(work_dir: &Path) -> Option<String> {
    // Cargo.toml 존재 시 cargo check 실행
    if work_dir.join("Cargo.toml").exists() {
        return run_check(work_dir).await;
    }
    // package.json 존재 시 npm test
    if work_dir.join("package.json").exists() {
        return run_npm_check(work_dir).await;
    }
    None
}

fn load_agents_md(work_dir: &Path) -> Option<String> {
    let path = work_dir.join("AGENTS.md");
    std::fs::read_to_string(path).ok()
}

fn load_skill_catalog() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let skills_dir = home.join(".opengoose/skills");
    if !skills_dir.is_dir() {
        return String::new();
    }

    let mut catalog = String::from("# Available Skills\n\n");
    let mut found = false;

    let entries = match std::fs::read_dir(&skills_dir) {
        Ok(e) => e,
        Err(_) => return String::new(),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&skill_md) {
            if let Some((name, desc)) = parse_skill_header(&content) {
                catalog.push_str(&format!("- **{name}**: {desc}\n"));
                found = true;
            }
        }
    }

    if found { catalog } else { String::new() }
}

fn parse_skill_header(content: &str) -> Option<(String, String)> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];

    let mut name = None;
    let mut description = None;
    for line in frontmatter.lines() {
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().trim_matches('"').to_string());
        }
        if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().trim_matches('"').to_string());
        }
    }

    Some((name?, description?))
}

async fn run_check(work_dir: &Path) -> Option<String> {
    let output = tokio::process::Command::new("cargo")
        .arg("check")
        .arg("--message-format=short")
        .current_dir(work_dir)
        .output()
        .await
        .ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        None
    } else {
        Some(format!("cargo check failed:\n{stderr}"))
    }
}

async fn run_npm_check(work_dir: &Path) -> Option<String> {
    let output = tokio::process::Command::new("npm")
        .arg("test")
        .arg("--")
        .arg("--passWithNoTests")
        .current_dir(work_dir)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        None
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Some(format!("npm test failed:\n{stderr}"))
    }
}
