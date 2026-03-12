use std::path::{Path, PathBuf};

use crate::error::CliResult;
use tokio::sync::Mutex;

use super::init::SAMPLE_PROJECT_FILE;
use super::*;
use crate::cmd::output::OutputMode;

static PROJECT_INIT_LOCK: Mutex<()> = Mutex::const_new(());

struct CurrentDirGuard {
    original: PathBuf,
}

impl CurrentDirGuard {
    fn change_to(path: &Path) -> Self {
        let original = std::env::current_dir().expect("current dir should resolve");
        std::env::set_current_dir(path).expect("current dir should change");
        Self { original }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original);
    }
}

async fn test_execute(action: ProjectAction, output: CliOutput) -> CliResult<()> {
    let tmp = tempfile::tempdir().unwrap();
    let store = ProjectStore::with_dir(tmp.path().to_path_buf());
    execute_with_store(action, store, output).await
}

fn text_output() -> CliOutput {
    CliOutput::new(OutputMode::Text)
}

fn json_output() -> CliOutput {
    CliOutput::new(OutputMode::Json)
}

#[tokio::test]
async fn list_succeeds() {
    test_execute(ProjectAction::List, text_output())
        .await
        .unwrap();
}

#[tokio::test]
async fn list_json_mode_succeeds() {
    test_execute(ProjectAction::List, json_output())
        .await
        .unwrap();
}

#[tokio::test]
async fn add_reports_file_not_found() {
    let err = test_execute(
        ProjectAction::Add {
            path: PathBuf::from("/nonexistent/path/project.yaml"),
            force: false,
        },
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("file not found") || msg.contains("not found"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn show_reports_unknown_project() {
    let err = test_execute(
        ProjectAction::Show {
            name: "definitely-nonexistent-project-xyz".into(),
        },
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn remove_reports_unknown_project() {
    let err = test_execute(
        ProjectAction::Remove {
            name: "definitely-nonexistent-project-xyz".into(),
        },
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn show_json_mode_reports_unknown_project() {
    let err = test_execute(
        ProjectAction::Show {
            name: "definitely-nonexistent-project-xyz".into(),
        },
        json_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn remove_json_mode_reports_unknown_project() {
    let err = test_execute(
        ProjectAction::Remove {
            name: "definitely-nonexistent-project-xyz".into(),
        },
        json_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn run_reports_unknown_project() {
    let err = test_execute(
        ProjectAction::Run {
            project: "definitely-nonexistent-project-xyz".into(),
            input: "hello".into(),
            team: None,
        },
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn init_creates_sample_file() {
    let _lock = PROJECT_INIT_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let _cwd = CurrentDirGuard::change_to(tmp.path());

    let result = execute(ProjectAction::Init { force: false }, text_output()).await;

    result.unwrap();
    assert!(tmp.path().join(SAMPLE_PROJECT_FILE).exists());
}

#[tokio::test]
async fn init_force_overwrites() {
    let _lock = PROJECT_INIT_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let _cwd = CurrentDirGuard::change_to(tmp.path());

    execute(ProjectAction::Init { force: false }, text_output())
        .await
        .unwrap();
    let result = execute(ProjectAction::Init { force: true }, text_output()).await;

    result.unwrap();
}

#[test]
fn project_store_new_succeeds() {
    let store = ProjectStore::new();
    assert!(store.is_ok());
}

#[test]
fn project_store_list_returns_vec() {
    let store = ProjectStore::new().unwrap();
    let names = store.list();
    assert!(names.is_ok());
}
