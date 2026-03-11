use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownState {
    Running = 0,
    Draining = 1,
    Stopped = 2,
}

impl ShutdownState {
    fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::Running,
            1 => Self::Draining,
            _ => Self::Stopped,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownSnapshot {
    pub state: ShutdownState,
    pub active_streams: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownDrainResult {
    pub timed_out: bool,
    pub remaining_streams: usize,
}

#[derive(Debug, Clone)]
pub struct ShutdownController {
    inner: Arc<ShutdownInner>,
}

#[derive(Debug)]
struct ShutdownInner {
    state: AtomicU8,
    active_streams: AtomicUsize,
    notify: Notify,
}

impl ShutdownController {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ShutdownInner {
                state: AtomicU8::new(ShutdownState::Running as u8),
                active_streams: AtomicUsize::new(0),
                notify: Notify::new(),
            }),
        }
    }

    pub fn snapshot(&self) -> ShutdownSnapshot {
        ShutdownSnapshot {
            state: self.state(),
            active_streams: self.active_streams(),
        }
    }

    pub fn state(&self) -> ShutdownState {
        ShutdownState::from_raw(self.inner.state.load(Ordering::SeqCst))
    }

    pub fn is_accepting_messages(&self) -> bool {
        self.state() == ShutdownState::Running
    }

    pub fn active_streams(&self) -> usize {
        self.inner.active_streams.load(Ordering::SeqCst)
    }

    pub fn begin_shutdown(&self) -> ShutdownSnapshot {
        self.inner
            .state
            .compare_exchange(
                ShutdownState::Running as u8,
                ShutdownState::Draining as u8,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .ok();
        self.snapshot()
    }

    pub fn mark_stopped(&self) -> ShutdownSnapshot {
        self.inner
            .state
            .store(ShutdownState::Stopped as u8, Ordering::SeqCst);
        self.snapshot()
    }

    pub fn try_acquire_stream(&self) -> Option<ShutdownStreamGuard> {
        loop {
            if !self.is_accepting_messages() {
                return None;
            }

            self.inner.active_streams.fetch_add(1, Ordering::SeqCst);

            if self.is_accepting_messages() {
                return Some(ShutdownStreamGuard {
                    controller: self.clone(),
                    released: false,
                });
            }

            self.inner.active_streams.fetch_sub(1, Ordering::SeqCst);
            self.inner.notify.notify_waiters();
        }
    }

    pub async fn wait_for_streams(&self, timeout: Duration) -> ShutdownDrainResult {
        let wait = async {
            while self.active_streams() > 0 {
                self.inner.notify.notified().await;
            }
        };

        match tokio::time::timeout(timeout, wait).await {
            Ok(()) => ShutdownDrainResult {
                timed_out: false,
                remaining_streams: 0,
            },
            Err(_) => ShutdownDrainResult {
                timed_out: true,
                remaining_streams: self.active_streams(),
            },
        }
    }

    fn release_stream(&self) {
        let previous = self.inner.active_streams.fetch_sub(1, Ordering::SeqCst);
        if previous <= 1 {
            self.inner.notify.notify_waiters();
        }
    }
}

impl Default for ShutdownController {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct ShutdownStreamGuard {
    controller: ShutdownController,
    released: bool,
}

impl Drop for ShutdownStreamGuard {
    fn drop(&mut self) {
        if !self.released {
            self.released = true;
            self.controller.release_stream();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_rejects_new_streams_after_shutdown_begins() {
        let controller = ShutdownController::new();
        let _guard = controller
            .try_acquire_stream()
            .expect("stream should start");

        let snapshot = controller.begin_shutdown();
        assert_eq!(snapshot.state, ShutdownState::Draining);
        assert_eq!(snapshot.active_streams, 1);
        assert!(controller.try_acquire_stream().is_none());
    }

    #[tokio::test]
    async fn wait_for_streams_times_out_when_guard_is_still_held() {
        let controller = ShutdownController::new();
        let _guard = controller
            .try_acquire_stream()
            .expect("stream should start");
        controller.begin_shutdown();

        let result = controller.wait_for_streams(Duration::from_millis(10)).await;
        assert!(result.timed_out);
        assert_eq!(result.remaining_streams, 1);
    }
}
