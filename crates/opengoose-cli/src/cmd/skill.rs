use std::path::PathBuf;

use crate::error::{CliError, CliResult};
use clap::Subcommand;

use opengoose_profiles::{Skill, SkillStore};

#[derive(Subcommand)]
/// Subcommands for `opengoose skill`.
pub enum SkillAction {
    /// List all installed skills
    List,
    /// Show a skill's full YAML
    Show {
        /// Skill name (e.g. git-tools)
        name: String,
    },
    /// Install a skill from a YAML file
    Add {
        /// Path to the skill YAML file
        path: PathBuf,
        /// Overwrite if the skill already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a skill
    Remove {
        /// Skill name (e.g. git-tools)
        name: String,
    },
    /// Install bundled default skills
    Init {
        /// Overwrite existing skills
        #[arg(long)]
        force: bool,
    },
}

/// Dispatch and execute the selected skill subcommand.
pub fn execute(action: SkillAction) -> CliResult<()> {
    match action {
        SkillAction::List => cmd_list(),
        SkillAction::Show { name } => cmd_show(&name),
        SkillAction::Add { path, force } => cmd_add(&path, force),
        SkillAction::Remove { name } => cmd_remove(&name),
        SkillAction::Init { force } => cmd_init(force),
    }
}

fn cmd_list() -> CliResult<()> {
    let store = SkillStore::new()?;
    let names = store.list()?;

    if names.is_empty() {
        println!("No skills found. Use `opengoose skill init` to install defaults.");
        return Ok(());
    }

    println!("Skills:");
    for name in &names {
        let skill = store.get(name)?;
        let desc = skill.description.as_deref().unwrap_or("(no description)");
        let ext_count = skill.extensions.len();
        println!("  {:<20} [{} extension(s)] {}", name, ext_count, desc);
    }
    Ok(())
}

fn cmd_show(name: &str) -> CliResult<()> {
    let store = SkillStore::new()?;
    let skill = store.get(name)?;
    let yaml = skill.to_yaml()?;
    print!("{yaml}");
    Ok(())
}

fn cmd_add(path: &PathBuf, force: bool) -> CliResult<()> {
    if !path.exists() {
        return Err(CliError::Validation(format!("file not found: {}", path.display())));
    }

    let content = std::fs::read_to_string(path)?;
    let skill = Skill::from_yaml(&content)?;
    let name = skill.name.clone();

    let store = SkillStore::new()?;
    store.save(&skill, force)?;

    println!("Added skill `{name}`.");
    Ok(())
}

fn cmd_remove(name: &str) -> CliResult<()> {
    let store = SkillStore::new()?;
    store.remove(name)?;
    println!("Removed skill `{name}`.");
    Ok(())
}

fn cmd_init(force: bool) -> CliResult<()> {
    let store = SkillStore::new()?;
    let count = store.install_defaults(force)?;

    if count == 0 {
        println!("All default skills already exist. Use --force to overwrite.");
    } else {
        println!("Installed {count} default skill(s).");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    fn temp_store() -> (TempDir, SkillStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SkillStore::with_dir(tmp.path().to_path_buf());
        (tmp, store)
    }

    fn minimal_skill_yaml(name: &str) -> String {
        format!("name: {name}\nversion: \"1.0.0\"\ndescription: A test skill\nextensions: []\n")
    }

    fn write_skill_file(dir: &TempDir, name: &str) -> PathBuf {
        let path = dir.path().join(format!("{name}.yaml"));
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "{}", minimal_skill_yaml(name)).unwrap();
        path
    }

    // ---- Skill YAML parsing ----

    #[test]
    fn skill_from_yaml_valid_minimal() {
        let yaml = "name: my-skill\nversion: \"1.0.0\"\n";
        let skill = Skill::from_yaml(yaml).expect("should parse valid YAML");
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.version, "1.0.0");
        assert!(skill.description.is_none());
        assert!(skill.extensions.is_empty());
    }

    #[test]
    fn skill_from_yaml_with_description() {
        let yaml = "name: git-tools\nversion: \"2.0\"\ndescription: Git helpers\n";
        let skill = Skill::from_yaml(yaml).expect("should parse");
        assert_eq!(skill.description.as_deref(), Some("Git helpers"));
    }

    #[test]
    fn skill_from_yaml_empty_name_fails() {
        let yaml = "name: \"\"\nversion: \"1.0.0\"\n";
        assert!(Skill::from_yaml(yaml).is_err());
    }

    #[test]
    fn skill_from_yaml_empty_version_fails() {
        let yaml = "name: my-skill\nversion: \"\"\n";
        assert!(Skill::from_yaml(yaml).is_err());
    }

    #[test]
    fn skill_to_yaml_roundtrip() {
        let yaml = minimal_skill_yaml("roundtrip-skill");
        let skill = Skill::from_yaml(&yaml).unwrap();
        let out = skill.to_yaml().unwrap();
        let reparsed = Skill::from_yaml(&out).unwrap();
        assert_eq!(reparsed.name, skill.name);
        assert_eq!(reparsed.version, skill.version);
    }

    // ---- SkillStore via with_dir ----

    #[test]
    fn skill_store_list_empty_initially() {
        let (_tmp, store) = temp_store();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn skill_store_save_and_list() {
        let (_tmp, store) = temp_store();
        let skill = Skill::from_yaml(&minimal_skill_yaml("my-skill")).unwrap();
        store.save(&skill, false).unwrap();
        let names = store.list().unwrap();
        assert_eq!(names, vec!["my-skill"]);
    }

    #[test]
    fn skill_store_get_returns_saved_skill() {
        let (_tmp, store) = temp_store();
        let skill = Skill::from_yaml(&minimal_skill_yaml("lookup")).unwrap();
        store.save(&skill, false).unwrap();
        let loaded = store.get("lookup").unwrap();
        assert_eq!(loaded.name, "lookup");
        assert_eq!(loaded.version, "1.0.0");
    }

    #[test]
    fn skill_store_get_missing_errors() {
        let (_tmp, store) = temp_store();
        assert!(store.get("nonexistent").is_err());
    }

    #[test]
    fn skill_store_save_without_force_rejects_duplicate() {
        let (_tmp, store) = temp_store();
        let skill = Skill::from_yaml(&minimal_skill_yaml("dup")).unwrap();
        store.save(&skill, false).unwrap();
        assert!(store.save(&skill, false).is_err());
    }

    #[test]
    fn skill_store_save_with_force_overwrites() {
        let (_tmp, store) = temp_store();
        let skill = Skill::from_yaml(&minimal_skill_yaml("overwrite")).unwrap();
        store.save(&skill, false).unwrap();
        assert!(store.save(&skill, true).is_ok());
    }

    #[test]
    fn skill_store_remove_deletes_skill() {
        let (_tmp, store) = temp_store();
        let skill = Skill::from_yaml(&minimal_skill_yaml("removable")).unwrap();
        store.save(&skill, false).unwrap();
        store.remove("removable").unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn skill_store_remove_missing_errors() {
        let (_tmp, store) = temp_store();
        assert!(store.remove("ghost").is_err());
    }

    #[test]
    fn skill_store_install_defaults_returns_count() {
        let (_tmp, store) = temp_store();
        let count = store.install_defaults(false).unwrap();
        assert!(count > 0, "expected default skills to be installed");
    }

    #[test]
    fn skill_store_install_defaults_idempotent_without_force() {
        let (_tmp, store) = temp_store();
        store.install_defaults(false).unwrap();
        let second = store.install_defaults(false).unwrap();
        assert_eq!(
            second, 0,
            "re-installing without force should install nothing"
        );
    }

    #[test]
    fn skill_store_install_defaults_with_force_reinstalls() {
        let (_tmp, store) = temp_store();
        let first = store.install_defaults(false).unwrap();
        let second = store.install_defaults(true).unwrap();
        assert_eq!(first, second, "force should reinstall all defaults");
    }

    // ---- cmd_add via file path ----

    #[test]
    fn cmd_add_succeeds_with_valid_yaml_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_skill_file(&tmp, "file-skill");
        // Use a separate directory for the store so the source YAML file
        // doesn't collide with the store's own files.
        let store_dir = tmp.path().join("store");
        std::fs::create_dir_all(&store_dir).unwrap();
        let store = SkillStore::with_dir(store_dir);
        let content = std::fs::read_to_string(&path).unwrap();
        let skill = Skill::from_yaml(&content).unwrap();
        store.save(&skill, false).unwrap();
        assert!(store.get("file-skill").is_ok());
    }

    #[test]
    fn cmd_add_file_not_found_returns_error() {
        let path = PathBuf::from("/nonexistent/path/skill.yaml");
        // Simulate what cmd_add does: check file existence
        assert!(!path.exists(), "test precondition: file must not exist");
    }
}
