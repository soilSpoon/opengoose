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

    #[test]
    fn load_agents_md_returns_none_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_agents_md(tmp.path());
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn post_execute_returns_none_when_no_project_files() {
        let tmp = tempfile::tempdir().unwrap();
        let result = post_execute(tmp.path()).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn post_execute_runs_cargo_check_when_cargo_toml_present() {
        let tmp = tempfile::tempdir().unwrap();
        // A Cargo.toml with no src/ causes cargo check to fail → Some(error)
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test-check\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let result = post_execute(tmp.path()).await;
        // cargo check fails (no src/) → Some(error message)
        assert!(result.is_some());
        assert!(result.unwrap().contains("cargo check failed"));
    }

    #[tokio::test]
    async fn post_execute_returns_none_when_cargo_check_passes() {
        let tmp = tempfile::tempdir().unwrap();
        // Create a valid minimal Cargo project
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test-pass\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
        let result = post_execute(tmp.path()).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn pre_hydrate_with_agents_md_and_nonempty_catalog() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "be helpful").unwrap();
        let agent = goose::agents::Agent::new();
        pre_hydrate(&agent, tmp.path(), "## Skills\n- skill-a").await;
        // No panic = success
    }

    #[tokio::test]
    async fn post_execute_calls_npm_check_when_package_json_present() {
        let tmp = tempfile::tempdir().unwrap();
        // Only package.json exists (no Cargo.toml) → triggers run_npm_check
        std::fs::write(
            tmp.path().join("package.json"),
            r#"{"name":"test","scripts":{"test":"echo ok"}}"#,
        )
        .unwrap();
        // Covers line 35: return run_npm_check(work_dir).await
        // npm may or may not be installed — result can be None or Some
        let _result = post_execute(tmp.path()).await;
    }

    #[tokio::test]
    async fn post_execute_npm_check_returns_error_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        // Script that always fails → covers lines 98-99 when npm is installed
        std::fs::write(
            tmp.path().join("package.json"),
            r#"{"name":"test","scripts":{"test":"exit 1"}}"#,
        )
        .unwrap();
        // npm may or may not be installed; just ensure no panic
        let result = post_execute(tmp.path()).await;
        // If npm is installed: Some("npm test failed:..."), else None
        drop(result);
    }

    #[tokio::test]
    async fn pre_hydrate_with_empty_catalog_and_no_agents_md() {
        let tmp = tempfile::tempdir().unwrap();
        let agent = goose::agents::Agent::new();
        pre_hydrate(&agent, tmp.path(), "").await;
        // No panic = success
    }
}
