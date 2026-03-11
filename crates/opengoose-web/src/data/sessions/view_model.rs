use std::collections::BTreeSet;

use opengoose_profiles::{AgentProfile, ProfileStore};
use opengoose_teams::TeamStore;
use opengoose_types::SessionKey;
use urlencoding::encode;

use super::loader::SessionRecord;
use crate::data::utils::{platform_tone, preview};
use crate::data::views::{
    MessageBubble, MetaRow, SelectOption, SessionDetailView, SessionListItem,
};

pub(in crate::data) fn build_session_list_items(
    sessions: &[SessionRecord],
    selected_key: Option<String>,
    source_label: &str,
) -> Vec<SessionListItem> {
    sessions
        .iter()
        .map(|session| {
            let parsed = SessionKey::from_stable_id(&session.summary.session_key);
            let title = match &parsed.namespace {
                Some(namespace) => format!("{namespace} / {}", parsed.channel_id),
                None => parsed.channel_id.clone(),
            };
            let subtitle = session
                .summary
                .active_team
                .clone()
                .map(|team| format!("{team} active · {source_label}"))
                .unwrap_or_else(|| format!("No active team · {source_label}"));
            let preview = session
                .messages
                .last()
                .map(|message| preview(&message.content, 84))
                .unwrap_or_else(|| "No messages captured yet.".into());
            let encoded = encode(&session.summary.session_key);

            SessionListItem {
                title,
                subtitle,
                preview,
                updated_at: session.summary.updated_at.clone(),
                badge: parsed.platform.as_str().to_uppercase(),
                badge_tone: platform_tone(parsed.platform.as_str()),
                page_url: format!("/sessions?session={encoded}"),
                active: selected_key
                    .as_ref()
                    .map(|key| key == &session.summary.session_key)
                    .unwrap_or(false),
            }
        })
        .collect()
}

pub(in crate::data) fn build_session_detail(
    session: &SessionRecord,
    source_label: &str,
) -> SessionDetailView {
    let parsed = SessionKey::from_stable_id(&session.summary.session_key);
    let selected_model = session.summary.selected_model.clone().unwrap_or_default();

    SessionDetailView {
        session_key: session.summary.session_key.clone(),
        title: format!("Session {}", parsed.channel_id),
        subtitle: match &parsed.namespace {
            Some(namespace) => format!("{} / {}", parsed.platform.as_str(), namespace),
            None => format!("{} / direct", parsed.platform.as_str()),
        },
        source_label: source_label.into(),
        meta: vec![
            MetaRow {
                label: "Stable key".into(),
                value: session.summary.session_key.clone(),
            },
            MetaRow {
                label: "Active team".into(),
                value: session
                    .summary
                    .active_team
                    .clone()
                    .unwrap_or_else(|| "None".into()),
            },
            MetaRow {
                label: "Selected model".into(),
                value: session
                    .summary
                    .selected_model
                    .clone()
                    .unwrap_or_else(|| "Profile default".into()),
            },
            MetaRow {
                label: "Created".into(),
                value: session.summary.created_at.clone(),
            },
            MetaRow {
                label: "Last update".into(),
                value: session.summary.updated_at.clone(),
            },
        ],
        notice: None,
        selected_model,
        model_options: session_model_options(
            session.summary.active_team.as_deref(),
            session.summary.selected_model.as_deref(),
        ),
        messages: session
            .messages
            .iter()
            .map(|message| MessageBubble {
                role_label: if message.role == "assistant" {
                    "Assistant".into()
                } else {
                    "User".into()
                },
                author_label: message.author.clone().unwrap_or_else(|| "unknown".into()),
                timestamp: message.created_at.clone(),
                content: message.content.clone(),
                tone: if message.role == "assistant" {
                    "accent"
                } else {
                    "plain"
                },
                alignment: if message.role == "assistant" {
                    "right"
                } else {
                    "left"
                },
            })
            .collect(),
        empty_hint: "This session has no persisted messages yet.".into(),
    }
}

fn session_model_options(
    active_team: Option<&str>,
    selected_model: Option<&str>,
) -> Vec<SelectOption> {
    let mut seen = BTreeSet::new();
    let mut options = Vec::new();

    let mut push_option = |value: &str, label: String| {
        if value.trim().is_empty() || !seen.insert(value.to_string()) {
            return;
        }

        options.push(SelectOption {
            value: value.to_string(),
            label,
            selected: selected_model == Some(value),
        });
    };

    if let Some(selected_model) = selected_model {
        push_option(
            selected_model,
            format!("{selected_model} (current override)"),
        );
    }

    if let Ok(profile_store) = ProfileStore::new() {
        if let Ok(profile) = profile_store.get("main") {
            append_profile_model_options(&profile, &mut push_option);
        }

        if let Some(active_team) = active_team
            && let Ok(team_store) = TeamStore::new()
            && let Ok(team) = team_store.get(active_team)
        {
            for team_agent in team.agents {
                if let Ok(profile) = profile_store.get(&team_agent.profile) {
                    append_profile_model_options(&profile, &mut push_option);
                }
            }
        }
    }

    options
}

fn append_profile_model_options(
    profile: &AgentProfile,
    push_option: &mut impl FnMut(&str, String),
) {
    let Some(settings) = profile.settings.as_ref() else {
        return;
    };

    if let Some(model) = settings.goose_model.as_deref() {
        let provider = settings
            .goose_provider
            .as_deref()
            .unwrap_or("profile default");
        push_option(model, format!("{model} ({provider})"));
    }

    for fallback in &settings.provider_fallbacks {
        if let Some(model) = fallback.goose_model.as_deref() {
            push_option(
                model,
                format!("{model} (fallback via {})", fallback.goose_provider),
            );
        }
    }
}
