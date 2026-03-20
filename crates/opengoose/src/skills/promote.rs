use anyhow::bail;
use std::path::{Path, PathBuf};

pub fn run(name: &str, to: &str, from_rig: Option<&str>, force: bool) -> anyhow::Result<()> {
    // 1. Find the skill in rig scope
    let source = find_rig_skill(name, from_rig)?;

    // 2. Determine target directory
    let target = match to {
        "project" => PathBuf::from(".opengoose/skills/learned").join(name),
        "global" => {
            let home = dirs::home_dir().unwrap_or_else(|| ".".into());
            home.join(".opengoose/skills/learned").join(name)
        }
        _ => bail!("invalid target: {to} (expected 'project' or 'global')"),
    };

    // 3. Check if target exists
    if target.exists() && !force {
        bail!(
            "skill '{name}' already exists at {}. Use --force to overwrite.",
            target.display()
        );
    }

    // 4. Copy skill directory
    std::fs::create_dir_all(&target)?;
    copy_dir_contents(&source, &target)?;

    // 5. Update metadata.json with promotion info
    let meta_path = target.join("metadata.json");
    if meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert(
                        "promoted_to".into(),
                        serde_json::Value::String(to.to_string()),
                    );
                    obj.insert(
                        "promoted_at".into(),
                        serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
                    );
                    if let Ok(json) = serde_json::to_string_pretty(&meta) {
                        let _ = std::fs::write(&meta_path, json);
                    }
                }
            }
        }
    }

    let rig_name = source
        .ancestors()
        .nth(3) // learned/{name} -> skills -> {rig-id}
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    println!("Promoted '{name}' from rig:{rig_name} → {to}");
    println!("  {}", target.display());
    Ok(())
}

fn find_rig_skill(name: &str, from_rig: Option<&str>) -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let rigs_base = home.join(".opengoose/rigs");

    if let Some(rig) = from_rig {
        let path = rigs_base.join(rig).join("skills/learned").join(name);
        if path.is_dir() && path.join("SKILL.md").is_file() {
            return Ok(path);
        }
        bail!("skill '{name}' not found in rig '{rig}'");
    }

    // Search all rigs
    if rigs_base.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&rigs_base) {
            for entry in entries.flatten() {
                let path = entry.path().join("skills/learned").join(name);
                if path.is_dir() && path.join("SKILL.md").is_file() {
                    return Ok(path);
                }
            }
        }
    }

    bail!("skill '{name}' not found in any rig. Use 'opengoose skills list' to see available skills.")
}

fn copy_dir_contents(src: &Path, dst: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_file() {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::ffi::OsString;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_isolated_env(tmp: &std::path::Path) {
        unsafe {
            env::set_var("HOME", tmp);
        }
        env::set_current_dir(tmp).unwrap();
    }

    fn restore_env(home: Option<OsString>, cwd: std::path::PathBuf) {
        unsafe {
            match home {
                Some(v) => env::set_var("HOME", v),
                None => env::remove_var("HOME"),
            }
        }
        env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn find_rig_skill_specific_rig() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("rigs/worker-1/skills/learned/my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Use when testing\n---\n",
        )
        .unwrap();

        // Can't test find_rig_skill directly because it uses dirs::home_dir()
        // Instead test copy_dir_contents
        let dst = tmp.path().join("target");
        std::fs::create_dir_all(&dst).unwrap();
        copy_dir_contents(&skill_dir, &dst).unwrap();
        assert!(dst.join("SKILL.md").exists());
    }

    #[test]
    fn copy_preserves_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "skill content").unwrap();
        std::fs::write(src.join("metadata.json"), "{}").unwrap();

        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&dst).unwrap();
        copy_dir_contents(&src, &dst).unwrap();

        assert!(dst.join("SKILL.md").exists());
        assert!(dst.join("metadata.json").exists());
        assert_eq!(
            std::fs::read_to_string(dst.join("SKILL.md")).unwrap(),
            "skill content"
        );
    }

    #[test]
    fn find_rig_skill_from_named_rig_with_isolated_home() {
        let _guard = ENV_LOCK.lock().unwrap();
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_env(tmp.path());

        let skill_dir = tmp.path().join(".opengoose/rigs/r1/skills/learned/my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: my-skill\ndescription: Use when testing\n---\n").unwrap();

        let found = find_rig_skill("my-skill", Some("r1")).unwrap();
        assert_eq!(found, skill_dir);

        restore_env(home, cwd);
    }

    #[test]
    fn run_promote_rejects_invalid_target() {
        let _guard = ENV_LOCK.lock().unwrap();
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_env(tmp.path());

        let source = tmp.path().join(".opengoose/rigs/r1/skills/learned/s1");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: s1\ndescription: Use when testing\n---\n").unwrap();

        let err = run("s1", "workspace", Some("r1"), false).unwrap_err();
        assert!(err.to_string().contains("invalid target"));

        restore_env(home, cwd);
    }

    #[test]
    fn run_promote_to_project_writes_metadata() {
        let _guard = ENV_LOCK.lock().unwrap();
        let cwd = env::current_dir().unwrap();
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        with_isolated_env(tmp.path());

        let source = tmp.path().join(".opengoose/rigs/r1/skills/learned/skill-project");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: skill-project\ndescription: Use when testing\n---\n").unwrap();
        let metadata = serde_json::json!({
            "generated_from": {
                "stamp_id": 1,
                "work_item_id": 1,
                "dimension": "Quality",
                "score": 0.5
            },
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": {
                "injected_count": 0,
                "subsequent_scores": []
            }
        });
        std::fs::write(source.join("metadata.json"), serde_json::to_string_pretty(&metadata).unwrap()).unwrap();

        run("skill-project", "project", Some("r1"), false).unwrap();

        let target = tmp.path().join(".opengoose/skills/learned/skill-project");
        assert!(target.join("SKILL.md").is_file());
        let meta = std::fs::read_to_string(target.join("metadata.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&meta).unwrap();
        assert_eq!(parsed["promoted_to"], "project");

        restore_env(home, cwd);
    }
}
