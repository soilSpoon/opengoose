use anyhow::bail;
use std::path::{Path, PathBuf};

pub fn run(
    base_dir: &Path,
    name: &str,
    to: &str,
    from_rig: Option<&str>,
    force: bool,
) -> anyhow::Result<()> {
    // 1. Find the skill in rig scope
    let source = find_rig_skill(base_dir, name, from_rig)?;

    // 2. Determine target directory
    let target = resolve_target_path(base_dir, name, to)?;

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
    if meta_path.exists()
        && let Ok(content) = std::fs::read_to_string(&meta_path)
        && let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(&content)
        && apply_promotion_metadata(&mut meta, to)
        && let Ok(json) = serde_json::to_string_pretty(&meta)
    {
        let _ = std::fs::write(&meta_path, json);
    }

    let rig_name = extract_rig_name(&source).unwrap_or_else(|| "unknown".into());

    println!("Promoted '{name}' from rig:{rig_name} → {to}");
    println!("  {}", target.display());
    Ok(())
}

fn resolve_target_path(base_dir: &Path, name: &str, to: &str) -> anyhow::Result<PathBuf> {
    match to {
        "project" => Ok(PathBuf::from(".opengoose/skills/learned").join(name)),
        "global" => Ok(base_dir.join(".opengoose/skills/learned").join(name)),
        _ => anyhow::bail!("invalid target: {to} (expected 'project' or 'global')"),
    }
}

fn extract_rig_name(source: &Path) -> Option<String> {
    // source is .../rigs/<rig-id>/skills/learned/<name>
    // ancestors: learned/{name} → skills → {rig-id}
    source
        .ancestors()
        .nth(3)
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
}

fn apply_promotion_metadata(meta: &mut serde_json::Value, to: &str) -> bool {
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "promoted_to".into(),
            serde_json::Value::String(to.to_string()),
        );
        obj.insert(
            "promoted_at".into(),
            serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
        );
        true
    } else {
        false
    }
}

fn find_rig_skill(base_dir: &Path, name: &str, from_rig: Option<&str>) -> anyhow::Result<PathBuf> {
    let rigs_base = base_dir.join(".opengoose/rigs");

    if let Some(rig) = from_rig {
        let path = rigs_base.join(rig).join("skills/learned").join(name);
        if path.is_dir() && path.join("SKILL.md").is_file() {
            return Ok(path);
        }
        bail!("skill '{name}' not found in rig '{rig}'");
    }

    // Search all rigs
    if rigs_base.is_dir()
        && let Ok(entries) = std::fs::read_dir(&rigs_base)
    {
        for entry in entries.flatten() {
            let path = entry.path().join("skills/learned").join(name);
            if path.is_dir() && path.join("SKILL.md").is_file() {
                return Ok(path);
            }
        }
    }

    bail!(
        "skill '{name}' not found in any rig. Use 'opengoose skills list' to see available skills."
    )
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
    use crate::test_utils::IsolatedEnv;

    #[test]
    fn find_rig_skill_specific_rig() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let skill_dir = tmp
            .path()
            .join(".opengoose/rigs/worker-1/skills/learned/my-skill");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");

        // Test copy_dir_contents directly
        let dst = tmp.path().join("target");
        std::fs::create_dir_all(&dst).expect("create target dir");
        copy_dir_contents(&skill_dir, &dst).expect("copy dir contents");
        assert!(dst.join("SKILL.md").exists());
    }

    #[test]
    fn copy_preserves_files() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).expect("create src dir");
        std::fs::write(src.join("SKILL.md"), "skill content").expect("write SKILL.md");
        std::fs::write(src.join("metadata.json"), "{}").expect("write metadata.json");

        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&dst).expect("create dst dir");
        copy_dir_contents(&src, &dst).expect("copy dir contents");

        assert!(dst.join("SKILL.md").exists());
        assert!(dst.join("metadata.json").exists());
        assert_eq!(
            std::fs::read_to_string(dst.join("SKILL.md")).expect("read SKILL.md"),
            "skill content"
        );
    }

    #[test]
    fn find_rig_skill_from_named_rig_with_isolated_home() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());

        let skill_dir = tmp
            .path()
            .join(".opengoose/rigs/r1/skills/learned/my-skill");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");

        let found = find_rig_skill(tmp.path(), "my-skill", Some("r1")).expect("find rig skill");
        assert_eq!(found, skill_dir);
    }

    #[test]
    fn run_promote_rejects_invalid_target() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).expect("set current dir");

        let source = tmp.path().join(".opengoose/rigs/r1/skills/learned/s1");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: s1\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");

        let err = run(tmp.path(), "s1", "workspace", Some("r1"), false)
            .expect_err("should reject invalid target");
        assert!(err.to_string().contains("invalid target"));
    }

    #[test]
    fn run_promote_to_global_succeeds() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());

        let source = tmp
            .path()
            .join(".opengoose/rigs/r1/skills/learned/global-skill");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: global-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");

        run(tmp.path(), "global-skill", "global", Some("r1"), false).expect("promote to global");

        let target = tmp.path().join(".opengoose/skills/learned/global-skill");
        assert!(target.join("SKILL.md").is_file());
    }

    #[test]
    fn run_promote_fails_when_target_exists_without_force() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).expect("set current dir");

        let source = tmp
            .path()
            .join(".opengoose/rigs/r1/skills/learned/dup-skill");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: dup-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");

        // First promote succeeds
        run(tmp.path(), "dup-skill", "project", Some("r1"), false).expect("first promote");

        // Second without force fails
        let err = run(tmp.path(), "dup-skill", "project", Some("r1"), false)
            .expect_err("should fail without force");
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn run_promote_with_force_overwrites_existing_target() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).expect("set current dir");

        let source = tmp
            .path()
            .join(".opengoose/rigs/r1/skills/learned/force-skill");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: force-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");

        run(tmp.path(), "force-skill", "project", Some("r1"), false).expect("first promote");
        run(tmp.path(), "force-skill", "project", Some("r1"), true).expect("force promote"); // force = true
    }

    #[test]
    fn find_rig_skill_returns_error_when_named_rig_skill_missing() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());

        // Rig exists but skill doesn't
        std::fs::create_dir_all(tmp.path().join(".opengoose/rigs/r1/skills/learned"))
            .expect("create rig dir");

        let err = find_rig_skill(tmp.path(), "missing-skill", Some("r1"))
            .expect_err("should fail for missing skill");
        assert!(err.to_string().contains("not found in rig"));
    }

    #[test]
    fn find_rig_skill_searches_all_rigs_when_no_rig_specified() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());

        let skill_dir = tmp
            .path()
            .join(".opengoose/rigs/worker-x/skills/learned/auto-skill");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: auto-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");

        let found =
            find_rig_skill(tmp.path(), "auto-skill", None).expect("find auto-discovered skill");
        assert!(found.ends_with("auto-skill"));
    }

    #[test]
    fn find_rig_skill_returns_error_when_rigs_exist_but_skill_absent() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());

        // Rigs dir exists and has a rig, but rig doesn't have the target skill
        let rig_dir = tmp
            .path()
            .join(".opengoose/rigs/r1/skills/learned/other-skill");
        std::fs::create_dir_all(&rig_dir).expect("create rig dir");
        std::fs::write(
            rig_dir.join("SKILL.md"),
            "---\nname: other-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");

        let err = find_rig_skill(tmp.path(), "missing-skill", None)
            .expect_err("should fail for absent skill");
        assert!(err.to_string().contains("not found in any rig"));
    }

    #[test]
    fn copy_dir_contents_skips_subdirectories() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).expect("create src dir");
        std::fs::write(src.join("SKILL.md"), "skill content").expect("write SKILL.md");
        // Add a subdirectory — should be skipped (not a file)
        std::fs::create_dir_all(src.join("subdir")).expect("create subdir");
        std::fs::write(src.join("subdir").join("nested.txt"), "nested").expect("write nested.txt");

        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&dst).expect("create dst dir");
        copy_dir_contents(&src, &dst).expect("copy dir contents");

        assert!(dst.join("SKILL.md").exists());
        assert!(!dst.join("subdir").exists());
    }

    #[test]
    fn run_promote_with_metadata_invalid_json_skips_metadata_update() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).expect("set current dir");

        let source = tmp
            .path()
            .join(".opengoose/rigs/r1/skills/learned/bad-json-skill");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: bad-json-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");
        std::fs::write(source.join("metadata.json"), "{ not valid json }")
            .expect("write invalid metadata.json");

        // Should succeed: invalid metadata.json is simply skipped
        run(tmp.path(), "bad-json-skill", "project", Some("r1"), false)
            .expect("promote with bad json");
        let target = tmp.path().join(".opengoose/skills/learned/bad-json-skill");
        assert!(target.join("SKILL.md").is_file());
    }

    #[test]
    fn run_promote_with_metadata_non_object_skips_insert() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).expect("set current dir");

        let source = tmp
            .path()
            .join(".opengoose/rigs/r1/skills/learned/array-meta-skill");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: array-meta-skill\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");
        std::fs::write(source.join("metadata.json"), "[1, 2, 3]")
            .expect("write array metadata.json");

        // Should succeed: non-object metadata.json is skipped
        run(tmp.path(), "array-meta-skill", "project", Some("r1"), false)
            .expect("promote with array meta");
        let target = tmp
            .path()
            .join(".opengoose/skills/learned/array-meta-skill");
        assert!(target.join("SKILL.md").is_file());
    }

    #[test]
    fn find_rig_skill_returns_error_when_no_rigs_dir() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());

        // No .opengoose/rigs dir at all
        let err = find_rig_skill(tmp.path(), "any-skill", None)
            .expect_err("should fail with no rigs dir");
        assert!(err.to_string().contains("not found in any rig"));
    }

    #[test]
    fn run_promote_to_project_writes_metadata() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let _env = IsolatedEnv::new(tmp.path());
        std::env::set_current_dir(tmp.path()).expect("set current dir");

        let source = tmp
            .path()
            .join(".opengoose/rigs/r1/skills/learned/skill-project");
        std::fs::create_dir_all(&source).expect("create source dir");
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: skill-project\ndescription: Use when testing\n---\n",
        )
        .expect("write SKILL.md");
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
        std::fs::write(
            source.join("metadata.json"),
            serde_json::to_string_pretty(&metadata).expect("serialize metadata"),
        )
        .expect("write metadata.json");

        run(tmp.path(), "skill-project", "project", Some("r1"), false).expect("promote to project");

        let target = tmp.path().join(".opengoose/skills/learned/skill-project");
        assert!(target.join("SKILL.md").is_file());
        let meta =
            std::fs::read_to_string(target.join("metadata.json")).expect("read promoted metadata");
        let parsed: serde_json::Value =
            serde_json::from_str(&meta).expect("parse promoted metadata");
        assert_eq!(parsed["promoted_to"], "project");
    }

    // --- Tests for extracted pure functions ---

    #[test]
    fn resolve_target_path_project() {
        let path = resolve_target_path(Path::new("/home/user"), "my-skill", "project")
            .expect("resolve project path");
        assert_eq!(path, PathBuf::from(".opengoose/skills/learned/my-skill"));
    }

    #[test]
    fn resolve_target_path_global() {
        let path = resolve_target_path(Path::new("/home/user"), "my-skill", "global")
            .expect("resolve global path");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.opengoose/skills/learned/my-skill")
        );
    }

    #[test]
    fn resolve_target_path_invalid() {
        let err = resolve_target_path(Path::new("/home/user"), "my-skill", "workspace")
            .expect_err("should reject invalid target");
        assert!(err.to_string().contains("invalid target"));
    }

    #[test]
    fn extract_rig_name_from_valid_path() {
        let path = PathBuf::from("/home/user/.opengoose/rigs/my-rig/skills/learned/foo");
        assert_eq!(extract_rig_name(&path), Some("my-rig".to_string()));
    }

    #[test]
    fn extract_rig_name_from_short_path() {
        let path = PathBuf::from("foo");
        assert_eq!(extract_rig_name(&path), None);
    }

    #[test]
    fn apply_promotion_metadata_sets_fields() {
        let mut meta = serde_json::json!({"name": "test-skill"});
        assert!(apply_promotion_metadata(&mut meta, "global"));
        assert_eq!(meta["promoted_to"], "global");
        assert!(meta["promoted_at"].as_str().is_some());
    }

    #[test]
    fn apply_promotion_metadata_non_object_returns_false() {
        let mut meta = serde_json::json!([1, 2, 3]);
        assert!(!apply_promotion_metadata(&mut meta, "project"));
    }
}
