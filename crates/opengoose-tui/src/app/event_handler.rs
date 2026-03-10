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
        let (summary, level, notice) = summarize_event(&event.kind);

        match &event.kind {
            AppEventKind::GooseReady => {}
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
                self.cache_message(MessageEntry {
                    session_key: session_key.clone(),
                    author: author.clone(),
                    content: content.clone(),
                });
                self.refresh_sessions();
            }
            AppEventKind::ResponseSent {
                session_key,
                content,
            } => {
                self.cache_message(MessageEntry {
                    session_key: session_key.clone(),
                    author: "goose".into(),
                    content: content.clone(),
                });
                self.refresh_sessions();
            }
            AppEventKind::PairingCodeGenerated { code } => {
                self.pairing_code = Some(code.clone());
            }
            AppEventKind::PairingCompleted { session_key } => {
                self.active_sessions.insert(session_key.clone());
                self.refresh_sessions();
            }
            AppEventKind::SessionDisconnected { session_key, .. } => {
                self.active_sessions.remove(session_key);
                self.refresh_sessions();
            }
            AppEventKind::TeamActivated {
                session_key,
                team_name,
            } => {
                self.active_teams
                    .insert(session_key.clone(), team_name.clone());
                self.refresh_sessions();
            }
            AppEventKind::TeamDeactivated { session_key } => {
                self.active_teams.remove(session_key);
                self.refresh_sessions();
            }
            AppEventKind::Error { .. } => {
                self.set_agent_status(AgentStatus::Idle, None);
            }
            AppEventKind::TracingEvent { .. } => {}
            AppEventKind::StreamStarted { session_key, .. } => {
                self.set_agent_status(AgentStatus::Thinking, Some(session_key.clone()));
            }
            AppEventKind::StreamUpdated { session_key, .. } => {
                self.set_agent_status(AgentStatus::Generating, Some(session_key.clone()));
            }
            AppEventKind::StreamCompleted { session_key, .. } => {
                self.set_agent_status(AgentStatus::Idle, Some(session_key.clone()));
            }
            AppEventKind::TeamRunStarted { .. }
            | AppEventKind::TeamStepStarted { .. }
            | AppEventKind::TeamStepCompleted { .. }
            | AppEventKind::TeamStepFailed { .. }
            | AppEventKind::TeamRunCompleted { .. }
            | AppEventKind::TeamRunFailed { .. }
            | AppEventKind::ChannelReconnecting { .. } => {}
        }

        if let Some(notice) = notice {
            self.set_status_notice(notice, level);
        }

        let shown_in_messages = matches!(
            &event.kind,
            AppEventKind::MessageReceived { .. } | AppEventKind::ResponseSent { .. }
        );
        if !shown_in_messages {
            self.events.push_back(EventEntry {
                summary,
                level,
                timestamp: Instant::now(),
            });
            if self.events.len() > MAX_EVENTS {
                self.events.pop_front();
            }
        }
    }

    pub fn tick(&mut self) {
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
                    self.set_status_notice(
                        "Provider list could not be loaded. Check your connection and retry."
                            .to_string(),
                        EventLevel::Error,
                    );
                    self.provider_select.visible = false;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            }
        }

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
                    self.set_status_notice(
                        "Model lookup failed. The provider may be unavailable right now."
                            .to_string(),
                        EventLevel::Error,
                    );
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            }
        }

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
                        if self.credential_flow.has_more() {
                            self.credential_flow.current_key += 1;
                            self.advance_credential_flow();
                        } else if let Err(e) = self.store_credentials() {
                            let message = format!("Failed to store credentials: {e}");
                            self.push_event(&message, EventLevel::Error);
                            self.set_status_notice(message, EventLevel::Error);
                            self.credential_flow.reset();
                        }
                    }
                    Err(e) => {
                        let message = format!("OAuth failed: {e}");
                        self.push_event(&message, EventLevel::Error);
                        self.set_status_notice(message, EventLevel::Error);
                        self.credential_flow.reset();
                    }
                }
            }
        }
    }
}

fn summarize_event(kind: &AppEventKind) -> (String, EventLevel, Option<String>) {
    match kind {
        AppEventKind::ChannelDisconnected { platform, reason } => {
            let summary = humanize_disconnect(
                &format!("{} gateway", platform.as_str()),
                reason,
                "Gateway connection lost",
            );
            (summary.clone(), EventLevel::Error, Some(summary))
        }
        AppEventKind::SessionDisconnected {
            session_key,
            reason,
        } => {
            let summary = humanize_disconnect(
                &App::format_session_label(session_key),
                reason,
                "Session disconnected",
            );
            (summary.clone(), EventLevel::Error, Some(summary))
        }
        AppEventKind::Error { context, message } => {
            let summary = humanize_error(context, message);
            (summary.clone(), EventLevel::Error, Some(summary))
        }
        AppEventKind::StreamStarted { session_key, .. } => {
            let summary = format!(
                "Agent is thinking for {}.",
                App::format_session_label(session_key)
            );
            (summary, EventLevel::Info, None)
        }
        AppEventKind::StreamUpdated {
            session_key,
            content_len,
            ..
        } => {
            let summary = format!(
                "Agent is generating a response for {} ({} chars).",
                App::format_session_label(session_key),
                content_len
            );
            (summary, EventLevel::Info, None)
        }
        AppEventKind::StreamCompleted { session_key, .. } => {
            let summary = format!(
                "Agent finished responding in {}.",
                App::format_session_label(session_key)
            );
            (summary, EventLevel::Info, None)
        }
        _ => {
            let level = match kind {
                AppEventKind::Error { .. } => EventLevel::Error,
                _ => EventLevel::Info,
            };
            (kind.to_string(), level, None)
        }
    }
}

fn humanize_disconnect(subject: &str, reason: &str, prefix: &str) -> String {
    let lowered = reason.to_ascii_lowercase();
    if lowered.contains("timeout") || lowered.contains("timed out") {
        return format!("{prefix}: {subject} timed out. Check the network and try again.");
    }
    if lowered.contains("connection refused") {
        return format!("{prefix}: {subject} refused the connection.");
    }
    if lowered.contains("dns") || lowered.contains("resolve") {
        return format!("{prefix}: {subject} could not be resolved.");
    }
    format!("{prefix}: {subject} ({reason}).")
}

fn humanize_error(context: &str, message: &str) -> String {
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("events dropped due to lag") {
        return "The TUI fell behind and dropped some updates. Resize the terminal or reduce log volume."
            .to_string();
    }
    if lowered.contains("timeout") || lowered.contains("timed out") {
        return format!("{context}: the request timed out. Please retry.");
    }
    if lowered.contains("connection refused") {
        return format!("{context}: the target service refused the connection.");
    }
    if lowered.contains("broken pipe") {
        return format!("{context}: the connection closed unexpectedly.");
    }
    format!("{context}: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_secrets::{SecretResult, SecretStore, SecretValue};
    use opengoose_types::{Platform, SessionKey};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    struct MockStore {
        secrets: Mutex<HashMap<String, String>>,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                secrets: Mutex::new(HashMap::new()),
            }
        }
    }

    impl SecretStore for MockStore {
        fn get(&self, key: &str) -> SecretResult<Option<SecretValue>> {
            Ok(self
                .secrets
                .lock()
                .unwrap()
                .get(key)
                .map(|v| SecretValue::new(v.clone())))
        }
        fn set(&self, key: &str, value: &str) -> SecretResult<()> {
            self.secrets
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        fn delete(&self, key: &str) -> SecretResult<bool> {
            Ok(self.secrets.lock().unwrap().remove(key).is_some())
        }
    }

    fn test_app() -> App {
        App::new(AppMode::Normal, None, None)
    }

    fn make_event(kind: AppEventKind) -> AppEvent {
        AppEvent {
            kind,
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn test_handle_message_received_caches_selected_messages() {
        let mut app = test_app();
        let session_key = SessionKey::dm(Platform::Discord, "user1");
        app.sessions.push(SessionListEntry {
            session_key: session_key.clone(),
            active_team: None,
            created_at: None,
            updated_at: None,
            is_active: true,
        });
        app.select_session(0);

        app.handle_app_event(make_event(AppEventKind::MessageReceived {
            session_key: session_key.clone(),
            author: "alice".into(),
            content: "hello".into(),
        }));

        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages.back().unwrap().author, "alice");
        assert_eq!(app.selected_session, Some(session_key));
    }

    #[test]
    fn test_handle_pairing_completed_refreshes_sessions() {
        let mut app = test_app();
        let session_key = SessionKey::dm(Platform::Discord, "user1");
        app.handle_app_event(make_event(AppEventKind::PairingCompleted {
            session_key: session_key.clone(),
        }));
        assert!(app.active_sessions.contains(&session_key));
    }

    #[test]
    fn test_handle_stream_events_update_agent_status() {
        let mut app = test_app();
        let session_key = SessionKey::dm(Platform::Discord, "ch1");
        app.handle_app_event(make_event(AppEventKind::StreamStarted {
            session_key: session_key.clone(),
            stream_id: "s1".into(),
        }));
        assert_eq!(app.agent_status, AgentStatus::Thinking);

        app.handle_app_event(make_event(AppEventKind::StreamUpdated {
            session_key: session_key.clone(),
            stream_id: "s1".into(),
            content_len: 100,
        }));
        assert_eq!(app.agent_status, AgentStatus::Generating);

        app.handle_app_event(make_event(AppEventKind::StreamCompleted {
            session_key,
            stream_id: "s1".into(),
            full_text: "done".into(),
        }));
        assert_eq!(app.agent_status, AgentStatus::Idle);
    }

    #[test]
    fn test_handle_error_sets_notice() {
        let mut app = test_app();
        app.handle_app_event(make_event(AppEventKind::Error {
            context: "relay".into(),
            message: "timed out while waiting".into(),
        }));
        assert_eq!(app.events.back().unwrap().level, EventLevel::Error);
        assert!(
            app.status_notice
                .as_ref()
                .unwrap()
                .message
                .contains("timed out")
        );
    }

    #[test]
    fn test_tick_provider_loading_closed() {
        let mut app = test_app();
        let (tx, rx) = oneshot::channel::<Vec<opengoose_provider_bridge::ProviderSummary>>();
        drop(tx);
        app.provider_loading_rx = Some(rx);

        app.tick();

        assert!(app.provider_loading_rx.is_none());
        assert!(!app.provider_select.visible);
        assert_eq!(app.events.back().unwrap().level, EventLevel::Error);
    }

    #[test]
    fn test_tick_model_loading_success() {
        let mut app = test_app();
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(vec!["gpt-4".to_string(), "gpt-3.5".to_string()]);
        app.model_loading_rx = Some(rx);
        app.model_select.loading = true;

        app.tick();

        assert!(app.model_loading_rx.is_none());
        assert!(!app.model_select.loading);
        assert_eq!(app.model_select.models.len(), 2);
    }

    #[test]
    fn test_tick_provider_loading_success() {
        let mut app = test_app();
        let (tx, rx) = oneshot::channel();
        let providers = vec![opengoose_provider_bridge::ProviderSummary {
            name: "openai".into(),
            display_name: "OpenAI".into(),
            description: "desc".into(),
            default_model: "gpt-4".into(),
            known_models: vec![],
            config_keys: vec![opengoose_provider_bridge::ConfigKeySummary {
                name: "OPENAI_API_KEY".into(),
                required: true,
                secret: true,
                oauth_flow: false,
                default: None,
                primary: true,
            }],
        }];
        let _ = tx.send(providers);
        app.provider_loading_rx = Some(rx);

        app.tick();

        assert!(app.provider_loading_rx.is_none());
        assert_eq!(app.cached_providers.len(), 1);
        assert!(app.provider_select.visible);
    }

    #[test]
    fn test_oauth_failure_surfaces_notice() {
        let store = Arc::new(MockStore::new());
        let mut app = App::with_store(AppMode::Normal, None, None, store, None);
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Err(anyhow::anyhow!("auth failed")));
        app.oauth_done_rx = Some(rx);

        app.tick();

        assert!(app.status_notice.is_some());
        assert!(
            app.status_notice
                .as_ref()
                .unwrap()
                .message
                .contains("OAuth failed")
        );
    }
}
