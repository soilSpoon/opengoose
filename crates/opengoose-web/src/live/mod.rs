use std::time::Duration;

mod changes;
mod snapshot;
mod watcher;

pub(crate) use watcher::spawn_live_event_watcher;

pub(crate) const LIVE_EVENT_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(test)]
mod tests;
