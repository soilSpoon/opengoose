use super::*;

impl App {
    pub fn open_provider_select(&mut self) {
        self.open_provider_select_for(ProviderSelectPurpose::Configure);
    }

    /// Open the provider selection modal for a specific purpose.
    pub fn open_provider_select_for(&mut self, purpose: ProviderSelectPurpose) {
        self.provider_select.purpose = purpose;
        if !self.cached_providers.is_empty() {
            self.populate_provider_select_from_cache();
        } else {
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
        let show_all = self.provider_select.purpose == ProviderSelectPurpose::ListModels;
        let mut providers = Vec::new();
        let mut ids = Vec::new();

        for provider in &self.cached_providers {
            if show_all || !provider.config_keys.is_empty() {
                let has_oauth = provider.config_keys.iter().any(|key| key.oauth_flow);
                let label = if has_oauth {
                    format!("{} (OAuth)", provider.display_name)
                } else {
                    provider.display_name.clone()
                };
                providers.push(label);
                ids.push(provider.name.clone());
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
}
