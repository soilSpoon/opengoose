use clap::Subcommand;

pub mod evolve;
pub mod load;

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    &crate::ENV_LOCK
}

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
        } => {
            opengoose_skills::manage::add::run(&base_dir, &source, all, skill.as_deref(), global)
                .await
        }
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
        } => opengoose_skills::manage::promote::run(
            &base_dir,
            &name,
            &to,
            from_rig.as_deref(),
            force,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::test_env_lock;
    use std::future::Future;
    use tempfile::tempdir;

    struct EnvGuard {
        home: Option<std::ffi::OsString>,
        opengoose_home: Option<std::ffi::OsString>,
        xdg_state_home: Option<std::ffi::OsString>,
        cwd: std::path::PathBuf,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.home {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
                match &self.opengoose_home {
                    Some(v) => std::env::set_var("OPENGOOSE_HOME", v),
                    None => std::env::remove_var("OPENGOOSE_HOME"),
                }
                match &self.xdg_state_home {
                    Some(v) => std::env::set_var("XDG_STATE_HOME", v),
                    None => std::env::remove_var("XDG_STATE_HOME"),
                }
                let _ = std::env::set_current_dir(&self.cwd);
            }
        }
    }

    #[allow(clippy::await_holding_lock)]
    async fn with_clean_home<F, Fut>(f: F)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ()>,
    {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempdir().expect("temp dir creation should succeed");
        let env_guard = EnvGuard {
            home: std::env::var_os("HOME"),
            opengoose_home: std::env::var_os("OPENGOOSE_HOME"),
            xdg_state_home: std::env::var_os("XDG_STATE_HOME"),
            cwd: std::env::current_dir().expect("operation should succeed"),
        };
        let state_home = tmp.path().join("state");
        std::fs::create_dir_all(&state_home).expect("directory creation should succeed");

        // Keep skills tests isolated from any real user home state.
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("OPENGOOSE_HOME", tmp.path());
            std::env::set_var("XDG_STATE_HOME", &state_home);
            std::env::set_current_dir(tmp.path()).expect("operation should succeed");
        }

        f().await;

        drop(env_guard);
        drop(guard);
    }

    #[tokio::test]
    async fn run_skills_command_dispatches_add() {
        with_clean_home(|| async {
            let result = run_skills_command(SkillsAction::Add {
                source: "bad-source-input".to_string(),
                all: true,
                skill: None,
                global: false,
            })
            .await;
            assert!(result.is_err());
        })
        .await;
    }

    #[tokio::test]
    async fn run_skills_command_dispatches_list() {
        with_clean_home(|| async {
            run_skills_command(SkillsAction::List {
                global: false,
                archived: false,
            })
            .await
            .expect("operation should succeed");
        })
        .await;
    }

    #[tokio::test]
    async fn run_skills_command_dispatches_remove() {
        with_clean_home(|| async {
            run_skills_command(SkillsAction::Remove {
                name: "missing".to_string(),
                global: false,
            })
            .await
            .expect("operation should succeed");
        })
        .await;
    }

    #[tokio::test]
    async fn run_skills_command_dispatches_update() {
        with_clean_home(|| async {
            run_skills_command(SkillsAction::Update)
                .await
                .expect("async operation should succeed");
        })
        .await;
    }

    #[tokio::test]
    async fn run_skills_command_dispatches_promote_missing_skill() {
        with_clean_home(|| async {
            let result = run_skills_command(SkillsAction::Promote {
                name: "does-not-exist".to_string(),
                to: "project".to_string(),
                from_rig: Some("missing-rig".to_string()),
                force: false,
            })
            .await;
            assert!(result.is_err());
        })
        .await;
    }

    #[tokio::test]
    async fn run_skills_command_dispatches_promote_to_global() {
        with_clean_home(|| async {
            let cwd = std::env::current_dir().expect("operation should succeed");
            let rig_dir = cwd.join(".opengoose/rigs/rig-1/skills/learned/my-skill");
            std::fs::create_dir_all(&rig_dir).expect("directory creation should succeed");
            std::fs::write(
                rig_dir.join("SKILL.md"),
                "---\nname: my-skill\ndescription: test\n---\n",
            )
            .expect("operation should succeed");

            run_skills_command(SkillsAction::Promote {
                name: "my-skill".to_string(),
                to: "global".to_string(),
                from_rig: Some("rig-1".to_string()),
                force: true,
            })
            .await
            .expect("operation should succeed");

            assert!(
                cwd.join(".opengoose/skills/learned/my-skill")
                    .join("SKILL.md")
                    .exists()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn run_skills_command_dispatches_list_archived() {
        with_clean_home(|| async {
            run_skills_command(SkillsAction::List {
                global: true,
                archived: true,
            })
            .await
            .expect("operation should succeed");
        })
        .await;
    }
}
