use clap::Subcommand;

pub mod source;
pub mod discover;
pub mod evolve;
pub mod list;
pub mod load;
pub mod lock;
pub mod add;
pub mod remove;
pub mod update;

#[derive(Subcommand)]
pub enum SkillsAction {
    /// Install skills from a Git repository
    Add {
        /// Git source (owner/repo or full URL)
        source: String,
        /// Install all skills without prompting
        #[arg(long)]
        all: bool,
        /// Install specific skill by name
        #[arg(long)]
        skill: Option<String>,
        /// Install to global directory
        #[arg(long, short)]
        global: bool,
    },
    /// List installed skills
    List {
        /// Show only global skills
        #[arg(long, short)]
        global: bool,
        /// Show archived skills
        #[arg(long)]
        archived: bool,
    },
    /// Remove an installed skill
    Remove {
        /// Skill name to remove
        name: String,
        /// Remove from global directory
        #[arg(long, short)]
        global: bool,
    },
    /// Update all installed skills
    Update,
}

pub async fn run_skills_command(action: SkillsAction) -> anyhow::Result<()> {
    match action {
        SkillsAction::Add { source, all, skill, global } => {
            add::run(&source, all, skill.as_deref(), global).await
        }
        SkillsAction::List { global, archived } => list::run(global, archived),
        SkillsAction::Remove { name, global } => remove::run(&name, global),
        SkillsAction::Update => update::run().await,
    }
}
