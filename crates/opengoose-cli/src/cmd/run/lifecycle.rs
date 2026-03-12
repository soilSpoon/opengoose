use std::sync::Arc;
use std::time::Duration;

use crate::error::CliResult;
use opengoose_core::Engine;
use opengoose_core::alerts::AlertDispatcher;
use opengoose_persistence::{
    AlertStore, DEFAULT_EVENT_RETENTION_DAYS, Database, EventStore, spawn_event_history_recorder,
};
use opengoose_profiles::ProfileStore;
use opengoose_tui::ComposerRequest;
use opengoose_types::{AppEventKind, EventBus};
use tokio_util::sync::CancellationToken;

/// Resolve runtime retention settings from the default `main` profile.
#[derive(Debug, Clone, Copy)]
pub(super) struct RetentionPolicy {
    pub message_retention_days: Option<u32>,
    pub event_retention_days: u32,
}

pub(super) fn main_profile_retention_policy() -> CliResult<RetentionPolicy> {
    let store = ProfileStore::new()?;
    let settings = store.get("main").ok().and_then(|profile| profile.settings);

    Ok(RetentionPolicy {
        message_retention_days: settings.as_ref().and_then(|s| s.message_retention_days),
        event_retention_days: settings
            .and_then(|s| s.event_retention_days)
            .unwrap_or(DEFAULT_EVENT_RETENTION_DAYS),
    })
}

/// Spawn a periodic cleanup task for expired session messages.
pub(super) fn spawn_periodic_cleanup(
    engine: Arc<Engine>,
    cancel: CancellationToken,
    retention_policy: RetentionPolicy,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    if let Some(retention_days) = retention_policy.message_retention_days
                        && let Err(e) = engine.sessions().cleanup_expired_messages(retention_days)
                    {
                        tracing::warn!(%e, retention_days, "periodic message cleanup failed");
                    }

                    if let Err(e) = EventStore::new(engine.db().clone())
                        .cleanup_expired(retention_policy.event_retention_days)
                    {
                        tracing::warn!(
                            %e,
                            retention_days = retention_policy.event_retention_days,
                            "periodic event cleanup failed"
                        );
                    }
                }
            }
        }
    });
}

pub(super) fn spawn_configured_periodic_cleanup(
    engine: Arc<Engine>,
    cancel: CancellationToken,
    retention_policy: RetentionPolicy,
) {
    tracing::info!(
        message_retention_days = retention_policy.message_retention_days,
        event_retention_days = retention_policy.event_retention_days,
        "enabled periodic retention cleanup"
    );
    spawn_periodic_cleanup(engine, cancel, retention_policy);
}

pub(super) fn spawn_runtime_event_recorder(
    db: Arc<Database>,
    event_bus: EventBus,
) -> opengoose_persistence::EventHistoryRecorderHandle {
    spawn_event_history_recorder(db, event_bus)
}

pub(super) fn spawn_periodic_alert_dispatch(
    db: Arc<Database>,
    event_bus: EventBus,
    cancel: CancellationToken,
    interval: Duration,
) {
    let dispatcher = Arc::new(AlertDispatcher::new(
        Arc::new(AlertStore::new(db)),
        event_bus,
    ));
    dispatcher.start_periodic(interval, cancel);
}

pub(super) fn spawn_tui_composer_handler(
    engine: Arc<Engine>,
    event_bus: EventBus,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<ComposerRequest>,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                request = rx.recv() => {
                    let Some(request) = request else {
                        break;
                    };
                    if let Err(e) = engine
                        .process_message_streaming(
                            &request.session_key,
                            Some("operator"),
                            &request.content,
                        )
                        .await
                    {
                        event_bus.emit(AppEventKind::Error {
                            context: "tui_compose".into(),
                            message: e.to_string(),
                        });
                    }
                }
            }
        }
    });
}
