// Agent creation — standalone factory for Goose Agent instances.

use anyhow::{Context, Result};
use goose::agents::Agent;
use goose::config::goose_mode::GooseMode;
use goose::model::ModelConfig;
use goose::session::session_manager::SessionType;
use tracing::info;

pub struct AgentConfig {
    pub session_id: String,
    pub system_prompt: Option<String>,
}

/// Create a Goose Agent with the given config.
/// Reads GOOSE_PROVIDER and GOOSE_MODEL from the environment.
pub async fn create_agent(config: AgentConfig) -> Result<Agent> {
    let provider_name =
        std::env::var("GOOSE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());

    let agent = Agent::new();

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let session = agent
        .config
        .session_manager
        .create_session(
            cwd,
            config.session_id.clone(),
            SessionType::User,
            GooseMode::Auto,
        )
        .await
        .context("failed to create session")?;

    let provider = match std::env::var("GOOSE_MODEL") {
        Ok(model_name) => {
            info!(
                provider = %provider_name,
                model = %model_name,
                session = %config.session_id,
                "creating agent"
            );
            let model_config = ModelConfig::new(&model_name)
                .context("invalid model config")?
                .with_canonical_limits(&provider_name);
            goose::providers::create(&provider_name, model_config, vec![]).await
        }
        Err(_) => {
            info!(
                provider = %provider_name,
                model = "default",
                session = %config.session_id,
                "creating agent"
            );
            goose::providers::create_with_default_model(&provider_name, vec![]).await
        }
    }
    .context("failed to create provider")?;

    agent
        .update_provider(provider, &session.id)
        .await
        .context("failed to set provider")?;

    if let Some(prompt) = config.system_prompt {
        agent
            .extend_system_prompt(config.session_id.clone(), prompt)
            .await;
    }

    Ok(agent)
}
