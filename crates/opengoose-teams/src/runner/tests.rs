use super::output::parse_agent_output;
use super::types::{
    AgentEventSummary, AgentOutput, AttemptFailure, FALLBACK_MODEL, FALLBACK_PROVIDER,
    ProviderTarget, resolve_provider_chain,
};
use opengoose_profiles::{AgentProfile, ProfileSettings, ProviderFallback};

#[test]
fn test_parse_broadcast() {
    let output = parse_agent_output(
        "Here's my analysis.\n[BROADCAST]: Found critical auth bug in line 42\nMore details here.",
    );
    assert_eq!(output.broadcasts.len(), 1);
    assert_eq!(output.broadcasts[0], "Found critical auth bug in line 42");
    assert_eq!(output.response, "Here's my analysis.\nMore details here.");
}

#[test]
fn test_parse_mention_colon() {
    let output = parse_agent_output("@reviewer: please check the auth module");
    assert_eq!(output.delegations.len(), 1);
    assert_eq!(output.delegations[0].0, "reviewer");
    assert_eq!(output.delegations[0].1, "please check the auth module");
    assert!(output.response.is_empty());
}

#[test]
fn test_parse_mention_space() {
    let output = parse_agent_output("@coder fix the bug in auth.rs");
    assert_eq!(output.delegations.len(), 1);
    assert_eq!(output.delegations[0].0, "coder");
    assert_eq!(output.delegations[0].1, "fix the bug in auth.rs");
}

#[test]
fn test_mixed_output() {
    let raw = "Starting analysis.\n\
               [BROADCAST]: database schema looks outdated\n\
               @coder: update the migration files\n\
               Here's the summary.\n\
               [BROADCAST]: tests are all passing";
    let output = parse_agent_output(raw);
    assert_eq!(output.broadcasts.len(), 2);
    assert_eq!(output.delegations.len(), 1);
    assert_eq!(output.response, "Starting analysis.\nHere's the summary.");
}

#[test]
fn test_no_special_output() {
    let output = parse_agent_output("Just a normal response with no special tags.");
    assert!(output.broadcasts.is_empty());
    assert!(output.delegations.is_empty());
    assert_eq!(
        output.response,
        "Just a normal response with no special tags."
    );
}

#[test]
fn test_parse_mention_at_only() {
    // "@" alone should not be parsed as a mention
    let output = parse_agent_output("@");
    assert!(output.delegations.is_empty());
    assert_eq!(output.response, "@");
}

#[test]
fn test_parse_mention_at_with_spaces() {
    // "@agent name with spaces: msg" — agent name has spaces, should not match colon form
    let output = parse_agent_output("@agent name with spaces: some message");
    // Falls through to space-based parsing: agent="agent", msg="name with spaces: some message"
    assert_eq!(output.delegations.len(), 1);
    assert_eq!(output.delegations[0].0, "agent");
}

#[test]
fn test_parse_mention_no_message() {
    // "@coder" alone (no message) should not be a delegation
    let output = parse_agent_output("@coder");
    assert!(output.delegations.is_empty());
    assert_eq!(output.response, "@coder");
}

#[test]
fn test_parse_mention_colon_empty_message() {
    // "@coder: " (empty after colon) — should not be parsed as delegation
    let output = parse_agent_output("@coder:");
    // colon form: agent="coder", msg="" → msg is empty → falls through to space form
    // space form: no space → returns None
    assert!(output.delegations.is_empty());
}

#[test]
fn test_parse_broadcast_whitespace() {
    let output = parse_agent_output("[BROADCAST]:    extra spaces   ");
    assert_eq!(output.broadcasts.len(), 1);
    assert_eq!(output.broadcasts[0], "extra spaces");
}

#[test]
fn test_parse_empty_input() {
    let output = parse_agent_output("");
    assert!(output.broadcasts.is_empty());
    assert!(output.delegations.is_empty());
    assert_eq!(output.response, "");
}

#[test]
fn test_parse_only_whitespace_lines() {
    let output = parse_agent_output("  \n  \n  ");
    assert!(output.broadcasts.is_empty());
    assert!(output.delegations.is_empty());
}

#[test]
fn test_multiple_delegations() {
    let raw = "@coder: fix the bug\n@reviewer: check the fix\n@tester run the tests";
    let output = parse_agent_output(raw);
    assert_eq!(output.delegations.len(), 3);
    assert_eq!(
        output.delegations[0],
        ("coder".into(), "fix the bug".into())
    );
    assert_eq!(
        output.delegations[1],
        ("reviewer".into(), "check the fix".into())
    );
    assert_eq!(
        output.delegations[2],
        ("tester".into(), "run the tests".into())
    );
    assert!(output.response.is_empty());
}

#[test]
fn test_agent_event_summary_default() {
    let summary = AgentEventSummary::default();
    assert!(summary.model_changes.is_empty());
    assert_eq!(summary.context_compactions, 0);
    assert!(summary.extension_notifications.is_empty());
}

#[test]
fn test_agent_output_profile_name() {
    // Verify AgentOutput fields are Debug-printable
    let output = AgentOutput {
        response: "hello".into(),
        delegations: vec![("a".into(), "b".into())],
        broadcasts: vec!["msg".into()],
    };
    let debug = format!("{:?}", output);
    assert!(debug.contains("hello"));
    assert!(debug.contains("msg"));
}

// ─── resolve_provider_chain tests ───────────────────────────────────

fn make_profile(settings: Option<ProfileSettings>) -> AgentProfile {
    AgentProfile {
        version: "1.0.0".to_string(),
        title: "test-agent".to_string(),
        description: None,
        instructions: Some("test".to_string()),
        prompt: None,
        extensions: vec![],
        skills: vec![],
        settings,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    }
}

#[test]
fn test_resolve_chain_no_settings_uses_fallback() {
    let profile = make_profile(None);
    let chain = resolve_provider_chain(&profile);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].provider_name, FALLBACK_PROVIDER);
    assert_eq!(chain[0].model_name, FALLBACK_MODEL);
}

#[test]
fn test_resolve_chain_with_explicit_provider() {
    let profile = make_profile(Some(ProfileSettings {
        goose_provider: Some("openai".to_string()),
        goose_model: Some("gpt-4.1".to_string()),
        ..Default::default()
    }));
    let chain = resolve_provider_chain(&profile);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].provider_name, "openai");
    assert_eq!(chain[0].model_name, "gpt-4.1");
}

#[test]
fn test_resolve_chain_with_fallbacks() {
    let profile = make_profile(Some(ProfileSettings {
        goose_provider: Some("anthropic".to_string()),
        goose_model: Some("claude-sonnet-4-6".to_string()),
        provider_fallbacks: vec![
            ProviderFallback {
                goose_provider: "openai".to_string(),
                goose_model: Some("gpt-4.1".to_string()),
            },
            ProviderFallback {
                goose_provider: "xai".to_string(),
                goose_model: None,
            },
        ],
        ..Default::default()
    }));
    let chain = resolve_provider_chain(&profile);
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].provider_name, "anthropic");
    assert_eq!(chain[0].model_name, "claude-sonnet-4-6");
    assert_eq!(chain[1].provider_name, "openai");
    assert_eq!(chain[1].model_name, "gpt-4.1");
    assert_eq!(chain[2].provider_name, "xai");
    // When fallback omits model, inherits the primary model
    assert_eq!(chain[2].model_name, "claude-sonnet-4-6");
}

#[test]
fn test_resolve_chain_deduplicates_fallbacks() {
    let profile = make_profile(Some(ProfileSettings {
        goose_provider: Some("anthropic".to_string()),
        goose_model: Some("claude-sonnet-4-6".to_string()),
        provider_fallbacks: vec![
            // Duplicate of primary — should be skipped
            ProviderFallback {
                goose_provider: "anthropic".to_string(),
                goose_model: Some("claude-sonnet-4-6".to_string()),
            },
            ProviderFallback {
                goose_provider: "openai".to_string(),
                goose_model: Some("gpt-4.1".to_string()),
            },
        ],
        ..Default::default()
    }));
    let chain = resolve_provider_chain(&profile);
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].provider_name, "anthropic");
    assert_eq!(chain[1].provider_name, "openai");
}

#[test]
fn test_resolve_chain_skips_empty_provider_names() {
    let profile = make_profile(Some(ProfileSettings {
        goose_provider: Some("anthropic".to_string()),
        goose_model: Some("claude-sonnet-4-6".to_string()),
        provider_fallbacks: vec![
            ProviderFallback {
                goose_provider: "".to_string(),
                goose_model: Some("gpt-4.1".to_string()),
            },
            ProviderFallback {
                goose_provider: "   ".to_string(),
                goose_model: None,
            },
            ProviderFallback {
                goose_provider: "openai".to_string(),
                goose_model: Some("gpt-4.1".to_string()),
            },
        ],
        ..Default::default()
    }));
    let chain = resolve_provider_chain(&profile);
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].provider_name, "anthropic");
    assert_eq!(chain[1].provider_name, "openai");
}

#[test]
fn test_resolve_chain_provider_only_no_model() {
    let profile = make_profile(Some(ProfileSettings {
        goose_provider: Some("openai".to_string()),
        goose_model: None,
        ..Default::default()
    }));
    let chain = resolve_provider_chain(&profile);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].provider_name, "openai");
    // Model falls back to GOOSE_MODEL env or FALLBACK_MODEL
}

#[test]
fn test_resolve_chain_same_provider_different_model_not_deduped() {
    let profile = make_profile(Some(ProfileSettings {
        goose_provider: Some("anthropic".to_string()),
        goose_model: Some("claude-sonnet-4-6".to_string()),
        provider_fallbacks: vec![ProviderFallback {
            goose_provider: "anthropic".to_string(),
            goose_model: Some("claude-haiku-4-5".to_string()),
        }],
        ..Default::default()
    }));
    let chain = resolve_provider_chain(&profile);
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].model_name, "claude-sonnet-4-6");
    assert_eq!(chain[1].model_name, "claude-haiku-4-5");
}

#[test]
fn test_resolve_chain_empty_fallback_list() {
    let profile = make_profile(Some(ProfileSettings {
        goose_provider: Some("anthropic".to_string()),
        goose_model: Some("claude-sonnet-4-6".to_string()),
        provider_fallbacks: vec![],
        ..Default::default()
    }));
    let chain = resolve_provider_chain(&profile);
    assert_eq!(chain.len(), 1);
}

// ─── AttemptFailure tests ───────────────────────────────────────────

#[test]
fn test_attempt_failure_no_emitted_content() {
    let failure = AttemptFailure::new(anyhow::anyhow!("connection refused"), false);
    assert!(!failure.emitted_content);
    assert_eq!(failure.error.to_string(), "connection refused");
}

#[test]
fn test_attempt_failure_with_emitted_content() {
    let failure = AttemptFailure::new(anyhow::anyhow!("stream interrupted"), true);
    assert!(failure.emitted_content);
    assert_eq!(failure.error.to_string(), "stream interrupted");
}

// ─── ProviderTarget tests ───────────────────────────────────────────

#[test]
fn test_provider_target_clone() {
    let target = ProviderTarget {
        provider_name: "anthropic".to_string(),
        model_name: "claude-sonnet-4-6".to_string(),
    };
    let cloned = target.clone();
    assert_eq!(cloned.provider_name, "anthropic");
    assert_eq!(cloned.model_name, "claude-sonnet-4-6");
}

#[test]
fn test_provider_target_debug() {
    let target = ProviderTarget {
        provider_name: "openai".to_string(),
        model_name: "gpt-4.1".to_string(),
    };
    let debug = format!("{:?}", target);
    assert!(debug.contains("openai"));
    assert!(debug.contains("gpt-4.1"));
}

// ─── AgentEventSummary tests ────────────────────────────────────────

#[test]
fn test_event_summary_accumulates_model_changes() {
    let mut summary = AgentEventSummary::default();
    summary
        .model_changes
        .push(("claude-opus-4-6".into(), "auto".into()));
    summary
        .model_changes
        .push(("claude-sonnet-4-6".into(), "fast".into()));
    assert_eq!(summary.model_changes.len(), 2);
    assert_eq!(summary.model_changes[0].0, "claude-opus-4-6");
    assert_eq!(summary.model_changes[1].1, "fast");
}

#[test]
fn test_event_summary_accumulates_compactions() {
    let mut summary = AgentEventSummary::default();
    summary.context_compactions += 1;
    summary.context_compactions += 1;
    assert_eq!(summary.context_compactions, 2);
}

#[test]
fn test_event_summary_accumulates_notifications() {
    let mut summary = AgentEventSummary::default();
    summary.extension_notifications.push("code-analyzer".into());
    summary.extension_notifications.push("web-search".into());
    assert_eq!(summary.extension_notifications.len(), 2);
    assert_eq!(summary.extension_notifications[0], "code-analyzer");
}

// ─── Fallback constants tests ───────────────────────────────────────

#[test]
fn test_fallback_constants_are_valid() {
    assert!(!FALLBACK_PROVIDER.is_empty());
    assert!(!FALLBACK_MODEL.is_empty());
    assert_eq!(FALLBACK_PROVIDER, "anthropic");
    assert_eq!(FALLBACK_MODEL, "claude-sonnet-4-6");
}

// ─── AgentRunner construction tests ─────────────────────────────────

use super::AgentRunner;
use opengoose_profiles::ExtensionRef;
use opengoose_projects::ProjectContext;
use std::path::PathBuf;

#[tokio::test]
async fn test_from_inline_prompt_sets_profile_name() {
    let runner = AgentRunner::from_inline_prompt("You are a test bot.", "test-bot")
        .await
        .unwrap();
    assert_eq!(runner.profile_name(), "test-bot");
}

#[tokio::test]
async fn test_from_inline_prompt_default_max_turns() {
    let runner = AgentRunner::from_inline_prompt("You are helpful.", "helper")
        .await
        .unwrap();
    assert_eq!(runner.max_turns, 10);
}

#[tokio::test]
async fn test_from_inline_prompt_default_retry_config_is_none() {
    let runner = AgentRunner::from_inline_prompt("You are helpful.", "helper")
        .await
        .unwrap();
    assert!(runner.retry_config.is_none());
}

#[tokio::test]
async fn test_from_inline_prompt_cwd_is_current_dir() {
    let runner = AgentRunner::from_inline_prompt("prompt", "agent")
        .await
        .unwrap();
    let expected = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    assert_eq!(runner.cwd(), expected);
}

#[tokio::test]
async fn test_from_inline_prompt_session_id_is_nonempty() {
    let runner = AgentRunner::from_inline_prompt("prompt", "agent")
        .await
        .unwrap();
    assert!(
        !runner.session_id().is_empty(),
        "session_id should be non-empty"
    );
}

#[tokio::test]
async fn test_from_inline_prompt_produces_unique_sessions() {
    let r1 = AgentRunner::from_inline_prompt("prompt", "agent")
        .await
        .unwrap();
    let r2 = AgentRunner::from_inline_prompt("prompt", "agent")
        .await
        .unwrap();
    assert_ne!(
        r1.session_id(),
        r2.session_id(),
        "each from_inline_prompt call should get a unique session"
    );
}

#[tokio::test]
async fn test_from_profile_keyed_returns_valid_session() {
    let profile = make_profile(None);
    let runner = AgentRunner::from_profile_keyed(&profile, "my-stable-session".to_string())
        .await
        .unwrap();
    assert!(!runner.session_id().is_empty());
    assert_eq!(runner.profile_name(), "test-agent");
}

#[tokio::test]
async fn test_from_profile_keyed_with_project_uses_project_cwd() {
    let profile = make_profile(None);
    let project = ProjectContext {
        title: "test-project".to_string(),
        goal: "ship it".to_string(),
        cwd: PathBuf::from("/tmp/test-project-dir"),
        context_entries: vec![],
        default_team: None,
    };
    let runner =
        AgentRunner::from_profile_keyed_with_project(&profile, "sess".to_string(), Some(&project))
            .await
            .unwrap();
    assert_eq!(runner.cwd(), PathBuf::from("/tmp/test-project-dir"));
}

#[tokio::test]
async fn test_from_profile_keyed_without_project_uses_process_cwd() {
    let profile = make_profile(None);
    let runner =
        AgentRunner::from_profile_keyed_with_project(&profile, "sess2".to_string(), None)
            .await
            .unwrap();
    let expected = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    assert_eq!(runner.cwd(), expected);
}

#[tokio::test]
async fn test_instructions_takes_precedence_over_prompt() {
    // When both `instructions` and `prompt` are set, `instructions` wins.
    let profile = AgentProfile {
        version: "1.0.0".to_string(),
        title: "precedence-test".to_string(),
        description: None,
        instructions: Some("Use these instructions.".to_string()),
        prompt: Some("Use this prompt.".to_string()),
        extensions: vec![],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    };
    let runner = AgentRunner::from_profile(&profile).await.unwrap();
    assert_eq!(runner.profile_name(), "precedence-test");
}

#[tokio::test]
async fn test_prompt_field_used_when_no_instructions() {
    let profile = AgentProfile {
        version: "1.0.0".to_string(),
        title: "prompt-only".to_string(),
        description: None,
        instructions: None,
        prompt: Some("Use this prompt.".to_string()),
        extensions: vec![],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    };
    let runner = AgentRunner::from_profile(&profile).await.unwrap();
    assert_eq!(runner.profile_name(), "prompt-only");
}

#[tokio::test]
async fn test_custom_max_turns_from_settings() {
    let profile = make_profile(Some(ProfileSettings {
        max_turns: Some(25),
        ..Default::default()
    }));
    let runner = AgentRunner::from_profile(&profile).await.unwrap();
    assert_eq!(runner.max_turns, 25);
}

#[tokio::test]
async fn test_retry_config_from_settings() {
    let profile = make_profile(Some(ProfileSettings {
        max_retries: Some(3),
        retry_checks: vec!["cargo test".to_string()],
        on_failure: Some("cargo clean".to_string()),
        ..Default::default()
    }));
    let runner = AgentRunner::from_profile(&profile).await.unwrap();
    let rc = runner.retry_config.as_ref().expect("should have retry config");
    assert_eq!(rc.max_retries, 3);
    assert_eq!(rc.checks.len(), 1);
    assert_eq!(rc.on_failure.as_deref(), Some("cargo clean"));
}

#[tokio::test]
async fn test_retry_config_none_without_max_retries() {
    let profile = make_profile(Some(ProfileSettings {
        retry_checks: vec!["cargo test".to_string()],
        ..Default::default()
    }));
    let runner = AgentRunner::from_profile(&profile).await.unwrap();
    assert!(runner.retry_config.is_none());
}

#[tokio::test]
async fn test_provider_chain_plumbed_from_profile() {
    let profile = make_profile(Some(ProfileSettings {
        goose_provider: Some("openai".to_string()),
        goose_model: Some("gpt-4.1".to_string()),
        provider_fallbacks: vec![ProviderFallback {
            goose_provider: "anthropic".to_string(),
            goose_model: Some("claude-sonnet-4-6".to_string()),
        }],
        ..Default::default()
    }));
    let runner = AgentRunner::from_profile(&profile).await.unwrap();
    assert_eq!(runner.provider_chain.len(), 2);
    assert_eq!(runner.provider_chain[0].provider_name, "openai");
    assert_eq!(runner.provider_chain[1].provider_name, "anthropic");
}

#[tokio::test]
async fn test_unsupported_extension_skipped_gracefully() {
    let profile = AgentProfile {
        version: "1.0.0".to_string(),
        title: "ext-test".to_string(),
        description: None,
        instructions: Some("test".to_string()),
        prompt: None,
        extensions: vec![ExtensionRef {
            name: "unsupported-ext".to_string(),
            ext_type: "nonexistent_type".to_string(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: Default::default(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        }],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    };
    let runner = AgentRunner::from_profile(&profile).await.unwrap();
    assert_eq!(runner.profile_name(), "ext-test");
}

#[tokio::test]
async fn test_project_context_with_empty_goal() {
    let profile = make_profile(None);
    let project = ProjectContext {
        title: "empty-goal".to_string(),
        goal: String::new(),
        cwd: PathBuf::from("/tmp"),
        context_entries: vec![],
        default_team: None,
    };
    let runner =
        AgentRunner::from_profile_keyed_with_project(&profile, "s1".to_string(), Some(&project))
            .await
            .unwrap();
    assert_eq!(runner.cwd(), PathBuf::from("/tmp"));
}

#[tokio::test]
async fn test_no_instructions_no_prompt_still_constructs() {
    // Neither instructions nor prompt — falls through to workspace identity path.
    let profile = AgentProfile {
        version: "1.0.0".to_string(),
        title: "bare-agent".to_string(),
        description: None,
        instructions: None,
        prompt: None,
        extensions: vec![],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    };
    let runner = AgentRunner::from_profile(&profile).await.unwrap();
    assert_eq!(runner.profile_name(), "bare-agent");
}
