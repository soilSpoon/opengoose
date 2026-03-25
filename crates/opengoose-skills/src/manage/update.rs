use crate::manage::{add, lock};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn global_skills_dir(base_dir: &Path) -> PathBuf {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| base_dir.join(".config"));
    config_home.join("goose").join("skills")
}

pub async fn run(base_dir: &Path) -> anyhow::Result<()> {
    let lock_data = lock::read_lock(base_dir);
    if lock_data.skills.is_empty() {
        println!("No skills installed. Use 'opengoose skills add' first.");
        return Ok(());
    }

    let mut by_source: HashMap<String, Vec<(String, lock::SkillLockEntry)>> = HashMap::new();
    for (name, entry) in &lock_data.skills {
        by_source
            .entry(entry.source.clone())
            .or_default()
            .push((name.clone(), entry.clone()));
    }

    let mut updated = 0;
    for (source, skills) in &by_source {
        println!("Checking {}...", source);
        for (name, _entry) in skills {
            let global = global_skills_dir(base_dir).join(name).is_dir();
            if let Err(e) = add::run(base_dir, source, false, Some(name.as_str()), global).await {
                println!("  Failed to update {}: {}", name, e);
            } else {
                updated += 1;
            }
        }
    }

    println!("\n{} skill(s) updated.", updated);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::IsolatedEnv;

    #[test]
    fn global_skills_dir_is_under_goose_skills() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let path = global_skills_dir(tmp.path());
        assert!(path.ends_with("goose/skills"));
    }

    #[test]
    fn global_skills_dir_prefers_xdg_config_home() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
        }
        assert_eq!(
            global_skills_dir(tmp.path()),
            tmp.path().join("config").join("goose").join("skills")
        );
    }

    #[tokio::test]
    async fn run_with_empty_lock_returns_ok_immediately() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        // No lock file → empty lock → "No skills installed" early return
        let result = run(tmp.path()).await;
        result.unwrap();
    }

    #[tokio::test]
    async fn run_with_skills_in_lock_attempts_update_and_fails_gracefully() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        // Create a local directory (not a git repo) to use as "source".
        // git clone on a non-git dir fails fast → covers the error path.
        let fake_repo = tmp.path().join("fake-repo");
        std::fs::create_dir_all(&fake_repo).expect("directory creation should succeed");

        let entry = lock::SkillLockEntry {
            source: fake_repo.to_string_lossy().to_string(),
            source_type: "local".to_string(),
            source_url: fake_repo.to_string_lossy().to_string(),
            skill_path: None,
            skill_folder_hash: "abc123".to_string(),
            installed_at: lock::now_iso(),
            updated_at: lock::now_iso(),
            plugin_name: None,
        };
        lock::add_entry(tmp.path(), "test-update-skill", entry)
            .expect("skill operation should succeed");

        // run() should iterate over skills, call add::run() which fails (not a git repo),
        // print the failure message, then finish and return Ok.
        let result = run(tmp.path()).await;
        result.unwrap();
    }
}
