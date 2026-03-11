use std::sync::Mutex;

static HOME_ENV_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn with_temp_home(prefix: &str, test: impl FnOnce()) {
    let _guard = HOME_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_home);
    std::fs::create_dir_all(&temp_home).expect("temp home should be created");
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

    let _ = std::fs::remove_dir_all(temp_home);
}
