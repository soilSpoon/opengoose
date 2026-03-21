// Middleware — pre_hydrate / post_execute hooks
//
// pre_hydrate: 작업 시작 전 시스템 프롬프트 확장 (AGENTS.md, 스킬 카탈로그, Board prime)
// post_execute: 작업 완료 후 자동 실행 (lint/test)

use goose::agents::Agent;
use std::path::Path;

/// pre_hydrate: 작업 시작 전 시스템 프롬프트에 컨텍스트 주입.
pub async fn pre_hydrate(agent: &Agent, work_dir: &Path, skill_catalog: &str) {
    // AGENTS.md 주입
    if let Some(agents_md) = load_agents_md(work_dir) {
        agent
            .extend_system_prompt("agents-md".to_string(), agents_md)
            .await;
    }

    // 스킬 카탈로그 주입
    if !skill_catalog.is_empty() {
        agent
            .extend_system_prompt("skill-catalog".to_string(), skill_catalog.to_string())
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

pub fn parse_skill_header(content: &str) -> Option<(String, String)> {
    let fm = opengoose_skills::metadata::parse_frontmatter(content)?;
    Some((fm.name, fm.description))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_header_extracts_name_and_description() {
        let content = "---\nname: test-skill\ndescription: Use when testing\n---\n# body";
        let parsed = parse_skill_header(content).unwrap();
        assert_eq!(parsed, ("test-skill".into(), "Use when testing".into()));
    }

    #[test]
    fn parse_skill_header_rejects_invalid() {
        assert!(parse_skill_header("# no frontmatter").is_none());
        assert!(parse_skill_header("---\nname: only-name\n---").is_none());
        assert!(parse_skill_header("---\ndescription: only desc\n---").is_none());
    }

    #[test]
    fn load_agents_md_reads_file_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("AGENTS.md");
        std::fs::write(&path, "agent instructions").unwrap();
        let loaded = load_agents_md(tmp.path());
        assert_eq!(loaded.unwrap(), "agent instructions");
    }
}
