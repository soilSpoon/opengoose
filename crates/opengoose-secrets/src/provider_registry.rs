/// Information about a single credential key required by a provider.
#[derive(Debug, Clone)]
pub struct KeyInfo {
    /// Environment variable name that Goose reads (e.g. `ANTHROPIC_API_KEY`).
    pub env_var: &'static str,
    /// Human-readable label shown in prompts (e.g. "API Key").
    pub label: &'static str,
    /// If `true`, input is masked (for passwords/tokens). Otherwise plain text.
    pub secret: bool,
}

/// A Goose LLM provider with its authentication requirements.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// CLI identifier matching Goose's provider name (e.g. `"anthropic"`).
    pub id: &'static str,
    /// Human-readable display name (e.g. `"Anthropic"`).
    pub display_name: &'static str,
    /// Credential keys this provider needs. Empty slice means no auth required.
    pub keys: &'static [KeyInfo],
}

impl ProviderInfo {
    /// Returns `true` if this provider requires no credentials (e.g. Ollama).
    pub fn no_auth_required(&self) -> bool {
        self.keys.is_empty()
    }
}

macro_rules! key {
    ($env:expr, $label:expr, secret) => {
        KeyInfo {
            env_var: $env,
            label: $label,
            secret: true,
        }
    };
    ($env:expr, $label:expr) => {
        KeyInfo {
            env_var: $env,
            label: $label,
            secret: false,
        }
    };
}

static PROVIDERS: &[ProviderInfo] = &[
    // ── Single API-key providers ─────────────────────────────
    ProviderInfo {
        id: "anthropic",
        display_name: "Anthropic",
        keys: &[key!("ANTHROPIC_API_KEY", "API Key", secret)],
    },
    ProviderInfo {
        id: "openai",
        display_name: "OpenAI",
        keys: &[key!("OPENAI_API_KEY", "API Key", secret)],
    },
    ProviderInfo {
        id: "google",
        display_name: "Google Gemini",
        keys: &[key!("GOOGLE_API_KEY", "API Key", secret)],
    },
    ProviderInfo {
        id: "openrouter",
        display_name: "OpenRouter",
        keys: &[key!("OPENROUTER_API_KEY", "API Key", secret)],
    },
    ProviderInfo {
        id: "xai",
        display_name: "xAI",
        keys: &[key!("XAI_API_KEY", "API Key", secret)],
    },
    ProviderInfo {
        id: "venice",
        display_name: "Venice",
        keys: &[key!("VENICE_API_KEY", "API Key", secret)],
    },
    ProviderInfo {
        id: "githubcopilot",
        display_name: "GitHub Copilot",
        keys: &[key!("GITHUB_TOKEN", "Token", secret)],
    },
    ProviderInfo {
        id: "tetrate",
        display_name: "Tetrate",
        keys: &[key!("TETRATE_API_KEY", "API Key", secret)],
    },
    // ── Multi-key providers ──────────────────────────────────
    ProviderInfo {
        id: "litellm",
        display_name: "LiteLLM",
        keys: &[
            key!("LITELLM_API_KEY", "API Key", secret),
            key!("LITELLM_HOST", "Host URL"),
        ],
    },
    ProviderInfo {
        id: "azure",
        display_name: "Azure OpenAI",
        keys: &[
            key!("AZURE_OPENAI_API_KEY", "API Key", secret),
            key!("AZURE_OPENAI_ENDPOINT", "Endpoint URL"),
            key!("AZURE_OPENAI_DEPLOYMENT_NAME", "Deployment Name"),
        ],
    },
    ProviderInfo {
        id: "databricks",
        display_name: "Databricks",
        keys: &[
            key!("DATABRICKS_TOKEN", "Token", secret),
            key!("DATABRICKS_HOST", "Host URL"),
        ],
    },
    ProviderInfo {
        id: "snowflake",
        display_name: "Snowflake",
        keys: &[
            key!("SNOWFLAKE_TOKEN", "Token", secret),
            key!("SNOWFLAKE_HOST", "Host URL"),
        ],
    },
    ProviderInfo {
        id: "bedrock",
        display_name: "AWS Bedrock",
        keys: &[
            key!("AWS_PROFILE", "AWS Profile"),
            key!("AWS_REGION", "AWS Region"),
        ],
    },
    ProviderInfo {
        id: "gcpvertexai",
        display_name: "GCP Vertex AI",
        keys: &[
            key!("GCP_PROJECT_ID", "Project ID"),
            key!("GCP_LOCATION", "Location"),
        ],
    },
    ProviderInfo {
        id: "sagemaker_tgi",
        display_name: "SageMaker TGI",
        keys: &[
            key!("AWS_PROFILE", "AWS Profile"),
            key!("AWS_REGION", "AWS Region"),
        ],
    },
    // ── No-auth providers ────────────────────────────────────
    ProviderInfo {
        id: "ollama",
        display_name: "Ollama",
        keys: &[],
    },
    ProviderInfo {
        id: "local_inference",
        display_name: "Local Inference",
        keys: &[],
    },
];

/// Returns all known Goose providers.
pub fn all_providers() -> &'static [ProviderInfo] {
    PROVIDERS
}

/// Look up a provider by its CLI identifier.
pub fn find_provider(id: &str) -> Option<&'static ProviderInfo> {
    PROVIDERS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_providers_not_empty() {
        assert!(!all_providers().is_empty());
    }

    #[test]
    fn test_all_ids_unique() {
        let providers = all_providers();
        let mut ids: Vec<&str> = providers.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), providers.len());
    }

    #[test]
    fn test_find_provider_existing() {
        let p = find_provider("anthropic").unwrap();
        assert_eq!(p.display_name, "Anthropic");
        assert_eq!(p.keys.len(), 1);
        assert_eq!(p.keys[0].env_var, "ANTHROPIC_API_KEY");
        assert!(p.keys[0].secret);
    }

    #[test]
    fn test_find_provider_multi_key() {
        let p = find_provider("azure").unwrap();
        assert_eq!(p.keys.len(), 3);
    }

    #[test]
    fn test_find_provider_no_auth() {
        let p = find_provider("ollama").unwrap();
        assert!(p.no_auth_required());
        assert!(p.keys.is_empty());
    }

    #[test]
    fn test_find_provider_nonexistent() {
        assert!(find_provider("nonexistent").is_none());
    }

    #[test]
    fn test_provider_ids_are_non_empty() {
        for provider in all_providers() {
            assert!(
                !provider.id.is_empty(),
                "provider id should not be empty for {}",
                provider.display_name
            );
            assert!(
                !provider.display_name.is_empty(),
                "provider display name should not be empty for {}",
                provider.id
            );
        }
    }

    #[test]
    fn test_provider_keys_have_labels_and_env_vars() {
        for provider in all_providers() {
            for key in provider.keys {
                assert!(
                    !key.env_var.is_empty(),
                    "env var should not be empty for {}",
                    provider.id
                );
                assert!(
                    !key.label.is_empty(),
                    "label should not be empty for {} key {}",
                    provider.id,
                    key.env_var
                );
            }
        }
    }
}
