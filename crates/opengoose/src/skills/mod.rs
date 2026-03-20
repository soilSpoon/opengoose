use clap::Subcommand;

pub mod evolve;
pub mod load;

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
    /// Promote a learned skill to project or global scope
    Promote {
        /// Skill name to promote
        name: String,
        /// Target scope: "project" or "global"
        #[arg(long)]
        to: String,
        /// Source rig (default: search all rigs)
        #[arg(long)]
        from_rig: Option<String>,
        /// Overwrite if target exists
        #[arg(long)]
        force: bool,
    },
}

pub async fn run_skills_command(action: SkillsAction) -> anyhow::Result<()> {
    let base_dir = crate::home_dir();
    match action {
        SkillsAction::Add {
            source,
            all,
            skill,
            global,
        } => opengoose_skills::manage::add::run(&base_dir, &source, all, skill.as_deref(), global).await,
        SkillsAction::List { global, archived } => {
            opengoose_skills::manage::list::run(&base_dir, global, archived)
        }
        SkillsAction::Remove { name, global } => {
            opengoose_skills::manage::remove::run(&base_dir, &name, global)
        }
        SkillsAction::Update => opengoose_skills::manage::update::run(&base_dir).await,
        SkillsAction::Promote {
            name,
            to,
            from_rig,
            force,
        } => opengoose_skills::manage::promote::run(&base_dir, &name, &to, from_rig.as_deref(), force),
    }
}
