#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub struct IsolatedEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
    prev_home: Option<String>,
    prev_xdg: Option<String>,
}

#[cfg(test)]
impl IsolatedEnv {
    pub fn new(tmp: &std::path::Path) -> Self {
        let guard = ENV_LOCK.lock().expect("lock operation should succeed");
        let prev_home = std::env::var("HOME").ok();
        let prev_xdg = std::env::var("XDG_STATE_HOME").ok();
        unsafe {
            std::env::set_var("HOME", tmp);
            std::env::set_var("XDG_STATE_HOME", tmp.join("xdg"));
        }
        Self {
            _guard: guard,
            prev_home,
            prev_xdg,
        }
    }
}

#[cfg(test)]
impl Drop for IsolatedEnv {
    fn drop(&mut self) {
        match &self.prev_home {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        match &self.prev_xdg {
            Some(v) => unsafe { std::env::set_var("XDG_STATE_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_STATE_HOME") },
        }
    }
}
