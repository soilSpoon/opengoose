use super::super::protocol::ProtocolMessage;
use super::super::transport::{ReplayResult, should_buffer_for_replay};
use super::RemoteAgentRegistry;

impl RemoteAgentRegistry {
    /// Send a protocol message to a specific remote agent.
    ///
    /// Returns `true` if the message was delivered immediately or buffered for replay.
    pub async fn send_to(&self, name: &str, msg: ProtocolMessage) -> bool {
        let bufferable = should_buffer_for_replay(&msg);
        let mut should_mark_reconnecting = false;

        let accepted = {
            let mut outbound = self.outbound.lock().await;
            let Some(transport) = outbound.get_mut(name) else {
                return false;
            };

            if bufferable {
                let event_id = transport.next_event_id;
                transport.next_event_id = transport.next_event_id.saturating_add(1);
                transport
                    .replay_buffer
                    .push_back(super::super::transport::ReplayEvent {
                        event_id,
                        message: msg.clone(),
                    });

                while transport.replay_buffer.len() > self.config.replay_buffer_capacity {
                    transport.replay_buffer.pop_front();
                }
            }

            match transport.tx.as_ref() {
                Some(tx) => {
                    if tx.send(msg).is_ok() {
                        true
                    } else {
                        transport.detach();
                        should_mark_reconnecting = true;
                        bufferable
                    }
                }
                None => bufferable,
            }
        };

        if should_mark_reconnecting {
            self.mark_reconnecting(name).await;
        }

        accepted
    }

    /// Re-enqueue buffered outbound events newer than `last_event_id`.
    pub async fn replay_since(&self, name: &str, last_event_id: u64) -> ReplayResult {
        let mut outbound = self.outbound.lock().await;
        let Some(transport) = outbound.get_mut(name) else {
            return ReplayResult::Unavailable;
        };

        let Some(tx) = transport.tx.as_ref() else {
            return ReplayResult::Unavailable;
        };

        let newest_event_id = transport.next_event_id.saturating_sub(1);
        if last_event_id > newest_event_id {
            return ReplayResult::BufferMiss;
        }
        if last_event_id == newest_event_id {
            return ReplayResult::Replayed(0);
        }

        let Some(oldest_event_id) = transport.replay_buffer.front().map(|event| event.event_id)
        else {
            return ReplayResult::BufferMiss;
        };
        if last_event_id.saturating_add(1) < oldest_event_id {
            return ReplayResult::BufferMiss;
        }

        let replayable: Vec<ProtocolMessage> = transport
            .replay_buffer
            .iter()
            .filter(|event| event.event_id > last_event_id)
            .map(|event| event.message.clone())
            .collect();

        let replayed_events = replayable.len() as u64;
        for message in replayable {
            if tx.send(message).is_err() {
                transport.detach();
                return ReplayResult::Unavailable;
            }
        }

        ReplayResult::Replayed(replayed_events)
    }
}
