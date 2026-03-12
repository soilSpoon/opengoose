use std::sync::Arc;

use opengoose_persistence::{Database, TriggerStore};
use opengoose_types::{AppEventKind, EventBus};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use super::{
    matches_file_watch_event, matches_message_event, matches_on_message_event,
    matches_on_schedule_event, matches_on_session_event,
};
use crate::message_bus::{BusEvent, MessageBus};

/// Spawn the file-watch trigger watcher as a background task.
///
/// On startup, queries all enabled `file_watch` triggers from the DB and
/// sets up a recursive filesystem watcher rooted at the current working
/// directory. When a file-system event fires, each trigger whose glob
/// pattern matches the affected path is evaluated and, if it matches,
/// a headless team run is started via the [`EventBus`].
///
/// The watcher respects the supplied [`CancellationToken`]: once cancelled
/// the background task exits cleanly and the underlying OS watcher is
/// dropped.
pub fn spawn_file_watch_trigger_watcher(
    db: Arc<Database>,
    event_bus: EventBus,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("file-watch trigger watcher started");

        // Use an unbounded channel so the synchronous notify callback never
        // blocks the OS thread it runs on.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Result<notify::Event>>();

        let mut watcher = match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(w) => w,
            Err(e) => {
                error!(%e, "file-watch trigger watcher: failed to create watcher");
                return;
            }
        };

        let watch_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        if let Err(e) =
            notify::Watcher::watch(&mut watcher, &watch_root, notify::RecursiveMode::Recursive)
        {
            error!(
                %e,
                path = %watch_root.display(),
                "file-watch trigger watcher: failed to watch directory"
            );
            return;
        }

        info!(path = %watch_root.display(), "file-watch trigger watcher: watching");

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("file-watch trigger watcher stopped");
                    break;
                }
                Some(result) = rx.recv() => {
                    match result {
                        Ok(event) => {
                            for path in &event.paths {
                                let path_str = path.to_string_lossy();
                                if let Err(e) =
                                    fire_file_watch_triggers(&db, &event_bus, &path_str).await
                                {
                                    error!(%e, "file-watch trigger watcher: failed handling event");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(%e, "file-watch trigger watcher: watch error");
                        }
                    }
                }
            }
        }
    })
}

async fn fire_file_watch_triggers(
    db: &Arc<Database>,
    event_bus: &EventBus,
    path: &str,
) -> anyhow::Result<()> {
    fire_matching_triggers(db, event_bus, "file_watch", |cond| {
        matches_file_watch_event(cond, path)
    })
    .await
}

/// Spawn the trigger watcher as a background task.
///
/// Listens on the global [`MessageBus`] tap and evaluates enabled
/// `message_received` triggers against each incoming event.
pub fn spawn_trigger_watcher(
    db: Arc<Database>,
    event_bus: EventBus,
    message_bus: MessageBus,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("trigger watcher started");
        let mut rx = message_bus.subscribe_all();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("trigger watcher stopped");
                    break;
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Err(e) = handle_bus_event(&db, &event_bus, &event).await {
                                error!(%e, "trigger watcher: failed handling event");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(n, "trigger watcher: lagged behind, skipped messages");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            info!("trigger watcher: bus closed, exiting");
                            break;
                        }
                    }
                }
            }
        }
    })
}

/// Spawn the EventBus trigger watcher as a background task.
///
/// Subscribes to the [`EventBus`] and evaluates `on_message`,
/// `on_session_start`, `on_session_end`, and `on_schedule` triggers
/// against each system event.
pub fn spawn_event_bus_trigger_watcher(
    db: Arc<Database>,
    event_bus: EventBus,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("event-bus trigger watcher started");
        let mut rx = event_bus.subscribe();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("event-bus trigger watcher stopped");
                    break;
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Err(e) = handle_app_event(&db, &event_bus, &event.kind).await {
                                error!(%e, "event-bus trigger watcher: failed handling event");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(n, "event-bus trigger watcher: lagged, skipped events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            info!("event-bus trigger watcher: bus closed, exiting");
                            break;
                        }
                    }
                }
            }
        }
    })
}

async fn handle_app_event(
    db: &Arc<Database>,
    event_bus: &EventBus,
    kind: &AppEventKind,
) -> anyhow::Result<()> {
    match kind {
        AppEventKind::MessageReceived {
            author, content, ..
        } => {
            fire_matching_triggers(db, event_bus, "on_message", |cond| {
                matches_on_message_event(cond, author, content)
            })
            .await?;
        }
        AppEventKind::GooseReady => {
            fire_matching_triggers(db, event_bus, "on_session_start", |cond| {
                matches_on_session_event(cond, "system")
            })
            .await?;
        }
        AppEventKind::ChannelReady { platform } => {
            let platform = platform.to_string();
            fire_matching_triggers(db, event_bus, "on_session_start", |cond| {
                matches_on_session_event(cond, &platform)
            })
            .await?;
        }
        AppEventKind::SessionDisconnected { session_key, .. } => {
            let platform = session_key.platform.to_string();
            fire_matching_triggers(db, event_bus, "on_session_end", |cond| {
                matches_on_session_event(cond, &platform)
            })
            .await?;
        }
        AppEventKind::TeamRunCompleted { team } => {
            fire_matching_triggers(db, event_bus, "on_schedule", |cond| {
                matches_on_schedule_event(cond, team)
            })
            .await?;
        }
        _ => {}
    }

    Ok(())
}

async fn fire_matching_triggers<F>(
    db: &Arc<Database>,
    event_bus: &EventBus,
    trigger_type: &str,
    matches: F,
) -> anyhow::Result<()>
where
    F: Fn(&str) -> bool,
{
    let store = TriggerStore::new(db.clone());
    let triggers = store.list_by_type(trigger_type)?;

    for trigger in triggers {
        if matches(&trigger.condition_json) {
            info!(
                trigger = %trigger.name,
                team = %trigger.team_name,
                trigger_type,
                "trigger matched: firing team run"
            );

            let input = if trigger.input.is_empty() {
                format!("Triggered by {trigger_type} event")
            } else {
                trigger.input.clone()
            };

            match crate::run_headless(crate::HeadlessConfig::new(
                &trigger.team_name,
                &input,
                db.clone(),
                event_bus.clone(),
            ))
            .await
            {
                Ok((run_id, _)) => {
                    info!(trigger = %trigger.name, run_id, "triggered team run completed");
                }
                Err(e) => {
                    warn!(
                        trigger = %trigger.name,
                        team = %trigger.team_name,
                        %e,
                        "triggered team run failed"
                    );
                }
            }

            if let Err(e) = store.mark_fired(&trigger.name) {
                error!(trigger = %trigger.name, %e, "failed to mark trigger as fired");
            }
        }
    }

    Ok(())
}

async fn handle_bus_event(
    db: &Arc<Database>,
    event_bus: &EventBus,
    event: &BusEvent,
) -> anyhow::Result<()> {
    let store = TriggerStore::new(db.clone());
    let triggers = store.list_by_type("message_received")?;

    for trigger in triggers {
        let channel = event.channel.as_deref();
        if matches_message_event(
            &trigger.condition_json,
            &event.from,
            channel,
            &event.payload,
        ) {
            info!(
                trigger = %trigger.name,
                team = %trigger.team_name,
                event_from = %event.from,
                "trigger matched: firing team run"
            );

            let input = if trigger.input.is_empty() {
                format!(
                    "Triggered by event from '{}': {}",
                    event.from,
                    truncate(&event.payload, 200)
                )
            } else {
                trigger.input.clone()
            };

            match crate::run_headless(crate::HeadlessConfig::new(
                &trigger.team_name,
                &input,
                db.clone(),
                event_bus.clone(),
            ))
            .await
            {
                Ok((run_id, _)) => {
                    info!(
                        trigger = %trigger.name,
                        run_id = %run_id,
                        "triggered team run completed"
                    );
                }
                Err(e) => {
                    warn!(
                        trigger = %trigger.name,
                        team = %trigger.team_name,
                        %e,
                        "triggered team run failed"
                    );
                }
            }

            if let Err(e) = store.mark_fired(&trigger.name) {
                error!(trigger = %trigger.name, %e, "failed to mark trigger as fired");
            }
        }
    }

    Ok(())
}

pub(super) fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..s.floor_char_boundary(max)]
    }
}
