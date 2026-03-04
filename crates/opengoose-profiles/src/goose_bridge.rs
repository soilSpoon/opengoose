use std::path::Path;

use crate::error::ProfileResult;

/// Register the profiles directory in `GOOSE_RECIPE_PATH` so that Goose's
/// Summon extension can discover agent profiles as sub-recipes.
///
/// **Must be called before the tokio multi-thread runtime is started** because
/// `set_var` is not safe to call while other threads may read the environment.
/// Also must run before `AgentManager::instance().await` because Summon
/// caches discovery paths at initialization time.
///
/// # Safety
///
/// This function uses `unsafe { std::env::set_var }`. The caller must ensure
/// that no other threads are concurrently reading environment variables.
/// In practice, call this from `main()` before `#[tokio::main]` or from a
/// single-threaded context.
pub fn register_profiles_path(profiles_dir: &Path) -> ProfileResult<()> {
    let new_path = match std::env::var("GOOSE_RECIPE_PATH") {
        Ok(existing) if !existing.is_empty() => {
            format!("{}:{}", profiles_dir.display(), existing)
        }
        _ => profiles_dir.display().to_string(),
    };

    // Safety: caller must ensure this is called before spawning threads.
    // See function-level doc comment.
    unsafe {
        std::env::set_var("GOOSE_RECIPE_PATH", &new_path);
    }

    tracing::debug!(
        path = %new_path,
        "registered profiles directory in GOOSE_RECIPE_PATH"
    );
    Ok(())
}
