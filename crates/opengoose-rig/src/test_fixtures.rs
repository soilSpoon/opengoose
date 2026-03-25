// Shared test fixtures for opengoose-rig.
//
// Consolidates the 20+ duplicated tempfile::tempdir() patterns
// across worktree.rs, middleware.rs, pipeline.rs, and conversation_log tests.

/// Create a temporary directory suitable for use as a fake home/base dir.
/// The caller must hold the returned TempDir for the duration of the test;
/// dropping it cleans up the directory.
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir creation should succeed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_dir_is_valid_directory() {
        let dir = temp_dir();
        assert!(dir.path().is_dir());
    }
}
