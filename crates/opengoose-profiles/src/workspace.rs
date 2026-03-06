use std::path::{Path, PathBuf};

/// Workspace context files loaded into the system prompt, in priority order.
const CONTEXT_FILES: &[&str] = &[
    "BOOTSTRAP.md",
    "IDENTITY.md",
    "USER.md",
    "SOUL.md",
    "MEMORY.md",
];

const BOOTSTRAP_TEMPLATE: &str = include_str!("../workspace-templates/BOOTSTRAP.md");

/// Pre-authored identity and soul files for specialist profiles.
///
/// Specialist profiles (developer, researcher, etc.) have fixed identities
/// that are seeded automatically on first run — no onboarding conversation needed.
struct BundledTemplates {
    identity: &'static str,
    soul: &'static str,
}

fn bundled_templates(profile_name: &str) -> Option<BundledTemplates> {
    match profile_name {
        "developer" => Some(BundledTemplates {
            identity: include_str!("../workspace-templates/developer/IDENTITY.md"),
            soul: include_str!("../workspace-templates/developer/SOUL.md"),
        }),
        "researcher" => Some(BundledTemplates {
            identity: include_str!("../workspace-templates/researcher/IDENTITY.md"),
            soul: include_str!("../workspace-templates/researcher/SOUL.md"),
        }),
        "reviewer" => Some(BundledTemplates {
            identity: include_str!("../workspace-templates/reviewer/IDENTITY.md"),
            soul: include_str!("../workspace-templates/reviewer/SOUL.md"),
        }),
        "writer" => Some(BundledTemplates {
            identity: include_str!("../workspace-templates/writer/IDENTITY.md"),
            soul: include_str!("../workspace-templates/writer/SOUL.md"),
        }),
        _ => None,
    }
}

/// Return the workspace directory for a named profile.
///
/// Resolves to `~/.opengoose/workspace-{profile_name}/`.
/// Returns `None` if the home directory cannot be determined.
pub fn workspace_dir_for(profile_name: &str) -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join(".opengoose")
            .join(format!("workspace-{profile_name}"))
    })
}

/// Ensure the workspace directory exists and seed identity files on first run.
///
/// **Specialist profiles** (developer, researcher, reviewer, writer) have
/// bundled IDENTITY.md and SOUL.md that are written immediately — no
/// onboarding conversation required.
///
/// **Other profiles** (e.g. "main") receive BOOTSTRAP.md instead, which
/// triggers an onboarding conversation so the agent can build its own identity.
///
/// Nothing is written if IDENTITY.md already exists (workspace is already set up).
pub fn setup_workspace(profile_name: &str, workspace_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(workspace_dir)?;

    let identity = workspace_dir.join("IDENTITY.md");

    // Already initialised — leave everything as-is.
    if identity.exists() {
        return Ok(());
    }

    if let Some(templates) = bundled_templates(profile_name) {
        // Specialist profile: write pre-authored identity immediately.
        std::fs::write(&identity, templates.identity)?;
        std::fs::write(workspace_dir.join("SOUL.md"), templates.soul)?;
    } else {
        // No bundled templates — seed BOOTSTRAP.md to trigger onboarding.
        let bootstrap = workspace_dir.join("BOOTSTRAP.md");
        if !bootstrap.exists() {
            std::fs::write(&bootstrap, BOOTSTRAP_TEMPLATE)?;
        }
    }

    Ok(())
}

/// Load workspace context files and return a combined system prompt extension.
///
/// Reads each file in [`CONTEXT_FILES`] order and formats them as markdown
/// sections. Returns an empty string when no files exist or all are empty.
pub fn load_workspace_context(workspace_dir: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();

    for filename in CONTEXT_FILES {
        let path = workspace_dir.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let content = content.trim();
            if !content.is_empty() {
                parts.push(format!("## {filename}\n\n{content}"));
            }
        }
    }

    parts.join("\n\n---\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_workspace() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        (tmp, dir)
    }

    #[test]
    fn workspace_dir_for_includes_profile_name() {
        if let Some(dir) = workspace_dir_for("developer") {
            let s = dir.to_string_lossy();
            assert!(s.contains("workspace-developer"));
            assert!(s.contains(".opengoose"));
        }
    }

    // ── Specialist profiles (bundled templates) ──────────────────────────────

    #[test]
    fn setup_workspace_seeds_bundled_identity_for_developer() {
        let (_tmp, dir) = temp_workspace();
        setup_workspace("developer", &dir).unwrap();

        assert!(
            dir.join("IDENTITY.md").exists(),
            "IDENTITY.md must be seeded"
        );
        assert!(dir.join("SOUL.md").exists(), "SOUL.md must be seeded");
        assert!(
            !dir.join("BOOTSTRAP.md").exists(),
            "no BOOTSTRAP.md for specialist"
        );

        let identity = std::fs::read_to_string(dir.join("IDENTITY.md")).unwrap();
        assert!(identity.contains("Developer"));
    }

    #[test]
    fn setup_workspace_seeds_bundled_identity_for_researcher() {
        let (_tmp, dir) = temp_workspace();
        setup_workspace("researcher", &dir).unwrap();
        assert!(dir.join("IDENTITY.md").exists());
        assert!(dir.join("SOUL.md").exists());
        assert!(!dir.join("BOOTSTRAP.md").exists());
    }

    #[test]
    fn setup_workspace_seeds_bundled_identity_for_reviewer() {
        let (_tmp, dir) = temp_workspace();
        setup_workspace("reviewer", &dir).unwrap();
        assert!(dir.join("IDENTITY.md").exists());
        assert!(dir.join("SOUL.md").exists());
        assert!(!dir.join("BOOTSTRAP.md").exists());
    }

    #[test]
    fn setup_workspace_seeds_bundled_identity_for_writer() {
        let (_tmp, dir) = temp_workspace();
        setup_workspace("writer", &dir).unwrap();
        assert!(dir.join("IDENTITY.md").exists());
        assert!(dir.join("SOUL.md").exists());
        assert!(!dir.join("BOOTSTRAP.md").exists());
    }

    // ── Personal profile (bootstrap) ─────────────────────────────────────────

    #[test]
    fn setup_workspace_seeds_bootstrap_for_main() {
        let (_tmp, dir) = temp_workspace();
        setup_workspace("main", &dir).unwrap();

        assert!(
            dir.join("BOOTSTRAP.md").exists(),
            "BOOTSTRAP.md must be seeded for main"
        );
        assert!(
            !dir.join("IDENTITY.md").exists(),
            "no IDENTITY.md until onboarding completes"
        );

        let content = std::fs::read_to_string(dir.join("BOOTSTRAP.md")).unwrap();
        assert!(content.contains("First Run"));
    }

    #[test]
    fn setup_workspace_seeds_bootstrap_for_unknown_profile() {
        let (_tmp, dir) = temp_workspace();
        setup_workspace("custom-bot", &dir).unwrap();
        assert!(dir.join("BOOTSTRAP.md").exists());
        assert!(!dir.join("IDENTITY.md").exists());
    }

    // ── Idempotency ───────────────────────────────────────────────────────────

    #[test]
    fn setup_workspace_is_idempotent_when_identity_exists() {
        let (_tmp, dir) = temp_workspace();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("IDENTITY.md"), "# My custom identity").unwrap();

        // Call twice — neither specialist nor bootstrap logic should overwrite.
        setup_workspace("developer", &dir).unwrap();
        setup_workspace("developer", &dir).unwrap();

        let identity = std::fs::read_to_string(dir.join("IDENTITY.md")).unwrap();
        assert_eq!(
            identity, "# My custom identity",
            "existing IDENTITY.md must not be overwritten"
        );
        assert!(
            !dir.join("SOUL.md").exists(),
            "SOUL.md must not appear after identity exists"
        );
    }

    #[test]
    fn setup_workspace_does_not_overwrite_existing_bootstrap() {
        let (_tmp, dir) = temp_workspace();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("BOOTSTRAP.md"), "custom bootstrap").unwrap();

        setup_workspace("main", &dir).unwrap();

        let content = std::fs::read_to_string(dir.join("BOOTSTRAP.md")).unwrap();
        assert_eq!(content, "custom bootstrap");
    }

    // ── Context loading ───────────────────────────────────────────────────────

    #[test]
    fn load_workspace_context_returns_empty_for_missing_files() {
        let (_tmp, dir) = temp_workspace();
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = load_workspace_context(&dir);
        assert!(ctx.is_empty());
    }

    #[test]
    fn load_workspace_context_formats_files_as_sections() {
        let (_tmp, dir) = temp_workspace();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("IDENTITY.md"), "**Name**: Aria").unwrap();
        std::fs::write(dir.join("USER.md"), "**Name**: Alex").unwrap();

        let ctx = load_workspace_context(&dir);
        assert!(ctx.contains("## IDENTITY.md"));
        assert!(ctx.contains("## USER.md"));
        assert!(ctx.contains("**Name**: Aria"));
        assert!(ctx.contains("**Name**: Alex"));
    }

    #[test]
    fn load_workspace_context_skips_empty_files() {
        let (_tmp, dir) = temp_workspace();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SOUL.md"), "   \n  ").unwrap();
        std::fs::write(dir.join("MEMORY.md"), "some memory").unwrap();

        let ctx = load_workspace_context(&dir);
        assert!(!ctx.contains("## SOUL.md"), "empty SOUL.md must be skipped");
        assert!(ctx.contains("## MEMORY.md"));
    }

    #[test]
    fn load_workspace_context_joins_with_separator() {
        let (_tmp, dir) = temp_workspace();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("IDENTITY.md"), "identity content").unwrap();
        std::fs::write(dir.join("USER.md"), "user content").unwrap();

        let ctx = load_workspace_context(&dir);
        assert!(ctx.contains("---"), "sections must be separated by ---");
    }
}
