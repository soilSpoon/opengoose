// Middleware — pre_hydrate / post_execute hooks
//
// pre_hydrate: 작업 시작 전 시스템 프롬프트 확장 (AGENTS.md, 스킬 카탈로그, Board prime)
// post_execute: 작업 완료 후 자동 실행 (lint/test)

use goose::agents::Agent;
use std::path::Path;

fn hydration_context(
    work_dir: &Path,
    skill_catalog: &str,
    board_prime: &str,
) -> Vec<(String, String)> {
    let mut ctx = Vec::new();
    if let Some(agents_md) = load_agents_md(work_dir) {
        ctx.push(("agents-md".to_string(), agents_md));
    }
    if !skill_catalog.is_empty() {
        ctx.push(("skill-catalog".to_string(), skill_catalog.to_string()));
    }
    if !board_prime.is_empty() {
        ctx.push(("board-prime".to_string(), board_prime.to_string()));
    }
    ctx
}

/// pre_hydrate: 작업 시작 전 시스템 프롬프트에 컨텍스트 주입.
pub async fn pre_hydrate(agent: &Agent, work_dir: &Path, skill_catalog: &str, board_prime: &str) {
    for (key, value) in hydration_context(work_dir, skill_catalog, board_prime) {
        agent.extend_system_prompt(key, value).await;
    }
}

/// post_execute: 작업 완료 후 자동 액션.
/// 코드 작업인 경우 lint/test 자동 실행 결과를 반환.
pub async fn post_execute(work_dir: &Path) -> anyhow::Result<Option<String>> {
    // Cargo.toml 존재 시 cargo check 실행
    if work_dir.join("Cargo.toml").exists() {
        return run_check(work_dir).await;
    }
    // package.json 존재 시 npm test
    if work_dir.join("package.json").exists() {
        return run_npm_check(work_dir).await;
    }
    Ok(None)
}

fn load_agents_md(work_dir: &Path) -> Option<String> {
    let path = work_dir.join("AGENTS.md");
    match std::fs::read_to_string(&path) {
        Ok(content) => Some(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::debug!(path = %path.display(), "failed to read AGENTS.md: {e}");
            None
        }
    }
}

pub fn parse_skill_header(content: &str) -> Option<(String, String)> {
    let fm = opengoose_skills::metadata::parse_frontmatter(content)?;
    Some((fm.name, fm.description))
}

/// 외부 커맨드 실행. 실패 시 label 포함 에러 메시지 반환.
async fn run_cmd(
    cmd: &str,
    args: &[&str],
    work_dir: &Path,
    label: &str,
    envs: &[(&str, &str)],
) -> anyhow::Result<Option<String>> {
    use std::time::Duration;

    let mut command = tokio::process::Command::new(cmd);
    command
        .args(args)
        .current_dir(work_dir)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for &(k, v) in envs {
        command.env(k, v);
    }

    let child = command
        .spawn()
        .map_err(|e| anyhow::anyhow!("{label}: failed to spawn: {e}"))?;

    let output = tokio::time::timeout(Duration::from_secs(300), child.wait_with_output())
        .await
        .map_err(|_| anyhow::anyhow!("{label}: timed out after 300s"))?
        .map_err(|e| anyhow::anyhow!("{label}: {e}"))?;

    if output.status.success() {
        Ok(None)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stdout.is_empty() {
            stderr.into_owned()
        } else {
            format!("{stdout}\n{stderr}")
        };
        Ok(Some(format!("{label} failed:\n{detail}")))
    }
}

async fn run_check(work_dir: &Path) -> anyhow::Result<Option<String>> {
    if let Some(err) = run_cmd(
        "cargo",
        &["check", "--message-format=short"],
        work_dir,
        "cargo check",
        &[],
    )
    .await?
    {
        return Ok(Some(err));
    }
    run_cmd("cargo", &["test"], work_dir, "cargo test", &[]).await
}

async fn run_npm_check(work_dir: &Path) -> anyhow::Result<Option<String>> {
    run_cmd(
        "npm",
        &["test", "--", "--passWithNoTests"],
        work_dir,
        "npm test",
        &[],
    )
    .await
}

// ── 테스트 전용: 자식 프로세스 환경만 변경 ────────────────────

#[cfg(test)]
async fn run_npm_check_with_path(work_dir: &Path, path: &str) -> anyhow::Result<Option<String>> {
    run_cmd(
        "npm",
        &["test", "--", "--passWithNoTests"],
        work_dir,
        "npm test",
        &[("PATH", path)],
    )
    .await
}

#[cfg(test)]
async fn run_check_with_path(work_dir: &Path, path: &str) -> anyhow::Result<Option<String>> {
    if let Some(err) = run_cmd(
        "cargo",
        &["check", "--message-format=short"],
        work_dir,
        "cargo check",
        &[("PATH", path)],
    )
    .await?
    {
        return Ok(Some(err));
    }
    run_cmd(
        "cargo",
        &["test"],
        work_dir,
        "cargo test",
        &[("PATH", path)],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_header_extracts_name_and_description() {
        let content = "---\nname: test-skill\ndescription: Use when testing\n---\n# body";
        let parsed = parse_skill_header(content).expect("skill header should parse");
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
        let tmp = crate::test_fixtures::temp_dir();
        let path = tmp.path().join("AGENTS.md");
        std::fs::write(&path, "agent instructions").expect("test fixture write should succeed");
        let loaded = load_agents_md(tmp.path());
        assert_eq!(
            loaded.expect("loaded metadata should exist"),
            "agent instructions"
        );
    }

    #[test]
    fn load_agents_md_returns_none_when_file_absent() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let loaded = load_agents_md(tmp.path());
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn hydration_context_includes_board_prime() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let ctx = hydration_context(
            tmp.path(),
            "",
            "Board: 3 open, 1 claimed, 2 done\nRig: worker\n",
        );
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].0, "board-prime");
        assert!(ctx[0].1.contains("3 open"));
    }

    #[test]
    fn hydration_context_includes_agents_md_and_catalog() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        std::fs::write(tmp.path().join("AGENTS.md"), "be helpful")
            .expect("test fixture write should succeed");
        let ctx = hydration_context(tmp.path(), "## Skills\n- skill-a", "");
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0], ("agents-md".into(), "be helpful".into()));
        assert_eq!(
            ctx[1],
            ("skill-catalog".into(), "## Skills\n- skill-a".into())
        );
    }

    #[test]
    fn hydration_context_skips_missing_agents_md_and_empty_catalog() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let ctx = hydration_context(tmp.path(), "", "");
        assert!(ctx.is_empty());
    }

    #[test]
    fn hydration_context_includes_only_catalog_when_no_agents_md() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let ctx = hydration_context(tmp.path(), "## Skills", "");
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].0, "skill-catalog");
    }

    #[test]
    fn hydration_context_includes_only_agents_md_when_catalog_empty() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        std::fs::write(tmp.path().join("AGENTS.md"), "instructions")
            .expect("test fixture write should succeed");
        let ctx = hydration_context(tmp.path(), "", "");
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0], ("agents-md".into(), "instructions".into()));
    }

    #[tokio::test]
    async fn post_execute_returns_none_when_no_project_files() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let result = post_execute(tmp.path())
            .await
            .expect("async operation should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn post_execute_runs_cargo_check_when_cargo_toml_present() {
        if std::process::Command::new("cargo")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // cargo not in PATH in this environment — skip
        }
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        // A Cargo.toml with no src/ causes cargo check to fail → Some(error)
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test-check\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("file write should succeed");
        let result = post_execute(tmp.path())
            .await
            .expect("async operation should succeed");
        // cargo check fails (no src/) → Some(error message)
        assert!(result.is_some());
        assert!(
            result
                .expect("result should be present")
                .contains("cargo check failed")
        );
    }

    #[tokio::test]
    async fn post_execute_returns_none_when_cargo_check_passes() {
        if std::process::Command::new("cargo")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // cargo not in PATH in this environment — skip
        }
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        // Create a valid minimal Cargo project
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test-pass\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("file write should succeed");
        std::fs::create_dir_all(tmp.path().join("src")).expect("directory creation should succeed");
        std::fs::write(tmp.path().join("src/lib.rs"), "")
            .expect("test fixture write should succeed");
        let result = post_execute(tmp.path())
            .await
            .expect("async operation should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn post_execute_runs_cargo_test_after_check() {
        if std::process::Command::new("cargo")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // cargo not in PATH in this environment — skip
        }
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        // Create a valid Cargo project with a failing test
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test-proj\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("file write should succeed");
        std::fs::create_dir_all(tmp.path().join("src")).expect("directory creation should succeed");
        std::fs::write(
            tmp.path().join("src/lib.rs"),
            r#"
            #[cfg(test)]
            mod tests {
                #[test]
                fn it_fails() { assert!(false); }
            }
        "#,
        )
        .expect("file write should succeed");
        let result = post_execute(tmp.path())
            .await
            .expect("async operation should succeed");
        assert!(result.is_some());
        assert!(
            result
                .expect("result should be present")
                .contains("cargo test failed")
        );
    }

    /// fake npm 바이너리를 생성하고 경로를 반환. Unix 전용 (#!/bin/sh 사용).
    #[cfg(unix)]
    fn setup_fake_npm(tmp: &std::path::Path, script: &str) -> String {
        let bin_dir = tmp.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let fake_npm = bin_dir.join("npm");
        std::fs::write(&fake_npm, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_npm, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let orig_path = std::env::var("PATH").unwrap_or_default();
        format!("{}:{orig_path}", bin_dir.display())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn npm_check_succeeds_with_fake_npm() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("package.json"),
            r#"{"name":"test","scripts":{"test":"echo ok"}}"#,
        )
        .unwrap();

        let path = setup_fake_npm(tmp.path(), "#!/bin/sh\nexit 0");
        let result = run_npm_check_with_path(tmp.path(), &path).await.unwrap();
        assert!(result.is_none(), "successful npm test should return None");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn npm_check_reports_failure_with_fake_npm() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("package.json"),
            r#"{"name":"test","scripts":{"test":"exit 1"}}"#,
        )
        .unwrap();

        let path = setup_fake_npm(tmp.path(), "#!/bin/sh\necho 'test failed' >&2; exit 1");
        let result = run_npm_check_with_path(tmp.path(), &path).await.unwrap();
        assert!(result.is_some(), "failed npm test should return Some");
        assert!(result.unwrap().contains("npm test failed"));
    }

    #[tokio::test]
    async fn cargo_check_returns_err_when_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();

        let result = run_check_with_path(tmp.path(), "/nonexistent-dir-for-test").await;
        assert!(
            result.is_err(),
            "missing cargo should return Err, not Ok(None)"
        );
    }
}
