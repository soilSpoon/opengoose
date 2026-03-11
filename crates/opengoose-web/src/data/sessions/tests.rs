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

#[test]
fn build_session_list_items_active_flag() {
    let sessions = vec![
        sample_session("discord:ns:chan-a", Some("team-1")),
        sample_session("telegram:direct:user-1", None),
    ];
    let items = build_session_list_items(&sessions, Some("telegram:direct:user-1".into()), "Live");
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
    let labels: Vec<_> = detail.meta.iter().map(|row| row.label.as_str()).collect();
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
        .find(|row| row.label == "Active team")
        .unwrap();
    assert_eq!(team_row.value, "None");
}

#[test]
fn build_session_detail_export_actions_encode_session_key() {
    let session = sample_session("discord:ns:studio-a:ops-bridge", None);
    let detail = build_session_detail(&session, "Mock");
    assert_eq!(detail.export_actions.len(), 2);
    assert!(
        detail.export_actions[0]
            .href
            .contains("discord%3Ans%3Astudio-a%3Aops-bridge")
    );
    assert!(detail.export_actions[1].href.ends_with("format=md"));
}

#[test]
fn build_batch_export_form_defaults_to_json_limit_and_hint() {
    let form = build_batch_export_form();
    assert_eq!(form.action_url, "/api/sessions/export");
    assert_eq!(form.limit, 100);
    assert_eq!(
        form.format_options
            .iter()
            .find(|option| option.selected)
            .map(|option| option.value.as_str()),
        Some("json")
    );
    assert!(form.hint.contains("since or until"));
}

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

#[test]
fn find_selected_session_requires_existing_key() {
    let sessions = vec![sample_session("discord:ns:chan-a", None)];
    let error = find_selected_session(&sessions, "missing").unwrap_err();
    assert!(error.to_string().contains("selected session missing"));
}
