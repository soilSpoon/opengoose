use opengoose_provider_bridge::ConfigKeySummary;

use super::*;

impl App {
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
            .map(|key| CredentialKey {
                env_var: key.name.clone(),
                label: credential_label(key),
                secret: key.secret,
                oauth_flow: key.oauth_flow,
                required: key.required,
                default: key.default.clone(),
            })
            .collect();
        self.credential_flow.current_key = 0;
        self.credential_flow.collected.clear();

        self.advance_credential_flow();
    }

    /// Advance to the next credential key, handling OAuth keys automatically.
    pub(crate) fn advance_credential_flow(&mut self) {
        match self.credential_flow.current() {
            Some(key) if key.oauth_flow => {
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
            Some(_) => self.open_credential_input(),
            None => {
                let _ = self.store_credentials();
            }
        }
    }

    /// Save the current credential input value and advance to the next key or finish.
    pub fn save_credential_and_advance(&mut self) -> Result<()> {
        let raw_value = self.secret_input.input.clone();
        let current_key = match self.credential_flow.current() {
            Some(key) => key.clone(),
            None => return Ok(()),
        };

        let value = if raw_value.is_empty() {
            if let Some(ref default) = current_key.default {
                default.clone()
            } else if current_key.required {
                self.secret_input.status_message = Some("Value cannot be empty".into());
                return Ok(());
            } else {
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
}

fn credential_label(key: &ConfigKeySummary) -> String {
    if key.oauth_flow {
        "OAuth".to_string()
    } else if key.name.ends_with("_API_KEY") || key.name.ends_with("_KEY") {
        "API Key".to_string()
    } else if key.name.ends_with("_TOKEN") {
        "Token".to_string()
    } else if key.name.contains("HOST") || key.name.contains("ENDPOINT") {
        "URL".to_string()
    } else {
        "Value".to_string()
    }
}
