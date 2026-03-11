use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use opengoose_persistence::{Database, HistoryMessage, SessionStore, SessionSummary};
use opengoose_profiles::ProfileStore;
use opengoose_teams::TeamStore;
use opengoose_types::SessionKey;
use urlencoding::encode;

use crate::data::utils::{platform_tone, preview};
use crate::data::views::{
    MessageBubble, MetaRow, SelectOption, SessionDetailView, SessionListItem, SessionsPageView,
};

#[derive(Clone)]
pub(super) struct SessionRecord {
    pub(super) summary: SessionSummary,
    pub(super) messages: Vec<HistoryMessage>,
}

/// Load the sessions page view-model, optionally selecting a session by key.
pub fn load_sessions_page(db: Arc<Database>, selected: Option<String>) -> Result<SessionsPageView> {
    let store = SessionStore::new(db);
    let session_rows = store.list_sessions(24)?;
    let using_mock = session_rows.is_empty();

    let sessions = if using_mock {
        mock_sessions()
    } else {
        live_sessions(&store, &session_rows)?
    };
    let selected_key = choose_selected_session(&sessions, selected);

    Ok(SessionsPageView {
        mode_label: if using_mock {
            "Mock preview".into()
        } else {
            "Live runtime".into()
        },
        mode_tone: if using_mock { "neutral" } else { "success" },
        live_stream_url: format!("/sessions/events?session={}", encode(&selected_key)),
        sessions: build_session_list_items(
            &sessions,
            Some(selected_key.clone()),
            if using_mock {
                "Mock preview"
            } else {
                "Live runtime"
            },
        ),
        selected: build_session_detail(
            sessions
                .iter()
                .find(|session| session.summary.session_key == selected_key)
                .context("selected session missing")?,
            if using_mock {
                "Mock preview"
            } else {
                "Live runtime"
            },
        ),
    })
}

pub(super) fn live_sessions(
    store: &SessionStore,
    rows: &[SessionSummary],
) -> Result<Vec<SessionRecord>> {
    rows.iter()
        .map(|summary| {
            let key = SessionKey::from_stable_id(&summary.session_key);
            Ok(SessionRecord {
                summary: summary.clone(),
                messages: store.load_history(&key, 40)?,
            })
        })
        .collect()
}

pub(super) fn mock_sessions() -> Vec<SessionRecord> {
    vec![
        SessionRecord {
            summary: SessionSummary {
                session_key: "discord:ns:studio-a:ops-bridge".into(),
                active_team: Some("feature-dev".into()),
                selected_model: Some("claude-sonnet-4-20250514".into()),
                created_at: "2026-03-10 09:00".into(),
                updated_at: "2026-03-10 10:28".into(),
            },
            messages: vec![
                HistoryMessage {
                    role: "user".into(),
                    content: "Spin up a reviewer and confirm the deploy checklist.".into(),
                    author: Some("pm-sora".into()),
                    created_at: "2026-03-10 10:11".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content:
                        "Feature-dev is active. Routing implementation notes to reviewer next."
                            .into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 10:12".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content:
                        "Reviewer flagged one missing migration note. Queue updated for follow-up."
                            .into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 10:28".into(),
                },
            ],
        },
        SessionRecord {
            summary: SessionSummary {
                session_key: "telegram:direct:founder-42".into(),
                active_team: None,
                selected_model: None,
                created_at: "2026-03-10 08:21".into(),
                updated_at: "2026-03-10 09:44".into(),
            },
            messages: vec![
                HistoryMessage {
                    role: "user".into(),
                    content: "Summarize the backlog movement from this morning.".into(),
                    author: Some("founder".into()),
                    created_at: "2026-03-10 09:40".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content:
                        "Three frontend issues advanced to implementation, one queue alert remains unresolved."
                            .into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 09:44".into(),
                },
            ],
        },
    ]
}

pub(super) fn build_session_list_items(
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
                .map(|team| format!("{} active · {}", team, source_label))
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

pub(super) fn build_session_detail(
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
    profile: &opengoose_profiles::AgentProfile,
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use opengoose_persistence::{HistoryMessage, SessionSummary};

    use super::*;

    fn sample_session(session_key: &str, active_team: Option<&str>) -> SessionRecord {
        SessionRecord {
            summary: SessionSummary {
                session_key: session_key.into(),
                active_team: active_team.map(str::to_string),
                selected_model: None,
                created_at: "2026-03-10 09:00".into(),
                updated_at: "2026-03-10 10:00".into(),
            },
            messages: vec![HistoryMessage {
                role: "user".into(),
                content: "Hello world".into(),
                author: Some("tester".into()),
                created_at: "2026-03-10 10:00".into(),
            }],
        }
    }

    // --- mock_sessions ---

    #[test]
    fn mock_sessions_returns_two_sessions() {
        let sessions = mock_sessions();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn mock_sessions_first_has_active_team() {
        let sessions = mock_sessions();
        assert!(sessions[0].summary.active_team.is_some());
    }

    #[test]
    fn mock_sessions_second_has_no_active_team() {
        let sessions = mock_sessions();
        assert!(sessions[1].summary.active_team.is_none());
    }

    #[test]
    fn build_session_detail_surfaces_selected_model() {
        let mut session = sample_session("discord:ns:chan-1", Some("feature-dev"));
        session.summary.selected_model = Some("gpt-5-mini".into());

        let detail = build_session_detail(&session, "Live");

        assert_eq!(detail.selected_model, "gpt-5-mini");
        assert!(
            detail
                .meta
                .iter()
                .any(|row| row.label == "Selected model" && row.value == "gpt-5-mini")
        );
    }

    // --- build_session_list_items ---

    #[test]
    fn build_session_list_items_active_flag() {
        let sessions = vec![
            sample_session("discord:ns:chan-a", Some("team-1")),
            sample_session("telegram:direct:user-1", None),
        ];
        let items =
            build_session_list_items(&sessions, Some("telegram:direct:user-1".into()), "Live");
        assert!(!items[0].active);
        assert!(items[1].active);
    }

    #[test]
    fn build_session_list_items_none_selection_all_inactive() {
        let sessions = vec![sample_session("discord:ns:chan-a", None)];
        let items = build_session_list_items(&sessions, None, "Mock");
        assert!(!items[0].active);
    }

    #[test]
    fn build_session_list_items_discord_badge_uppercase() {
        let sessions = vec![sample_session("discord:ns:chan-a", None)];
        let items = build_session_list_items(&sessions, None, "Mock");
        assert_eq!(items[0].badge, "DISCORD");
        assert_eq!(items[0].badge_tone, "cyan");
    }

    #[test]
    fn build_session_list_items_telegram_badge_tone() {
        let sessions = vec![sample_session("telegram:direct:user-1", None)];
        let items = build_session_list_items(&sessions, None, "Mock");
        assert_eq!(items[0].badge, "TELEGRAM");
        assert_eq!(items[0].badge_tone, "sage");
    }

    #[test]
    fn build_session_list_items_slack_badge_tone() {
        let sessions = vec![sample_session("slack:ns:workspace:general", None)];
        let items = build_session_list_items(&sessions, None, "Mock");
        assert_eq!(items[0].badge_tone, "amber");
    }

    #[test]
    fn build_session_list_items_with_active_team_subtitle() {
        let sessions = vec![sample_session("discord:ns:chan-a", Some("feature-dev"))];
        let items = build_session_list_items(&sessions, None, "Live runtime");
        assert!(items[0].subtitle.contains("feature-dev"));
        assert!(items[0].subtitle.contains("Live runtime"));
    }

    #[test]
    fn build_session_list_items_no_active_team_subtitle() {
        let sessions = vec![sample_session("discord:ns:chan-a", None)];
        let items = build_session_list_items(&sessions, None, "Live runtime");
        assert!(items[0].subtitle.contains("No active team"));
    }

    #[test]
    fn build_session_list_items_preview_from_last_message() {
        let sessions = vec![sample_session("discord:ns:chan-a", None)];
        let items = build_session_list_items(&sessions, None, "Mock");
        assert_eq!(items[0].preview, "Hello world");
    }

    #[test]
    fn build_session_list_items_preview_fallback_no_messages() {
        let mut session = sample_session("discord:ns:chan-a", None);
        session.messages.clear();
        let items = build_session_list_items(&[session], None, "Mock");
        assert!(items[0].preview.contains("No messages"));
    }

    #[test]
    fn build_session_list_items_page_url_contains_session_key() {
        let sessions = vec![sample_session("discord:ns:chan-a", None)];
        let items = build_session_list_items(&sessions, None, "Mock");
        assert!(items[0].page_url.starts_with("/sessions?session="));
    }

    // --- build_session_detail ---

    #[test]
    fn build_session_detail_title_contains_channel_id() {
        let session = sample_session("discord:ns:studio-a:ops-bridge", Some("team-1"));
        let detail = build_session_detail(&session, "Mock preview");
        assert!(detail.title.contains("ops-bridge"));
    }

    #[test]
    fn build_session_detail_with_namespace_subtitle() {
        let session = sample_session("discord:ns:studio-a:ops-bridge", None);
        let detail = build_session_detail(&session, "Mock");
        assert!(detail.subtitle.contains("discord"));
        assert!(detail.subtitle.contains("studio-a"));
    }

    #[test]
    fn build_session_detail_without_namespace_subtitle_contains_direct() {
        let session = sample_session("telegram:direct:user-1", None);
        let detail = build_session_detail(&session, "Live");
        assert!(detail.subtitle.contains("telegram"));
        assert!(detail.subtitle.contains("direct"));
    }

    #[test]
    fn build_session_detail_meta_includes_key_and_team() {
        let session = sample_session("discord:ns:chan-a", Some("feature-dev"));
        let detail = build_session_detail(&session, "Mock");
        let labels: Vec<_> = detail.meta.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"Stable key"));
        assert!(labels.contains(&"Active team"));
    }

    #[test]
    fn build_session_detail_message_bubble_user_role() {
        let session = sample_session("discord:ns:chan-a", None);
        let detail = build_session_detail(&session, "Mock");
        assert_eq!(detail.messages.len(), 1);
        assert_eq!(detail.messages[0].role_label, "User");
        assert_eq!(detail.messages[0].tone, "plain");
        assert_eq!(detail.messages[0].alignment, "left");
    }

    #[test]
    fn build_session_detail_message_bubble_assistant_role() {
        let mut session = sample_session("discord:ns:chan-a", None);
        session.messages.push(HistoryMessage {
            role: "assistant".into(),
            content: "I can help".into(),
            author: Some("goose".into()),
            created_at: "2026-03-10 10:01".into(),
        });
        let detail = build_session_detail(&session, "Mock");
        let assistant_bubble = &detail.messages[1];
        assert_eq!(assistant_bubble.role_label, "Assistant");
        assert_eq!(assistant_bubble.tone, "accent");
        assert_eq!(assistant_bubble.alignment, "right");
    }

    #[test]
    fn build_session_detail_author_fallback_when_none() {
        let mut session = sample_session("discord:ns:chan-a", None);
        session.messages[0].author = None;
        let detail = build_session_detail(&session, "Mock");
        assert_eq!(detail.messages[0].author_label, "unknown");
    }

    #[test]
    fn build_session_detail_active_team_none_shows_none() {
        let session = sample_session("discord:ns:chan-a", None);
        let detail = build_session_detail(&session, "Mock");
        let team_row = detail
            .meta
            .iter()
            .find(|r| r.label == "Active team")
            .unwrap();
        assert_eq!(team_row.value, "None");
    }

    // --- choose_selected_session ---

    #[test]
    fn choose_selected_session_returns_match() {
        let sessions = vec![
            sample_session("discord:ns:chan-a", Some("team-1")),
            sample_session("telegram:direct:user-1", None),
        ];
        let key = choose_selected_session(&sessions, Some("telegram:direct:user-1".into()));
        assert_eq!(key, "telegram:direct:user-1");
    }

    #[test]
    fn choose_selected_session_falls_back_to_first() {
        let sessions = vec![sample_session("discord:ns:chan-a", Some("team-1"))];
        let key = choose_selected_session(&sessions, Some("does-not-exist".into()));
        assert_eq!(key, "discord:ns:chan-a");
    }

    #[test]
    fn choose_selected_session_none_falls_back_to_first() {
        let sessions = vec![sample_session("discord:ns:chan-a", None)];
        let key = choose_selected_session(&sessions, None);
        assert_eq!(key, "discord:ns:chan-a");
    }
}

fn choose_selected_session(sessions: &[SessionRecord], selected: Option<String>) -> String {
    selected
        .filter(|target| {
            sessions
                .iter()
                .any(|session| session.summary.session_key == *target)
        })
        .unwrap_or_else(|| sessions[0].summary.session_key.clone())
}
