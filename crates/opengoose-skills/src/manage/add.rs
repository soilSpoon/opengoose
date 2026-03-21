use crate::manage::{discover, lock};
use crate::source;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub async fn run(
    base_dir: &Path,
    src: &str,
    all: bool,
    skill_filter: Option<&str>,
    global: bool,
) -> Result<()> {
    let git_source = source::parse_source(src)?;
    println!("Cloning {}...", git_source.owner_repo);

    let clone_dir = clone_repo(&git_source.clone_url)?;

    let skills = discover::discover_skills(&clone_dir);
    if skills.is_empty() {
        std::fs::remove_dir_all(&clone_dir).ok();
        anyhow::bail!("No skills found in {}", git_source.owner_repo);
    }
    println!("Found {} skill(s)", skills.len());

    let selected = select_skills(&skills, all, skill_filter)?;
    if selected.is_empty() {
        std::fs::remove_dir_all(&clone_dir).ok();
        println!("No skills selected.");
        return Ok(());
    }

    let target_base = install_base(base_dir, global)?;
    for skill in &selected {
        install_skill(skill, &target_base)?;
        let now = lock::now_iso();
        lock::add_entry(
            base_dir,
            &skill.name,
            lock::SkillLockEntry {
                source: git_source.owner_repo.clone(),
                source_type: "github".to_string(),
                source_url: git_source.clone_url.clone(),
                skill_path: Some(skill.rel_path.clone()),
                skill_folder_hash: String::new(),
                installed_at: now.clone(),
                updated_at: now,
                plugin_name: None,
            },
        )?;
        println!("  Installed: {}", skill.name);
    }

    std::fs::remove_dir_all(&clone_dir).ok();

    println!(
        "\n{} skill(s) installed to {}",
        selected.len(),
        target_base.display()
    );
    Ok(())
}

fn clone_repo(url: &str) -> Result<PathBuf> {
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        url.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    };
    let dir = std::env::temp_dir().join(format!("opengoose-skills-{hash}"));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", "--quiet", url])
        .arg(&dir)
        .status()?;
    if !status.success() {
        anyhow::bail!("git clone failed for {url}");
    }
    Ok(dir)
}

fn select_skills(
    skills: &[discover::DiscoveredSkill],
    all: bool,
    skill_filter: Option<&str>,
) -> Result<Vec<discover::DiscoveredSkill>> {
    if all {
        return Ok(skills.to_vec());
    }

    if let Some(name) = skill_filter {
        let found = skills.iter().find(|s| s.name == name);
        return match found {
            Some(s) => Ok(vec![s.clone()]),
            None => {
                let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
                anyhow::bail!("Skill '{name}' not found. Available: {}", names.join(", "))
            }
        };
    }

    let items: Vec<String> = skills
        .iter()
        .map(|s| format!("{} — {}", s.name, s.description))
        .collect();

    let selections = dialoguer::MultiSelect::new()
        .with_prompt("Select skills to install")
        .items(&items)
        .interact()?;

    Ok(selections.into_iter().map(|i| skills[i].clone()).collect())
}

pub fn install_base(base_dir: &Path, global: bool) -> Result<PathBuf> {
    if global {
        Ok(base_dir.join(".opengoose/skills/installed"))
    } else {
        Ok(PathBuf::from(".opengoose/skills/installed"))
    }
}

fn install_skill(skill: &discover::DiscoveredSkill, base: &Path) -> Result<()> {
    let target = base.join(&skill.name);
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    copy_dir_recursive(&skill.abs_path, &target)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') || name_str == "__pycache__" {
            continue;
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]
    use super::*;
    use crate::manage::discover::DiscoveredSkill;

    fn mock_skill(name: &str, rel_path: &str, abs_path: std::path::PathBuf) -> DiscoveredSkill {
        DiscoveredSkill {
            name: name.into(),
            description: format!("description of {name}"),
            rel_path: rel_path.into(),
            abs_path,
        }
    }

    #[test]
    fn select_skills_all_returns_all() {
        let skills = vec![
            mock_skill("a", "a", "a".into()),
            mock_skill("b", "b", "b".into()),
        ];
        let selected = select_skills(&skills, true, None).unwrap();
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_skills_specific_name() {
        let skills = vec![
            mock_skill("a", "a", "a".into()),
            mock_skill("b", "b", "b".into()),
        ];
        let selected = select_skills(&skills, false, Some("b")).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "b");
    }

    #[test]
    fn select_skills_missing_name_reports_helpful_error() {
        let skills = vec![mock_skill("a", "a", "a".into())];
        let err = select_skills(&skills, false, Some("x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Available"));
    }

    #[test]
    fn install_base_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let global = install_base(tmp.path(), true).unwrap();
        assert!(
            global
                .to_string_lossy()
                .ends_with(".opengoose/skills/installed")
        );

        let local = install_base(tmp.path(), false).unwrap();
        assert_eq!(local, PathBuf::from(".opengoose/skills/installed"));
    }

    #[test]
    fn copy_dir_recursive_skips_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join(".hidden")).unwrap();
        std::fs::write(src.join(".hidden").join("skip.me"), "1").unwrap_or(());
        std::fs::write(src.join("__pycache__"), "1").unwrap_or(());
        std::fs::write(src.join("ok.txt"), "ok").unwrap();

        let dst = tmp.path().join("dst");
        copy_dir_recursive(&src, &dst).unwrap();

        assert!(!dst.join(".hidden").exists());
        assert!(!dst.join("__pycache__").exists());
        assert!(dst.join("ok.txt").exists());
    }

    #[test]
    fn copy_dir_recursive_copies_nested_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join("subdir")).unwrap();
        std::fs::write(src.join("subdir").join("file.txt"), "nested").unwrap();
        std::fs::write(src.join("top.txt"), "top").unwrap();

        let dst = tmp.path().join("dst");
        copy_dir_recursive(&src, &dst).unwrap();

        assert!(dst.join("top.txt").exists());
        assert!(dst.join("subdir").join("file.txt").exists());
    }

    #[test]
    fn install_skill_copies_files_to_target() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("my-skill");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("SKILL.md"),
            "---\nname: my-skill\ndescription: test\n---",
        )
        .unwrap();

        let skill = mock_skill("my-skill", "my-skill", src);
        let base = tmp.path().join("installed");
        std::fs::create_dir_all(&base).unwrap();
        install_skill(&skill, &base).unwrap();
        assert!(base.join("my-skill").join("SKILL.md").exists());
    }

    #[test]
    fn install_skill_overwrites_existing_target() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("my-skill");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "v2").unwrap();

        let skill = mock_skill("my-skill", "my-skill", src);
        let base = tmp.path().join("installed");
        // Pre-create stale target with old file
        let target = base.join("my-skill");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("OLD.md"), "old").unwrap();

        install_skill(&skill, &base).unwrap();

        assert!(!base.join("my-skill").join("OLD.md").exists());
        assert!(base.join("my-skill").join("SKILL.md").exists());
    }

    /// Covers add::run lines 13-14 — git repo with no SKILL.md (no skills found).
    #[tokio::test]
    async fn run_fails_when_cloned_repo_has_no_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = crate::test_utils::IsolatedEnv::new(tmp.path());
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        // Create a git repo WITHOUT any SKILL.md files
        let repo = tmp.path().join("empty-repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@test.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@test.com")
                .output()
                .unwrap()
        };
        git(&["init"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(repo.join("README.md"), "No skills here").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "no skills"]);

        // run() should clone, find no skills → remove clone dir (line 13) → bail (line 14)
        let result = run(tmp.path(), repo.to_str().unwrap(), true, None, false).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No skills found"));

        std::env::set_current_dir(cwd).unwrap();
    }

    /// Covers add::run lines 11-52 (success path) and clone_repo lines 64, 72-73.
    /// Creates a real local git repo with a SKILL.md, then calls add::run twice:
    /// - First call: discovers skill, installs it (covers discover/select/install/lock paths).
    /// - Second call: clone dir already exists from first call → covers line 64 (remove_dir_all).
    #[tokio::test]
    async fn run_installs_skill_from_local_git_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = crate::test_utils::IsolatedEnv::new(tmp.path());
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        // Build a minimal local git repo with one SKILL.md
        let repo = tmp.path().join("test-repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@test.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@test.com")
                .output()
                .unwrap()
        };
        git(&["init"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(
            repo.join("SKILL.md"),
            "---\nname: local-test-skill\ndescription: Use when testing local install\n---\n# Body\n",
        ).unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "add skill"]);

        let src = repo.to_str().unwrap();

        // First call: fresh install covers lines 11-52 and clone_repo success (72-73)
        let result = run(tmp.path(), src, true, None, false).await;
        assert!(result.is_ok(), "first add::run failed: {result:?}");
        assert!(
            tmp.path()
                .join(".opengoose/skills/installed/local-test-skill/SKILL.md")
                .exists()
        );

        // Second call with the same source: clone dir already exists → covers line 64
        let result2 = run(tmp.path(), src, true, None, false).await;
        assert!(result2.is_ok(), "second add::run failed: {result2:?}");

        // Restore
        std::env::set_current_dir(cwd).unwrap();
    }

    /// Covers clone_repo line 64 — the branch where the clone dir already exists before cloning.
    /// Pre-creates a directory with the same hash-based name that clone_repo would use, then
    /// calls clone_repo with a non-git URL so clone fails (non-zero exit) → returns Err.
    #[test]
    fn clone_repo_removes_stale_dir_before_cloning() {
        let tmp = tempfile::tempdir().unwrap();
        let url = tmp.path().to_str().unwrap();
        // Replicate the hash logic in clone_repo
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            url.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        };
        let dir = std::env::temp_dir().join(format!("opengoose-skills-{hash}"));
        // Pre-create so clone_repo hits line 64 (remove_dir_all)
        std::fs::create_dir_all(&dir).unwrap();
        // clone_repo should remove the stale dir then fail (not a git repo)
        let result = clone_repo(url);
        assert!(result.is_err());
        // The pre-created dir was removed, then git clone left no dir behind on failure
        assert!(!dir.exists());
    }

    /// Covers select_skills lines 94-106 — the skill_filter Some path when skill is not found.
    /// Also indirectly validates the error message format.
    #[test]
    fn select_skills_specific_missing_name_error() {
        let skills = vec![mock_skill("alpha", "alpha", "alpha".into())];
        let err = select_skills(&skills, false, Some("missing")).unwrap_err();
        assert!(
            err.to_string().contains("missing"),
            "error should mention the skill name"
        );
        assert!(
            err.to_string().contains("Available"),
            "error should list available skills"
        );
    }
}
