/// Summary of a provider's metadata, extracted from Goose's `ProviderMetadata`.
#[derive(Debug, Clone, Default)]
pub struct ProviderSummary {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub default_model: String,
    /// Statically-known model names from Goose metadata.
    pub known_models: Vec<String>,
    /// Configuration keys needed by this provider.
    pub config_keys: Vec<ConfigKeySummary>,
}

/// Summary of a single configuration key for a provider.
#[derive(Debug, Clone)]
pub struct ConfigKeySummary {
    pub name: String,
    pub required: bool,
    pub secret: bool,
    /// When `true`, `configure_oauth()` should be called instead of prompting
    /// the user for manual input.
    pub oauth_flow: bool,
    pub default: Option<String>,
    /// Whether this key is shown prominently during setup.
    pub primary: bool,
}
