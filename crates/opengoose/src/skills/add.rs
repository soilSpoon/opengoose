use crate::skills::{discover, lock, source};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub async fn run(src: &str, all: bool, skill_filter: Option<&str>, global: bool) -> Result<()> {
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

    let target_base = install_base(global)?;
    for skill in &selected {
        install_skill(skill, &target_base)?;
        let now = lock::now_iso();
        lock::add_entry(
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

pub fn install_base(global: bool) -> Result<PathBuf> {
    if global {
        let home = dirs::home_dir().unwrap_or_else(|| ".".into());
        Ok(home.join(".opengoose/skills/installed"))
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
    use super::*;
    use crate::skills::discover::DiscoveredSkill;

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
        let global = install_base(true).unwrap();
        assert!(global.to_string_lossy().ends_with(".opengoose/skills/installed"));

        let local = install_base(false).unwrap();
        assert_eq!(local, PathBuf::from(".opengoose/skills/installed"));
    }

    #[test]
    fn copy_dir_recursive_skips_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join(".hidden")).unwrap();
        std::fs::write(src.join(".hidden").join("skip.me"), "1").unwrap_or_else(|_| ());
        std::fs::write(src.join("__pycache__"), "1").unwrap_or_else(|_| ());
        std::fs::write(src.join("ok.txt"), "ok").unwrap();

        let dst = tmp.path().join("dst");
        copy_dir_recursive(&src, &dst).unwrap();

        assert!(!dst.join(".hidden").exists());
        assert!(!dst.join("__pycache__").exists());
        assert!(dst.join("ok.txt").exists());
    }
}
