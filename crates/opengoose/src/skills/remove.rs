use crate::skills::{add, lock};

pub fn run(name: &str, global: bool) -> anyhow::Result<()> {
    let base = add::install_base(global)?;
    let skill_dir = base.join(name);

    if skill_dir.is_dir() {
        std::fs::remove_dir_all(&skill_dir)?;
        println!("Removed {}", skill_dir.display());
    } else {
        println!("Skill '{}' not found at {}", name, skill_dir.display());
    }

    lock::remove_entry(name)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::ffi::OsString;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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

    fn with_isolated_env(tmp: &std::path::Path) {
        unsafe {
            env::set_var("HOME", tmp);
            env::set_var("XDG_STATE_HOME", tmp.join("state"));
        }
        env::set_current_dir(tmp).unwrap();
    }

    fn restore_env(home: Option<OsString>, xdg_state: Option<OsString>, cwd: std::path::PathBuf) {
        unsafe {
            match home {
                Some(v) => env::set_var("HOME", v),
                None => env::remove_var("HOME"),
            }
            match xdg_state {
                Some(v) => env::set_var("XDG_STATE_HOME", v),
                None => env::remove_var("XDG_STATE_HOME"),
            }
        }
        env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn remove_local_skill_directory_when_exists() {
        let _guard = ENV_LOCK.lock().unwrap();
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let xdg_state = env::var_os("XDG_STATE_HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_env(tmp.path());

        let local = tmp.path().join(".opengoose/skills/installed/demo-skill");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(local.join("SKILL.md"), "skill").unwrap();

        lock::add_entry("demo-skill", make_lock_entry("demo-skill")).unwrap();
        run("demo-skill", false).unwrap();

        assert!(!local.exists());
        assert!(!lock::read_lock().skills.contains_key("demo-skill"));

        restore_env(home, xdg_state, cwd);
    }

    #[test]
    fn remove_global_skill_directory_when_exists() {
        let _guard = ENV_LOCK.lock().unwrap();
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let xdg_state = env::var_os("XDG_STATE_HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_env(tmp.path());

        let global = tmp.path().join(".opengoose/skills/installed/global-skill");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(global.join("SKILL.md"), "skill").unwrap();

        run("global-skill", true).unwrap();
        assert!(!global.exists());

        restore_env(home, xdg_state, cwd);
    }

    #[test]
    fn remove_nonexistent_skill_is_noop() {
        let _guard = ENV_LOCK.lock().unwrap();
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let xdg_state = env::var_os("XDG_STATE_HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_env(tmp.path());

        run("does-not-exist", false).unwrap();
        run("does-not-exist", true).unwrap();

        assert_eq!(lock::read_lock().skills.len(), 0);
        restore_env(home, xdg_state, cwd);
    }
}
