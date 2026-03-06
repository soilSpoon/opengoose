use std::time::Instant;

use opengoose_types::{AppEvent, AppEventKind};

use super::state::*;

impl App {
    pub fn push_event(&mut self, summary: &str, level: EventLevel) {
        self.events.push_back(EventEntry {
            summary: summary.to_string(),
            level,
            timestamp: Instant::now(),
        });
        if self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    pub fn handle_app_event(&mut self, event: AppEvent) {
        match &event.kind {
            AppEventKind::GooseReady => {
                // Goose agent system is ready (no platform connection change).
            }
            AppEventKind::ChannelReady { platform } => {
                self.connected_platforms.insert(platform.clone());
            }
            AppEventKind::ChannelDisconnected { platform, .. } => {
                self.connected_platforms.remove(platform);
            }
            AppEventKind::MessageReceived {
                session_key,
                author,
                content,
            } => {
                self.messages.push_back(MessageEntry {
                    session_key: session_key.clone(),
                    author: author.clone(),
                    content: content.clone(),
                });
                if self.messages.len() > MAX_MESSAGES {
                    self.messages.pop_front();
                }
                self.messages_scroll = 0;
            }
            AppEventKind::ResponseSent {
                session_key,
                content,
            } => {
                self.messages.push_back(MessageEntry {
                    session_key: session_key.clone(),
                    author: "goose".into(),
                    content: content.clone(),
                });
                if self.messages.len() > MAX_MESSAGES {
                    self.messages.pop_front();
                }
                self.messages_scroll = 0;
            }
            AppEventKind::PairingCodeGenerated { code } => {
                self.pairing_code = Some(code.clone());
            }
            AppEventKind::PairingCompleted { session_key } => {
                self.active_sessions.insert(session_key.clone());
            }
            AppEventKind::SessionDisconnected { session_key, .. } => {
                self.active_sessions.remove(session_key);
            }
            AppEventKind::TeamActivated {
                session_key,
                team_name,
            } => {
                self.active_teams
                    .insert(session_key.clone(), team_name.clone());
            }
            AppEventKind::TeamDeactivated { session_key } => {
                self.active_teams.remove(session_key);
            }
            AppEventKind::Error { .. } => {}
            AppEventKind::TracingEvent { .. } => {}
            // Team and workflow events are displayed via the events panel (below)
            AppEventKind::TeamRunStarted { .. }
            | AppEventKind::TeamStepStarted { .. }
            | AppEventKind::TeamStepCompleted { .. }
            | AppEventKind::TeamStepFailed { .. }
            | AppEventKind::TeamRunCompleted { .. }
            | AppEventKind::TeamRunFailed { .. } => {}
            // Streaming events are informational — handled by the gateway layer
            AppEventKind::StreamStarted { .. }
            | AppEventKind::StreamUpdated { .. }
            | AppEventKind::StreamCompleted { .. } => {}
        }

        // All events go to the events panel — except message events which
        // are already shown in the messages panel.
        let shown_in_messages = matches!(
            &event.kind,
            AppEventKind::MessageReceived { .. } | AppEventKind::ResponseSent { .. }
        );
        if !shown_in_messages {
            let level = match &event.kind {
                AppEventKind::Error { .. } => EventLevel::Error,
                _ => EventLevel::Info,
            };
            self.events.push_back(EventEntry {
                summary: event.kind.to_string(),
                level,
                timestamp: Instant::now(),
            });
            if self.events.len() > MAX_EVENTS {
                self.events.pop_front();
            }
        }
    }

    pub fn tick(&mut self) {
        // Poll async provider loading
        if let Some(ref mut rx) = self.provider_loading_rx {
            match rx.try_recv() {
                Ok(providers) => {
                    self.cached_providers = providers;
                    self.provider_loading_rx = None;
                    self.populate_provider_select_from_cache();
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.provider_loading_rx = None;
                    self.push_event("Failed to load providers.", EventLevel::Error);
                    self.provider_select.visible = false;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            }
        }

        // Poll async model loading
        if let Some(ref mut rx) = self.model_loading_rx {
            match rx.try_recv() {
                Ok(models) => {
                    self.model_select.models = models;
                    self.model_select.loading = false;
                    self.model_loading_rx = None;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.model_loading_rx = None;
                    self.model_select.loading = false;
                    self.push_event("Failed to fetch models.", EventLevel::Error);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            }
        }

        // Poll async OAuth completion
        if let Some(ref mut rx) = self.oauth_done_rx {
            let result = match rx.try_recv() {
                Ok(r) => Some(r),
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    Some(Err(anyhow::anyhow!("OAuth task terminated unexpectedly")))
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.oauth_done_rx = None;
                match result {
                    Ok(()) => {
                        self.push_event(
                            &format!(
                                "OAuth completed for {}.",
                                self.credential_flow
                                    .provider_display
                                    .as_deref()
                                    .unwrap_or("")
                            ),
                            EventLevel::Info,
                        );
                        // Advance past the OAuth key
                        if self.credential_flow.has_more() {
                            self.credential_flow.current_key += 1;
                            self.advance_credential_flow();
                        } else if let Err(e) = self.store_credentials() {
                            self.push_event(
                                &format!("Failed to store credentials: {e}"),
                                EventLevel::Error,
                            );
                            self.credential_flow.reset();
                        }
                    }
                    Err(e) => {
                        self.push_event(&format!("OAuth failed: {e}"), EventLevel::Error);
                        self.credential_flow.reset();
                    }
                }
            }
        }
    }
}
