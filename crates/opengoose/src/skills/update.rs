use crate::skills::{add, lock};
use std::collections::HashMap;
use std::path::PathBuf;

fn global_skills_dir() -> PathBuf {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| crate::home_dir().join(".config"));
    config_home.join("goose").join("skills")
}

pub async fn run() -> anyhow::Result<()> {
    let lock_data = lock::read_lock();
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
            let global = global_skills_dir().join(name).is_dir();
            if let Err(e) = add::run(source, false, Some(name.as_str()), global).await {
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
    use crate::ENV_LOCK;
    use std::env;
    use std::ffi::OsString;

    fn with_config_home(tmp: &std::path::Path) {
        unsafe {
            env::set_var("XDG_CONFIG_HOME", tmp);
        }
    }

    fn with_home(tmp: &std::path::Path) {
        unsafe {
            env::set_var("HOME", tmp);
        }
    }

    fn restore_env(xdg: Option<OsString>, home: Option<OsString>) {
        unsafe {
            match xdg {
                Some(v) => env::set_var("XDG_CONFIG_HOME", v),
                None => env::remove_var("XDG_CONFIG_HOME"),
            }
            match home {
                Some(v) => env::set_var("HOME", v),
                None => env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn global_skills_dir_is_under_goose_skills() {
        let path = global_skills_dir();
        assert!(path.ends_with("goose/skills"));
    }

    #[test]
    fn global_skills_dir_prefers_xdg_config_home() {
        let _guard = ENV_LOCK.lock().unwrap();
        let home = env::var_os("HOME");
        let xdg = env::var_os("XDG_CONFIG_HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_home(tmp.path());
        with_config_home(&tmp.path().join("config"));
        assert_eq!(
            global_skills_dir(),
            tmp.path().join("config").join("goose").join("skills")
        );
        restore_env(xdg, home);
    }
}
