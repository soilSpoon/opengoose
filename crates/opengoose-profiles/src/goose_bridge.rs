use std::path::Path;

use crate::error::ProfileResult;

/// Register the profiles directory in `GOOSE_RECIPE_PATH` so that Goose's
/// Summon extension can discover agent profiles as sub-recipes.
///
/// **Must be called before `AgentManager::instance().await`** because Summon
/// caches discovery paths at initialization time.
pub fn register_profiles_path(profiles_dir: &Path) -> ProfileResult<()> {
    let new_path = match std::env::var("GOOSE_RECIPE_PATH") {
        Ok(existing) if !existing.is_empty() => {
            format!("{}:{}", profiles_dir.display(), existing)
        }
        _ => profiles_dir.display().to_string(),
    };

    // Safety: called before the tokio multi-thread runtime spawns worker
    // threads, so no concurrent reads of the environment.
    unsafe {
        std::env::set_var("GOOSE_RECIPE_PATH", &new_path);
    }

    tracing::debug!(
        path = %new_path,
        "registered profiles directory in GOOSE_RECIPE_PATH"
    );
    Ok(())
}
