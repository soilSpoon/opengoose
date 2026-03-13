mod discovery;
mod manifest;
mod runtime;
mod status;
mod validation;

use std::path::Path;

use super::*;

fn write_manifest(dir: &Path, content: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("plugin.toml"), content).unwrap();
}
