use crate::SkillError;
use crate::manage::{add, lock};
use std::path::Path;

pub fn run(base_dir: &Path, name: &str, global: bool) -> Result<(), SkillError> {
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
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).expect("set_current_dir should succeed");

        let local = tmp.path().join(".opengoose/skills/installed/demo-skill");
        std::fs::create_dir_all(&local).expect("directory creation should succeed");
        std::fs::write(local.join("SKILL.md"), "skill").expect("test fixture write should succeed");

        lock::add_entry(tmp.path(), "demo-skill", make_lock_entry("demo-skill"))
            .expect("add_entry should succeed");
        run(tmp.path(), "demo-skill", false).expect("skill remove should succeed");

        assert!(!local.exists());
        assert!(
            !lock::read_lock(tmp.path())
                .skills
                .contains_key("demo-skill")
        );
    }

    #[test]
    fn remove_global_skill_directory_when_exists() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        let global = tmp.path().join(".opengoose/skills/installed/global-skill");
        std::fs::create_dir_all(&global).expect("directory creation should succeed");
        std::fs::write(global.join("SKILL.md"), "skill")
            .expect("test fixture write should succeed");

        run(tmp.path(), "global-skill", true).expect("skill remove should succeed");
        assert!(!global.exists());
    }

    #[test]
    fn remove_nonexistent_skill_is_noop() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).expect("set_current_dir should succeed");

        run(tmp.path(), "does-not-exist", false).expect("noop remove should succeed");
        run(tmp.path(), "does-not-exist", true).expect("noop remove should succeed");

        assert_eq!(lock::read_lock(tmp.path()).skills.len(), 0);
    }
}
