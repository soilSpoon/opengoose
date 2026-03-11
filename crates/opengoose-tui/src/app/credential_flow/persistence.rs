use super::*;

impl App {
    pub fn save_secret_and_notify(&mut self) -> Result<()> {
        let token = self.secret_input.input.clone();
        if token.is_empty() {
            self.secret_input.status_message = Some("Token cannot be empty".into());
            return Ok(());
        }

        let key = SecretKey::DiscordBotToken;
        self.store.set(key.as_str(), &token)?;

        let mut config = match &self.config_path {
            Some(path) => ConfigFile::load_from(path)?,
            None => ConfigFile::load()?,
        };
        config.mark_in_keyring(&key);
        match &self.config_path {
            Some(path) => config.save_to(path)?,
            None => config.save()?,
        }

        self.secret_input.visible = false;
        self.secret_input.input.clear();
        self.secret_input.status_message = None;

        if let Some(sender) = self.token_sender.take() {
            let _ = sender.send(token);
            self.mode = AppMode::Normal;
        } else {
            self.push_event("Token updated. Restart to connect.", EventLevel::Info);
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

        let mut config = match &self.config_path {
            Some(path) => ConfigFile::load_from(path)?,
            None => ConfigFile::load()?,
        };
        config.mark_provider(&provider_id, keyring_keys);
        match &self.config_path {
            Some(path) => config.save_to(path)?,
            None => config.save()?,
        }

        self.push_event(&format!("Authenticated with {display}."), EventLevel::Info);
        self.secret_input.visible = false;
        self.secret_input.input.clear();
        self.secret_input.status_message = None;
        self.secret_input.title = None;
        self.secret_input.is_secret = true;
        self.credential_flow.reset();

        Ok(())
    }
}
