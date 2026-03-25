// Initialization logic — home directory, database URL, environment lock for tests

use std::path::PathBuf;

/// Return the user's home directory, preferring $HOME (for test isolation)
/// and falling back to `dirs::home_dir()`.
pub(crate) fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        PathBuf::from(h)
    } else {
        dirs::home_dir().unwrap_or_else(|| ".".into())
    }
}

pub(crate) fn db_url() -> String {
    let home = home_dir();
    let dir = home.join(".opengoose");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(path = %dir.display(), "failed to create .opengoose dir: {e}");
    }
    format!("sqlite://{}?mode=rwc", dir.join("board.db").display())
}

/// Global mutex for tests that modify environment variables (HOME, XDG_STATE_HOME, cwd).
/// All such tests across every module must acquire this lock to avoid cross-contamination.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::test_env_lock;

    #[test]
    fn db_url_points_to_board_db() {
        let url = db_url();
        assert!(url.starts_with("sqlite://"));
        assert!(url.ends_with(".opengoose/board.db?mode=rwc"));
    }

    #[test]
    fn db_url_points_to_home_opengoose() {
        let url = db_url();
        assert!(url.starts_with("sqlite://"));
        assert!(url.ends_with("board.db?mode=rwc"));
    }

    #[test]
    fn home_dir_uses_home_env_var() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", "/tmp/test-home-dir");
        }
        let result = home_dir();
        assert_eq!(result, PathBuf::from("/tmp/test-home-dir"));
        unsafe {
            match prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }
}
