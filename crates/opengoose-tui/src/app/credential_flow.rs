use anyhow::Result;
use opengoose_provider_bridge::GooseProviderService;
use opengoose_secrets::{ConfigFile, SecretKey};
use tokio::sync::oneshot;

use super::state::*;

impl App {
    pub fn save_secret_and_notify(&mut self) -> Result<()> {
        let token = self.secret_input.input.clone();
        if token.is_empty() {
            self.secret_input.status_message = Some("Token cannot be empty".into());
            return Ok(());
        }

        let key = SecretKey::DiscordBotToken;

        // Store in keyring via injected store
        self.store.set(key.as_str(), &token)?;

        // Mark in config
        let mut config = self.load_config()?;
        config.mark_in_keyring(&key);
        self.save_config(&config)?;

        self.secret_input.visible = false;
        self.secret_input.input.clear();
        self.secret_input.status_message = None;

        // If we have a token_sender, send the token and switch to Normal mode
        if let Some(sender) = self.token_sender.take() {
            let _ = sender.send(token);
            self.mode = AppMode::Normal;
        } else {
            self.push_event("Token updated. Restart to connect.", EventLevel::Info);
        }

        Ok(())
    }

    pub fn open_provider_select(&mut self) {
        self.open_provider_select_for(ProviderSelectPurpose::Configure);
    }

    /// Open the provider selection modal for a specific purpose.
    pub fn open_provider_select_for(&mut self, purpose: ProviderSelectPurpose) {
        self.provider_select.purpose = purpose;
        if !self.cached_providers.is_empty() {
            self.populate_provider_select_from_cache();
        } else {
            // Trigger async load
            let (tx, rx) = oneshot::channel();
            tokio::spawn(async move {
                let providers = GooseProviderService::list_providers().await;
                let _ = tx.send(providers);
            });
            self.provider_loading_rx = Some(rx);
            self.push_event("Loading providers...", EventLevel::Info);
        }
    }

    pub(crate) fn populate_provider_select_from_cache(&mut self) {
        let mut providers = Vec::new();
        let mut ids = Vec::new();
        let show_all = self.provider_select.purpose == ProviderSelectPurpose::ListModels;
        for p in &self.cached_providers {
            if show_all || !p.config_keys.is_empty() {
                let has_oauth = p.config_keys.iter().any(|k| k.oauth_flow);
                let label = if has_oauth {
                    format!("{} (OAuth)", p.display_name)
                } else {
                    p.display_name.clone()
                };
                providers.push(label);
                ids.push(p.name.clone());
            }
        }
        self.provider_select.providers = providers;
        self.provider_select.provider_ids = ids;
        self.provider_select.selected = 0;
        self.provider_select.visible = true;
    }

    /// Handle Enter on the provider select modal — dispatches based on purpose.
    pub fn confirm_provider_select(&mut self) {
        match self.provider_select.purpose {
            ProviderSelectPurpose::Configure => self.start_credential_flow(),
            ProviderSelectPurpose::ListModels => {
                let idx = self.provider_select.selected;
                if let Some(id) = self.provider_select.provider_ids.get(idx).cloned() {
                    self.provider_select.visible = false;
                    self.fetch_models(&id);
                }
            }
        }
    }

    /// Start the credential input flow for the selected provider.
    pub fn start_credential_flow(&mut self) {
        let idx = self.provider_select.selected;
        let provider_id = match self.provider_select.provider_ids.get(idx) {
            Some(id) => id.clone(),
            None => return,
        };
        self.provider_select.visible = false;

        let provider = match self.cached_providers.iter().find(|p| p.name == provider_id) {
            Some(p) => p.clone(),
            None => return,
        };

        self.credential_flow.provider_id = Some(provider_id);
        self.credential_flow.provider_display = Some(provider.display_name.clone());
        self.credential_flow.keys = provider
            .config_keys
            .iter()
            .map(|k| CredentialKey {
                env_var: k.name.clone(),
                label: if k.oauth_flow {
                    "OAuth".to_string()
                } else if k.name.ends_with("_API_KEY") || k.name.ends_with("_KEY") {
                    "API Key".to_string()
                } else if k.name.ends_with("_TOKEN") {
                    "Token".to_string()
                } else if k.name.contains("HOST") || k.name.contains("ENDPOINT") {
                    "URL".to_string()
                } else {
                    "Value".to_string()
                },
                secret: k.secret,
                oauth_flow: k.oauth_flow,
                required: k.required,
                default: k.default.clone(),
            })
            .collect();
        self.credential_flow.current_key = 0;
        self.credential_flow.collected.clear();

        // Open the secret input for the first key (or start OAuth)
        self.advance_credential_flow();
    }

    /// Advance to the next credential key, handling OAuth keys automatically.
    pub(crate) fn advance_credential_flow(&mut self) {
        match self.credential_flow.current() {
            Some(key) if key.oauth_flow => {
                // Start OAuth in background
                let provider_name = self.credential_flow.provider_id.clone().unwrap_or_default();
                let (tx, rx) = oneshot::channel();
                tokio::spawn(async move {
                    let result = GooseProviderService::run_oauth(&provider_name).await;
                    let _ = tx.send(result);
                });
                self.oauth_done_rx = Some(rx);
                self.push_event(
                    &format!(
                        "OAuth authentication in progress for {}...",
                        self.credential_flow
                            .provider_display
                            .as_deref()
                            .unwrap_or("")
                    ),
                    EventLevel::Info,
                );
            }
            Some(_) => {
                self.open_credential_input();
            }
            None => {
                // All keys collected — store them
                let _ = self.store_credentials();
            }
        }
    }

    /// Open the secret_input modal for the current credential key.
    fn open_credential_input(&mut self) {
        if let Some(key) = self.credential_flow.current() {
            let optional_hint = if !key.required {
                " (optional)"
            } else if key.default.is_some() {
                " (Enter for default)"
            } else {
                ""
            };
            let label = format!(
                "{} — {} [{}]{}",
                self.credential_flow
                    .provider_display
                    .as_deref()
                    .unwrap_or(""),
                key.label,
                key.env_var,
                optional_hint
            );
            self.secret_input.visible = true;
            self.secret_input.input.clear();
            self.secret_input.status_message = None;
            self.secret_input.title = Some(label);
            self.secret_input.is_secret = key.secret;
        }
    }

    /// Save the current credential input value and advance to the next key or finish.
    pub fn save_credential_and_advance(&mut self) -> Result<()> {
        let raw_value = self.secret_input.input.clone();
        let current_key = match self.credential_flow.current() {
            Some(k) => k.clone(),
            None => return Ok(()),
        };

        let value = if raw_value.is_empty() {
            if let Some(ref default) = current_key.default {
                default.clone()
            } else if current_key.required {
                self.secret_input.status_message = Some("Value cannot be empty".into());
                return Ok(());
            } else {
                // Optional key with no default — skip it
                return self.advance_to_next_key();
            }
        } else {
            raw_value
        };

        self.credential_flow
            .collected
            .push((current_key.env_var.clone(), value));

        self.advance_to_next_key()
    }

    /// Move to the next credential key or finish storing.
    fn advance_to_next_key(&mut self) -> Result<()> {
        if self.credential_flow.has_more() {
            self.credential_flow.current_key += 1;
            self.secret_input.visible = false;
            self.secret_input.input.clear();
            self.advance_credential_flow();
        } else {
            self.store_credentials()?;
        }
        Ok(())
    }

    /// Store all collected credentials in keyring and config.
    pub(crate) fn store_credentials(&mut self) -> Result<()> {
        let provider_id = match &self.credential_flow.provider_id {
            Some(id) => id.clone(),
            None => return Ok(()),
        };
        let display = self
            .credential_flow
            .provider_display
            .clone()
            .unwrap_or_default();

        let mut keyring_keys = Vec::new();
        for (env_var, value) in &self.credential_flow.collected {
            let keyring_key = env_var.to_lowercase();
            self.store.set(&keyring_key, value)?;
            keyring_keys.push(keyring_key);
        }

        let mut config = self.load_config()?;
        config.mark_provider(&provider_id, keyring_keys);
        self.save_config(&config)?;

        self.push_event(&format!("Authenticated with {display}."), EventLevel::Info);

        // Reset UI state
        self.secret_input.visible = false;
        self.secret_input.input.clear();
        self.secret_input.status_message = None;
        self.secret_input.title = None;
        self.secret_input.is_secret = true;
        self.credential_flow.reset();

        Ok(())
    }

    /// Fetch models for a provider asynchronously.
    pub fn fetch_models(&mut self, provider_name: &str) {
        let name = provider_name.to_string();
        self.model_select.provider_name = name.clone();
        self.model_select.loading = true;
        self.model_select.models.clear();
        self.model_select.selected = 0;
        self.model_select.visible = true;

        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            let result = GooseProviderService::fetch_models(&name).await;
            let _ = tx.send(result.unwrap_or_default());
        });
        self.model_loading_rx = Some(rx);
    }

    fn load_config(&self) -> Result<ConfigFile> {
        match &self.config_path {
            Some(p) => ConfigFile::load_from(p),
            None => ConfigFile::load(),
        }
    }

    fn save_config(&self, config: &ConfigFile) -> Result<()> {
        match &self.config_path {
            Some(p) => config.save_to(p),
            None => config.save(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_provider_bridge::ConfigKeySummary;
    use opengoose_secrets::{SecretResult, SecretStore, SecretValue};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

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

    fn test_app_with_store() -> (App, Arc<MockStore>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let store = Arc::new(MockStore::new());
        let app = App::with_store(
            AppMode::Normal,
            None,
            None,
            store.clone(),
            Some(config_path),
        );
        (app, store, dir)
    }

    fn make_provider(
        name: &str,
        display: &str,
        keys: Vec<ConfigKeySummary>,
    ) -> opengoose_provider_bridge::ProviderSummary {
        opengoose_provider_bridge::ProviderSummary {
            name: name.into(),
            display_name: display.into(),
            description: "desc".into(),
            default_model: "model".into(),
            known_models: vec![],
            config_keys: keys,
        }
    }

    fn api_key(name: &str) -> ConfigKeySummary {
        ConfigKeySummary {
            name: name.into(),
            required: true,
            secret: true,
            oauth_flow: false,
            default: None,
            primary: true,
        }
    }

    #[test]
    fn test_populate_provider_select_configure_filters_empty_keys() {
        let (mut app, _, _dir) = test_app_with_store();
        app.cached_providers = vec![
            make_provider("openai", "OpenAI", vec![api_key("OPENAI_API_KEY")]),
            make_provider("local", "Local", vec![]),
        ];
        app.provider_select.purpose = ProviderSelectPurpose::Configure;
        app.populate_provider_select_from_cache();

        assert_eq!(app.provider_select.providers.len(), 1);
        assert_eq!(app.provider_select.providers[0], "OpenAI");
        assert_eq!(app.provider_select.provider_ids[0], "openai");
        assert!(app.provider_select.visible);
        assert_eq!(app.provider_select.selected, 0);
    }

    #[test]
    fn test_populate_provider_select_list_models_shows_all() {
        let (mut app, _, _dir) = test_app_with_store();
        app.cached_providers = vec![
            make_provider("openai", "OpenAI", vec![api_key("OPENAI_API_KEY")]),
            make_provider("local", "Local", vec![]),
        ];
        app.provider_select.purpose = ProviderSelectPurpose::ListModels;
        app.populate_provider_select_from_cache();

        assert_eq!(app.provider_select.providers.len(), 2);
    }

    #[test]
    fn test_populate_provider_select_oauth_label() {
        let (mut app, _, _dir) = test_app_with_store();
        app.cached_providers = vec![make_provider(
            "google",
            "Google",
            vec![ConfigKeySummary {
                name: "GOOGLE_TOKEN".into(),
                required: true,
                secret: true,
                oauth_flow: true,
                default: None,
                primary: true,
            }],
        )];
        app.provider_select.purpose = ProviderSelectPurpose::Configure;
        app.populate_provider_select_from_cache();

        assert_eq!(app.provider_select.providers[0], "Google (OAuth)");
    }

    #[test]
    fn test_start_credential_flow_sets_keys() {
        let (mut app, _, _dir) = test_app_with_store();
        app.cached_providers = vec![make_provider(
            "openai",
            "OpenAI",
            vec![api_key("OPENAI_API_KEY")],
        )];
        app.provider_select.provider_ids = vec!["openai".into()];
        app.provider_select.selected = 0;

        app.start_credential_flow();

        assert_eq!(app.credential_flow.provider_id, Some("openai".into()));
        assert_eq!(app.credential_flow.provider_display, Some("OpenAI".into()));
        assert_eq!(app.credential_flow.keys.len(), 1);
        assert_eq!(app.credential_flow.keys[0].env_var, "OPENAI_API_KEY");
        assert_eq!(app.credential_flow.keys[0].label, "API Key");
        assert!(app.secret_input.visible);
    }

    #[test]
    fn test_start_credential_flow_key_label_mapping() {
        let (mut app, _, _dir) = test_app_with_store();
        let keys = vec![
            ConfigKeySummary {
                name: "MY_API_KEY".into(),
                required: true,
                secret: true,
                oauth_flow: false,
                default: None,
                primary: true,
            },
            ConfigKeySummary {
                name: "MY_TOKEN".into(),
                required: true,
                secret: true,
                oauth_flow: false,
                default: None,
                primary: true,
            },
            ConfigKeySummary {
                name: "HOST_URL".into(),
                required: false,
                secret: false,
                oauth_flow: false,
                default: Some("http://localhost".into()),
                primary: false,
            },
            ConfigKeySummary {
                name: "SOME_SETTING".into(),
                required: false,
                secret: false,
                oauth_flow: false,
                default: None,
                primary: false,
            },
        ];
        app.cached_providers = vec![make_provider("test", "Test", keys)];
        app.provider_select.provider_ids = vec!["test".into()];
        app.provider_select.selected = 0;

        app.start_credential_flow();

        assert_eq!(app.credential_flow.keys[0].label, "API Key");
        assert_eq!(app.credential_flow.keys[1].label, "Token");
        assert_eq!(app.credential_flow.keys[2].label, "URL");
        assert_eq!(app.credential_flow.keys[3].label, "Value");
    }

    #[test]
    fn test_start_credential_flow_no_provider_ids() {
        let (mut app, _, _dir) = test_app_with_store();
        app.provider_select.provider_ids = vec![];
        app.provider_select.selected = 0;

        app.start_credential_flow();
        assert!(app.credential_flow.provider_id.is_none());
    }

    #[test]
    fn test_save_credential_empty_required() {
        let (mut app, _, _dir) = test_app_with_store();
        app.credential_flow.provider_id = Some("test".into());
        app.credential_flow.keys.push(CredentialKey {
            env_var: "API_KEY".into(),
            label: "Key".into(),
            secret: true,
            oauth_flow: false,
            required: true,
            default: None,
        });
        app.secret_input.input.clear();

        let result = app.save_credential_and_advance();
        assert!(result.is_ok());
        assert_eq!(
            app.secret_input.status_message,
            Some("Value cannot be empty".into())
        );
    }

    #[test]
    fn test_save_credential_uses_default_when_empty() {
        let (mut app, store, _dir) = test_app_with_store();
        app.credential_flow.provider_id = Some("test".into());
        app.credential_flow.provider_display = Some("Test".into());
        app.credential_flow.keys.push(CredentialKey {
            env_var: "HOST".into(),
            label: "URL".into(),
            secret: false,
            oauth_flow: false,
            required: true,
            default: Some("http://localhost:8080".into()),
        });
        app.secret_input.input.clear();

        let result = app.save_credential_and_advance();
        assert!(result.is_ok());

        assert_eq!(
            store.secrets.lock().unwrap().get("host"),
            Some(&"http://localhost:8080".into())
        );
    }

    #[test]
    fn test_save_credential_with_value() {
        let (mut app, store, _dir) = test_app_with_store();
        app.credential_flow.provider_id = Some("test".into());
        app.credential_flow.provider_display = Some("Test".into());
        app.credential_flow.keys.push(CredentialKey {
            env_var: "API_KEY".into(),
            label: "Key".into(),
            secret: true,
            oauth_flow: false,
            required: true,
            default: None,
        });
        app.secret_input.input = "sk-12345".into();

        let result = app.save_credential_and_advance();
        assert!(result.is_ok());

        assert_eq!(
            store.secrets.lock().unwrap().get("api_key"),
            Some(&"sk-12345".into())
        );
    }

    #[test]
    fn test_save_credential_no_current_key() {
        let (mut app, _, _dir) = test_app_with_store();
        let result = app.save_credential_and_advance();
        assert!(result.is_ok());
    }

    #[test]
    fn test_store_credentials_no_provider() {
        let (mut app, _, _dir) = test_app_with_store();
        app.credential_flow.provider_id = None;
        let result = app.store_credentials();
        assert!(result.is_ok());
    }

    #[test]
    fn test_store_credentials_resets_ui() {
        let (mut app, _, _dir) = test_app_with_store();
        app.credential_flow.provider_id = Some("openai".into());
        app.credential_flow.provider_display = Some("OpenAI".into());
        app.credential_flow
            .collected
            .push(("OPENAI_API_KEY".into(), "sk-key".into()));
        app.secret_input.visible = true;
        app.secret_input.title = Some("title".into());

        let result = app.store_credentials();
        assert!(result.is_ok());

        assert!(!app.secret_input.visible);
        assert!(app.secret_input.input.is_empty());
        assert!(app.secret_input.status_message.is_none());
        assert!(app.secret_input.title.is_none());
        assert!(app.secret_input.is_secret);
        assert!(app.credential_flow.provider_id.is_none());
        assert!(app.events.back().unwrap().summary.contains("Authenticated"));
    }

    #[test]
    fn test_open_credential_input_optional_hint() {
        let (mut app, _, _dir) = test_app_with_store();
        app.credential_flow.provider_display = Some("Test".into());
        app.credential_flow.keys.push(CredentialKey {
            env_var: "OPTIONAL_KEY".into(),
            label: "Value".into(),
            secret: false,
            oauth_flow: false,
            required: false,
            default: None,
        });

        app.advance_credential_flow();

        let title = app.secret_input.title.as_deref().unwrap();
        assert!(title.contains("(optional)"));
        assert!(!app.secret_input.is_secret);
    }

    #[test]
    fn test_open_credential_input_default_hint() {
        let (mut app, _, _dir) = test_app_with_store();
        app.credential_flow.provider_display = Some("Test".into());
        app.credential_flow.keys.push(CredentialKey {
            env_var: "HOST".into(),
            label: "URL".into(),
            secret: false,
            oauth_flow: false,
            required: true,
            default: Some("http://localhost".into()),
        });

        app.advance_credential_flow();

        let title = app.secret_input.title.as_deref().unwrap();
        assert!(title.contains("(Enter for default)"));
    }

    #[test]
    fn test_save_credential_optional_skip() {
        let (mut app, _, _dir) = test_app_with_store();
        app.credential_flow.provider_id = Some("test".into());
        app.credential_flow.provider_display = Some("Test".into());
        app.credential_flow.keys.push(CredentialKey {
            env_var: "OPTIONAL".into(),
            label: "Value".into(),
            secret: false,
            oauth_flow: false,
            required: false,
            default: None,
        });
        app.secret_input.input.clear();

        let result = app.save_credential_and_advance();
        assert!(result.is_ok());
        assert!(app.credential_flow.collected.is_empty());
    }

    #[test]
    fn test_open_provider_select_sets_purpose() {
        let (mut app, _, _dir) = test_app_with_store();
        app.cached_providers = vec![make_provider("openai", "OpenAI", vec![api_key("KEY")])];
        app.open_provider_select();
        assert_eq!(
            app.provider_select.purpose,
            ProviderSelectPurpose::Configure
        );
        assert!(app.provider_select.visible);
    }
}
