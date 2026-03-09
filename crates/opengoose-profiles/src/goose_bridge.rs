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
    let separator = if cfg!(windows) { ";" } else { ":" };
    let new_path = match std::env::var("GOOSE_RECIPE_PATH") {
        Ok(existing) if !existing.is_empty() => {
            format!("{}{separator}{}", profiles_dir.display(), existing)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// Mutex to serialize env-var tests so they don't race each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Helper to safely run env-var tests by saving/restoring the env.
    fn with_env_restored(f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var("GOOSE_RECIPE_PATH").ok();
        // Safety: protected by ENV_LOCK so no other test touches this var concurrently.
        unsafe {
            std::env::remove_var("GOOSE_RECIPE_PATH");
        }
        f();
        // Restore
        unsafe {
            match saved {
                Some(v) => std::env::set_var("GOOSE_RECIPE_PATH", v),
                None => std::env::remove_var("GOOSE_RECIPE_PATH"),
            }
        }
    }

    #[test]
    fn test_register_sets_env_var() {
        with_env_restored(|| {
            let dir = PathBuf::from("/my/profiles");
            register_profiles_path(&dir).unwrap();
            assert_eq!(std::env::var("GOOSE_RECIPE_PATH").unwrap(), "/my/profiles");
        });
    }

    #[test]
    fn test_register_prepends_to_existing() {
        with_env_restored(|| {
            unsafe {
                std::env::set_var("GOOSE_RECIPE_PATH", "/existing/path");
            }
            let dir = PathBuf::from("/new/profiles");
            register_profiles_path(&dir).unwrap();
            let val = std::env::var("GOOSE_RECIPE_PATH").unwrap();
            assert!(val.starts_with("/new/profiles"));
            assert!(val.contains("/existing/path"));
        });
    }

    #[test]
    fn test_register_treats_empty_existing_as_absent() {
        // When GOOSE_RECIPE_PATH is set to "" it should be treated the same as
        // absent — the guard `!existing.is_empty()` ensures we don't prepend to
        // an empty string and produce "/new/profiles:" or similar.
        with_env_restored(|| {
            unsafe {
                std::env::set_var("GOOSE_RECIPE_PATH", "");
            }
            let dir = PathBuf::from("/profiles/dir");
            register_profiles_path(&dir).unwrap();
            assert_eq!(std::env::var("GOOSE_RECIPE_PATH").unwrap(), "/profiles/dir");
        });
    }
}
