use crate::skills::{add, lock};
use std::collections::HashMap;
use std::path::PathBuf;

fn global_skills_dir() -> PathBuf {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_else(|| ".".into()).join(".config"));
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
