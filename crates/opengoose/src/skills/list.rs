use crate::skills::load::{load_skills_3_scope, LoadedSkill, SkillScope};
use std::path::PathBuf;

pub fn run(global_only: bool) -> anyhow::Result<()> {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let global_dir = home.join(".opengoose/skills");
    let rigs_base = home.join(".opengoose/rigs");

    let project_dir = if global_only {
        None
    } else {
        let p = PathBuf::from(".opengoose/skills");
        if p.is_dir() { Some(p) } else { None }
    };

    let skills = load_skills_3_scope(
        &global_dir,
        project_dir.as_deref(),
        None, // rig_id not available from CLI list
        &rigs_base,
    );

    if skills.is_empty() {
        println!("No skills installed. Use 'opengoose skills add' to install skills.");
        return Ok(());
    }

    // Group by scope for display
    let global_installed: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| s.scope == SkillScope::Installed && is_under(&s.path, &global_dir))
        .collect();
    let global_learned: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| s.scope == SkillScope::Learned && is_under(&s.path, &global_dir))
        .collect();
    let project_installed: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| {
            s.scope == SkillScope::Installed
                && project_dir
                    .as_ref()
                    .map_or(false, |p| is_under(&s.path, p))
        })
        .collect();
    let project_learned: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| {
            s.scope == SkillScope::Learned
                && project_dir
                    .as_ref()
                    .map_or(false, |p| is_under(&s.path, p))
        })
        .collect();

    print_group("Global (installed)", &global_installed);
    print_group("Global (learned)", &global_learned);
    print_group("Project (installed)", &project_installed);
    print_group("Project (learned)", &project_learned);

    Ok(())
}

fn is_under(path: &std::path::Path, base: &std::path::Path) -> bool {
    // Canonicalize when possible, fall back to starts_with
    let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let canon_base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    canon_path.starts_with(&canon_base)
}

fn print_group(header: &str, skills: &[&LoadedSkill]) {
    if skills.is_empty() {
        return;
    }
    println!("\n{header}:");
    for skill in skills {
        let scope_label = match skill.scope {
            SkillScope::Installed => "installed",
            SkillScope::Learned => "learned",
        };
        println!(
            "  {:<20} — {:<40} ({})",
            skill.name, skill.description, scope_label
        );
    }
}
