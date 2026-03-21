use crate::manage::{add, lock};
use std::path::Path;

pub fn run(base_dir: &Path, name: &str, global: bool) -> anyhow::Result<()> {
    let base = add::install_base(base_dir, global)?;
    let skill_dir = base.join(name);

    if skill_dir.is_dir() {
        std::fs::remove_dir_all(&skill_dir)?;
        println!("Removed {}", skill_dir.display());
    } else {
        println!("Skill '{}' not found at {}", name, skill_dir.display());
    }

    lock::remove_entry(base_dir, name)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manage::lock;
    use crate::test_utils::IsolatedEnv;

    fn make_lock_entry(name: &str) -> lock::SkillLockEntry {
        lock::SkillLockEntry {
            source: "acme/skills".into(),
            source_type: "github".into(),
            source_url: "https://github.com/acme/skills.git".into(),
            skill_path: Some(format!("{name}.toml")),
            skill_folder_hash: "".into(),
            installed_at: "2026-03-20T00:00:00Z".into(),
            updated_at: "2026-03-20T00:00:00Z".into(),
            plugin_name: None,
        }
    }

    #[test]
    fn remove_local_skill_directory_when_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).unwrap();

        let local = tmp.path().join(".opengoose/skills/installed/demo-skill");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(local.join("SKILL.md"), "skill").unwrap();

        lock::add_entry(tmp.path(), "demo-skill", make_lock_entry("demo-skill")).unwrap();
        run(tmp.path(), "demo-skill", false).unwrap();

        assert!(!local.exists());
        assert!(
            !lock::read_lock(tmp.path())
                .skills
                .contains_key("demo-skill")
        );
    }

    #[test]
    fn remove_global_skill_directory_when_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = IsolatedEnv::new(tmp.path());

        let global = tmp.path().join(".opengoose/skills/installed/global-skill");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(global.join("SKILL.md"), "skill").unwrap();

        run(tmp.path(), "global-skill", true).unwrap();
        assert!(!global.exists());
    }

    #[test]
    fn remove_nonexistent_skill_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).unwrap();

        run(tmp.path(), "does-not-exist", false).unwrap();
        run(tmp.path(), "does-not-exist", true).unwrap();

        assert_eq!(lock::read_lock(tmp.path()).skills.len(), 0);
    }
}
