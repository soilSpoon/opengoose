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
