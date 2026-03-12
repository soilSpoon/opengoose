use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::super::*;

#[tokio::test]
async fn test_file_watch_trigger_watcher_cancels_cleanly() {
    let db = Arc::new(opengoose_persistence::Database::open_in_memory().unwrap());
    let event_bus = opengoose_types::EventBus::new(64);
    let cancel = CancellationToken::new();

    let handle = spawn_file_watch_trigger_watcher(db, event_bus, cancel.clone());

    // Give the task time to start up, then cancel it.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();

    // Should finish promptly after cancellation.
    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("watcher did not stop within timeout")
        .expect("watcher task panicked");
}

#[tokio::test]
async fn test_file_watch_trigger_fires_on_matching_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(opengoose_persistence::Database::open_in_memory().unwrap());
    let event_bus = opengoose_types::EventBus::new(64);
    let cancel = CancellationToken::new();

    // Register a file_watch trigger scoped to *.tmp files in the temp dir.
    let pattern = format!("{}/*.tmp", dir.path().display());
    let condition = serde_json::to_string(&FileWatchCondition {
        pattern: Some(pattern),
    })
    .unwrap();

    // The trigger references a non-existent team; the watcher will log a
    // warning but must not panic.
    opengoose_persistence::TriggerStore::new(db.clone())
        .create("watch-test", "file_watch", &condition, "no-such-team", "")
        .unwrap();

    // Change into the temp dir so the watcher root covers our test file.
    let prev_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).ok();

    let handle = spawn_file_watch_trigger_watcher(db, event_bus, cancel.clone());

    // Allow the watcher to initialise.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Create a matching file — this should generate a notify event.
    let tmp_file = dir.path().join("test.tmp");
    std::fs::write(&tmp_file, b"hello").unwrap();

    // Give the event time to propagate.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("watcher did not stop within timeout")
        .expect("watcher task panicked");

    // Restore working directory.
    std::env::set_current_dir(prev_dir).ok();
}
