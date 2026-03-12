use std::path::{Component, Path, PathBuf};

impl super::YamlFileStore {
    /// Resolve and validate the file path for a definition name.
    pub(super) fn store_path_for(&self, name: &str) -> std::io::Result<PathBuf> {
        let dir = self.validated_dir()?;
        let sanitized = crate::sanitize_name(&name.to_lowercase().replace(' ', "-"));
        let path = dir.join(format!("{sanitized}.yaml"));
        self.validate_store_path(&path)?;
        Ok(path)
    }

    /// Validate and normalize the store directory, rejecting traversal components.
    pub(super) fn validated_dir(&self) -> std::io::Result<PathBuf> {
        if self.dir.as_os_str().is_empty() {
            return Err(invalid_store_path("store directory must not be empty"));
        }

        let mut validated = PathBuf::new();
        for component in self.dir.components() {
            match component {
                Component::Prefix(prefix) => validated.push(prefix.as_os_str()),
                Component::RootDir => validated.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
                Component::Normal(part) => validated.push(part),
                Component::CurDir | Component::ParentDir => {
                    return Err(invalid_store_path(
                        "store directory must not contain relative traversal components",
                    ));
                }
            }
        }

        Ok(validated)
    }

    /// Ensure a resolved path is a single file directly inside the store directory.
    pub(super) fn validate_store_path(&self, path: &Path) -> std::io::Result<()> {
        let dir = self.validated_dir()?;
        let relative = path.strip_prefix(&dir).map_err(|_| {
            invalid_store_path("store paths must stay within the configured store directory")
        })?;
        let mut components = relative.components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(_)), None) => Ok(()),
            _ => Err(invalid_store_path(
                "store paths must resolve to a single file inside the store directory",
            )),
        }
    }
}

pub(super) fn invalid_store_path(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}
