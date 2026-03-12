use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use opengoose_persistence::Database;
use serde_json::Value;
use tempfile::TempDir;

pub(crate) struct CliHarness {
    _temp: TempDir,
    home: PathBuf,
    goose_root: PathBuf,
}

impl CliHarness {
    pub(crate) fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let goose_root = temp.path().join("goose");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&goose_root).unwrap();

        Self {
            _temp: temp,
            home,
            goose_root,
        }
    }

    pub(crate) fn home(&self) -> &Path {
        &self.home
    }

    pub(crate) fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_opengoose"))
            .args(args)
            .env("HOME", &self.home)
            .env("GOOSE_PATH_ROOT", &self.goose_root)
            .env("GOOSE_DISABLE_KEYRING", "1")
            .output()
            .unwrap()
    }
}

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

pub(crate) fn stdout_json(output: &Output) -> Value {
    serde_json::from_str(&stdout(output)).unwrap()
}

pub(crate) fn stderr_json(output: &Output) -> Value {
    serde_json::from_str(&stderr(output)).unwrap()
}

pub(crate) fn open_database(home: &Path) -> Arc<Database> {
    let db_path = home.join(".opengoose").join("sessions.db");
    Arc::new(Database::open_at(db_path).unwrap())
}

pub(crate) fn assert_runtime_error_message(output: &Output, kind: &str, expected_message: &str) {
    assert!(!output.status.success());
    let error = &stderr_json(output)["error"];
    assert_eq!(error["kind"], Value::from(kind));
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| message.contains(expected_message)),
        "unexpected error payload: {error}"
    );
}
