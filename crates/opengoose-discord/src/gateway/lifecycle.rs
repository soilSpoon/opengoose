//! Discord gateway lifecycle and event-loop handling.

use std::collections::HashSet;

use tracing::{error, info, warn};

use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_model::id::Id;
use twilight_model::id::marker::{ApplicationMarker, MessageMarker};

use tokio_util::sync::CancellationToken;

use opengoose_types::{AppEventKind, Platform};

use super::helpers::{handle_interaction, handle_message, register_slash_commands};
use super::{DiscordGateway, SEEN_MESSAGES_CAPACITY};

#[derive(Default)]
struct SeenMessageTracker {
    seen: HashSet<Id<MessageMarker>>,
    order: Vec<Id<MessageMarker>>,
}

impl SeenMessageTracker {
    fn record(&mut self, message_id: Id<MessageMarker>) -> bool {
        if !self.seen.insert(message_id) {
            return false;
        }

        self.order.push(message_id);
        if self.order.len() > SEEN_MESSAGES_CAPACITY {
            let evicted = self.order.remove(0);
            self.seen.remove(&evicted);
        }

        true
    }
}

impl DiscordGateway {
    pub(super) async fn run_gateway_loop(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        let intents = Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::DIRECT_MESSAGES;
        let mut shard = Shard::new(ShardId::ONE, self.token.clone(), intents);

        info!("discord gateway starting");

        let mut application_id: Option<Id<ApplicationMarker>> = None;
        let mut seen_messages = SeenMessageTracker::default();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    self.handle_shutdown();
                    break;
                }
                event = shard.next_event(EventTypeFlags::all()) => {
                    match event {
                        Some(Ok(event)) => {
                            self.handle_gateway_event(event, &mut application_id, &mut seen_messages)
                                .await;
                        }
                        Some(Err(error)) => self.handle_reconnect(error.to_string()),
                        None => {
                            self.handle_closed_shard();
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_gateway_event(
        &self,
        event: Event,
        application_id: &mut Option<Id<ApplicationMarker>>,
        seen_messages: &mut SeenMessageTracker,
    ) {
        match event {
            Event::MessageCreate(message) => {
                if !seen_messages.record(message.id) {
                    warn!(msg_id = %message.id, "duplicate MessageCreate ignored");
                    return;
                }

                handle_message(&self.bridge, self, &message.0).await;
            }
            Event::Ready(ready) => {
                let app_id = ready.application.id;
                *application_id = Some(app_id);
                self.handle_ready(app_id).await;
            }
            Event::InteractionCreate(interaction) => {
                if let Some(app_id) = *application_id {
                    handle_interaction(&self.http, app_id, &self.bridge, &interaction.0).await;
                }
            }
            _ => {}
        }
    }

    async fn handle_ready(&self, app_id: Id<ApplicationMarker>) {
        info!(?app_id, "discord gateway connected");
        self.event_bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Discord,
        });
        self.metrics.set_connected("discord");

        if let Err(error) = register_slash_commands(&self.http, app_id).await {
            error!(%error, "failed to register slash commands");
        }
    }

    fn handle_reconnect(&self, reason: String) {
        warn!(%reason, "discord gateway error, twilight will auto-reconnect");
        self.metrics.record_reconnect("discord", Some(reason));
        self.event_bus.emit(AppEventKind::ChannelReconnecting {
            platform: Platform::Discord,
            // Twilight manages attempt tracking internally; we report 0 to
            // indicate an auto-reconnect without a specific attempt count.
            attempt: 0,
            delay_secs: 0,
        });
    }

    fn handle_shutdown(&self) {
        info!("discord gateway shutting down");
        self.event_bus.emit(AppEventKind::ChannelDisconnected {
            platform: Platform::Discord,
            reason: "shutdown".into(),
        });
    }

    fn handle_closed_shard(&self) {
        error!("discord shard closed -- check bot token and privileged intents");
        let reason = "Discord connection closed. Verify your bot token and that MESSAGE_CONTENT intent is enabled in the Developer Portal.".to_string();
        self.event_bus.emit(AppEventKind::ChannelDisconnected {
            platform: Platform::Discord,
            reason: reason.clone(),
        });
        self.event_bus.emit(AppEventKind::Error {
            context: "discord".into(),
            message: reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use twilight_model::id::Id;
    use twilight_model::id::marker::MessageMarker;

    use super::SeenMessageTracker;
    use crate::gateway::SEEN_MESSAGES_CAPACITY;

    #[test]
    fn tracker_rejects_duplicate_ids() {
        let mut tracker = SeenMessageTracker::default();

        assert!(tracker.record(Id::<MessageMarker>::new(42)));
        assert!(!tracker.record(Id::<MessageMarker>::new(42)));
    }

    #[test]
    fn tracker_evicts_oldest_message_at_capacity() {
        let mut tracker = SeenMessageTracker::default();

        for id in 1..=(SEEN_MESSAGES_CAPACITY as u64 + 1) {
            assert!(tracker.record(Id::<MessageMarker>::new(id)));
        }

        assert_eq!(tracker.order.len(), SEEN_MESSAGES_CAPACITY);
        assert!(!tracker.seen.contains(&Id::<MessageMarker>::new(1)));
        assert!(
            tracker
                .seen
                .contains(&Id::<MessageMarker>::new(SEEN_MESSAGES_CAPACITY as u64 + 1))
        );
    }
}
