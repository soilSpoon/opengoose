use opengoose_profiles::AgentProfile;

/// Last-resort provider/model if neither profile settings nor the system
/// Goose config (GOOSE_PROVIDER / GOOSE_MODEL) supply a value.
pub(crate) const FALLBACK_PROVIDER: &str = "anthropic";
pub(crate) const FALLBACK_MODEL: &str = "claude-sonnet-4-6";

#[derive(Debug, Clone)]
pub(crate) struct ProviderTarget {
    pub provider_name: String,
    pub model_name: String,
}

#[derive(Debug)]
pub(crate) struct AttemptFailure {
    pub error: anyhow::Error,
    pub emitted_content: bool,
}

impl AttemptFailure {
    pub fn new(error: anyhow::Error, emitted_content: bool) -> Self {
        Self {
            error,
            emitted_content,
        }
    }
}

/// A parsed agent output: the main response plus any structured actions.
#[derive(Debug)]
pub struct AgentOutput {
    /// The final response text (with @mentions and [BROADCAST] lines stripped).
    pub response: String,
    /// Delegations detected: (recipient_agent, message).
    pub delegations: Vec<(String, String)>,
    /// Broadcast messages detected.
    pub broadcasts: Vec<String>,
}

/// Summary of non-message Goose events observed during an agent run.
///
/// These events are collected during `run_with_events()` so callers can
/// forward them to the OpenGoose EventBus (e.g., model changes, context
/// compaction, MCP notifications).
#[derive(Debug, Default)]
pub struct AgentEventSummary {
    /// Model changes: (new_model, mode).
    pub model_changes: Vec<(String, String)>,
    /// Number of context compaction events observed.
    pub context_compactions: u32,
    /// MCP extension notifications: (extension_name).
    pub extension_notifications: Vec<String>,
}

/// Resolve the provider fallback chain from profile settings and Goose config.
pub(crate) fn resolve_provider_chain(profile: &AgentProfile) -> Vec<ProviderTarget> {
    use goose::config::Config as GooseConfig;

    let settings = profile.settings.as_ref();
    let goose_cfg = GooseConfig::global();
    let provider_name = settings
        .and_then(|s| s.goose_provider.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| {
            goose_cfg
                .get_param::<String>("GOOSE_PROVIDER")
                .unwrap_or_else(|_| FALLBACK_PROVIDER.to_string())
        });
    let model_name = settings
        .and_then(|s| s.goose_model.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| {
            goose_cfg
                .get_param::<String>("GOOSE_MODEL")
                .unwrap_or_else(|_| FALLBACK_MODEL.to_string())
        });

    let mut chain = vec![ProviderTarget {
        provider_name: provider_name.clone(),
        model_name: model_name.clone(),
    }];

    if let Some(settings) = settings {
        for fallback in &settings.provider_fallbacks {
            if fallback.goose_provider.trim().is_empty() {
                continue;
            }

            let target = ProviderTarget {
                provider_name: fallback.goose_provider.clone(),
                model_name: fallback
                    .goose_model
                    .clone()
                    .unwrap_or_else(|| model_name.clone()),
            };

            if chain.iter().any(|candidate| {
                candidate.provider_name == target.provider_name
                    && candidate.model_name == target.model_name
            }) {
                continue;
            }

            chain.push(target);
        }
    }

    chain
}
