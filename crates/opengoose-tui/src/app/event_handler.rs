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
    fn test_handle_team_activated() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "ch1");
        app.handle_app_event(make_event(AppEventKind::TeamActivated {
            session_key: sk.clone(),
            team_name: "devops".into(),
        }));
        assert_eq!(app.active_teams.get(&sk), Some(&"devops".into()));
    }

    #[test]
    fn test_handle_team_deactivated() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "ch1");
        app.active_teams.insert(sk.clone(), "devops".into());
        app.handle_app_event(make_event(AppEventKind::TeamDeactivated {
            session_key: sk.clone(),
        }));
        assert!(!app.active_teams.contains_key(&sk));
    }

    #[test]
    fn test_handle_goose_ready_goes_to_events() {
        let mut app = test_app();
        app.handle_app_event(make_event(AppEventKind::GooseReady));
        assert_eq!(app.events.len(), 1);
        assert_eq!(app.events.back().unwrap().level, EventLevel::Info);
    }

    #[test]
    fn test_handle_team_run_started_goes_to_events() {
        let mut app = test_app();
        app.handle_app_event(make_event(AppEventKind::TeamRunStarted {
            team: "devops".into(),
            workflow: "chain".into(),
            input: "do stuff".into(),
        }));
        assert_eq!(app.events.len(), 1);
    }

    #[test]
    fn test_handle_stream_events_go_to_events() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "ch1");
        app.handle_app_event(make_event(AppEventKind::StreamStarted {
            session_key: sk.clone(),
            stream_id: "s1".into(),
        }));
        app.handle_app_event(make_event(AppEventKind::StreamUpdated {
            session_key: sk.clone(),
            stream_id: "s1".into(),
            content_len: 100,
        }));
        app.handle_app_event(make_event(AppEventKind::StreamCompleted {
            session_key: sk,
            stream_id: "s1".into(),
            full_text: "done".into(),
        }));
        assert_eq!(app.events.len(), 3);
    }

    #[test]
    fn test_tick_provider_loading_closed() {
        let mut app = test_app();
        let (tx, rx) = oneshot::channel::<Vec<opengoose_provider_bridge::ProviderSummary>>();
        drop(tx); // Close the channel
        app.provider_loading_rx = Some(rx);

        app.tick();

        assert!(app.provider_loading_rx.is_none());
        assert!(!app.provider_select.visible);
        assert_eq!(app.events.back().unwrap().level, EventLevel::Error);
    }

    #[test]
    fn test_tick_model_loading_closed() {
        let mut app = test_app();
        let (tx, rx) = oneshot::channel::<Vec<String>>();
        drop(tx);
        app.model_loading_rx = Some(rx);
        app.model_select.loading = true;

        app.tick();

        assert!(app.model_loading_rx.is_none());
        assert!(!app.model_select.loading);
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
    fn test_tick_oauth_failed() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut app = App::with_store(
            AppMode::Normal,
            None,
            None,
            Arc::new(MockStore::new()),
            Some(config_path),
        );
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Err(anyhow::anyhow!("auth error")));
        app.oauth_done_rx = Some(rx);
        app.credential_flow.provider_display = Some("Test".into());

        app.tick();

        assert!(app.oauth_done_rx.is_none());
        assert!(app.credential_flow.provider_id.is_none()); // reset
        assert!(app.events.back().unwrap().summary.contains("OAuth failed"));
    }

    #[test]
    fn test_tick_oauth_success_stores() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut app = App::with_store(
            AppMode::Normal,
            None,
            None,
            Arc::new(MockStore::new()),
            Some(config_path),
        );
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(()));
        app.oauth_done_rx = Some(rx);
        app.credential_flow.provider_id = Some("google".into());
        app.credential_flow.provider_display = Some("Google".into());
        // Single OAuth key, no more keys
        app.credential_flow.keys.push(CredentialKey {
            env_var: "TOKEN".into(),
            label: "OAuth".into(),
            secret: true,
            oauth_flow: true,
            required: true,
            default: None,
        });

        app.tick();

        assert!(app.oauth_done_rx.is_none());
        assert!(
            app.events
                .iter()
                .any(|e| e.summary.contains("OAuth completed"))
        );
    }

    #[test]
    fn test_tick_oauth_closed() {
        let mut app = test_app();
        let (tx, rx) = oneshot::channel::<anyhow::Result<()>>();
        drop(tx);
        app.oauth_done_rx = Some(rx);

        app.tick();

        assert!(app.oauth_done_rx.is_none());
        assert!(app.events.back().unwrap().summary.contains("OAuth failed"));
    }

    #[test]
    fn test_tick_no_receivers() {
        let mut app = test_app();
        // Should not panic when no receivers are set
        app.tick();
    }
}
