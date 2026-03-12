use std::future::Future;
use std::sync::Mutex;

use uuid::Uuid;

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub fn with_temp_home(test: impl FnOnce()) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let temp_home =
        std::env::temp_dir().join(format!("opengoose-integration-home-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_home).unwrap();

    let saved_home = std::env::var("HOME").ok();
    unsafe {
        std::env::set_var("HOME", &temp_home);
    }

    test();

    unsafe {
        match saved_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
    let _ = std::fs::remove_dir_all(&temp_home);
}

pub fn run_async_test(test: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(test);
}
