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
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs::home_dir().unwrap_or_else(|| ".".into()).join(".config"));
        Ok(config_home.join("goose").join("skills"))
    } else {
        Ok(PathBuf::from(".goose/skills"))
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
