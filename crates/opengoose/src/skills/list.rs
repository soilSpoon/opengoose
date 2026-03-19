use crate::skills::{add, discover, lock};
use std::path::Path;

pub fn run(global_only: bool) -> anyhow::Result<()> {
    let lock_data = lock::read_lock();
    let mut found_any = false;

    if !global_only {
        let project_path = std::path::PathBuf::from(".goose/skills");
        if print_scope("Project skills (.goose/skills/)", &project_path, &lock_data) {
            found_any = true;
        }
    }

    let global_path = add::install_base(true)?;
    if print_scope(
        &format!("Global skills ({})", global_path.display()),
        &global_path,
        &lock_data,
    ) {
        found_any = true;
    }

    if !found_any {
        println!("No skills installed. Use 'opengoose skills add' to install skills.");
    }

    Ok(())
}

fn print_scope(header: &str, path: &Path, lock_data: &lock::SkillLockFile) -> bool {
    if !path.is_dir() {
        return false;
    }

    let skills = discover::discover_skills(path);
    if skills.is_empty() {
        return false;
    }

    println!("\n{header}:");
    for skill in &skills {
        let source = lock_data
            .skills
            .get(&skill.name)
            .map(|e| e.source.as_str())
            .unwrap_or("local");
        println!("  {:<20} — {:<40} ({})", skill.name, skill.description, source);
    }
    true
}
