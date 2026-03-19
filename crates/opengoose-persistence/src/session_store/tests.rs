#![cfg(test)]

use diesel::prelude::*;
use opengoose_types::{Platform, SessionKey};

use crate::SessionStore;
use crate::session_store::export::{
    render_batch_session_exports_markdown, render_session_export_markdown,
};
use crate::session_store::types::{HistoryMessage, SessionExport, SessionExportQuery};
use crate::test_helpers::test_db;

fn test_key() -> SessionKey {
    SessionKey::new(Platform::Discord, "guild123".to_string(), "channel456")
}

#[test]
fn test_append_and_load_history() {
    let store = SessionStore::new(test_db());
    let key = test_key();

    store
        .append_user_message(&key, "hello", Some("alice"))
        .unwrap();
    store.append_assistant_message(&key, "hi there").unwrap();
    store
        .append_user_message(&key, "how are you?", Some("alice"))
        .unwrap();

    let history = store.load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].role, "user");
    assert_eq!(history[0].content, "hello");
    assert_eq!(history[1].role, "assistant");
    assert_eq!(history[1].content, "hi there");
    assert_eq!(history[2].role, "user");
    assert_eq!(history[2].content, "how are you?");
}

#[test]
fn test_load_history_limit() {
    let store = SessionStore::new(test_db());
    let key = test_key();

    for i in 0..10 {
        store
            .append_user_message(&key, &format!("msg {i}"), None)
            .unwrap();
    }

    let history = store.load_history(&key, 3).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].content, "msg 7");
    assert_eq!(history[2].content, "msg 9");
}

#[test]
fn test_load_history_for_stable_id_preserves_order_when_timestamps_tie() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let key = test_key();
    let stable_id = key.to_stable_id();

    store
        .append_user_message(&key, "msg 1", Some("alice"))
        .unwrap();
    store.append_assistant_message(&key, "msg 2").unwrap();
    store
        .append_user_message(&key, "msg 3", Some("alice"))
        .unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE messages
             SET created_at = '2026-03-11 12:00:00'
             WHERE session_key = 'discord:ns:guild123:channel456'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let history = store
        .load_history_for_stable_id(&stable_id, Some(2))
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].content, "msg 2");
    assert_eq!(history[1].content, "msg 3");
}

#[test]
fn test_active_team() {
    let store = SessionStore::new(test_db());
    let key = test_key();

    assert_eq!(store.get_active_team(&key).unwrap(), None);

    store.set_active_team(&key, Some("code-review")).unwrap();
    assert_eq!(
        store.get_active_team(&key).unwrap(),
        Some("code-review".into())
    );

    store.set_active_team(&key, None).unwrap();
    assert_eq!(store.get_active_team(&key).unwrap(), None);
}

#[test]
fn test_load_all_active_teams() {
    let store = SessionStore::new(test_db());

    let key1 = SessionKey::new(Platform::Discord, "g1".to_string(), "c1");
    let key2 = SessionKey::new(Platform::Discord, "g2".to_string(), "c2");
    let key3 = SessionKey::new(Platform::Discord, "g3".to_string(), "c3");

    store.set_active_team(&key1, Some("team-a")).unwrap();
    store.set_active_team(&key2, Some("team-b")).unwrap();
    store.set_active_team(&key3, None).unwrap();

    let teams = store.load_all_active_teams().unwrap();
    assert_eq!(teams.len(), 2);
    assert_eq!(teams.get(&key1).unwrap(), "team-a");
    assert_eq!(teams.get(&key2).unwrap(), "team-b");
}

#[test]
fn test_cleanup() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let key = test_key();

    store.append_user_message(&key, "old msg", None).unwrap();

    db.with(|conn| {
        diesel::sql_query("UPDATE sessions SET updated_at = datetime('now', '-100 hours')")
            .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let deleted = store.cleanup(72).unwrap();
    assert_eq!(deleted, 1);

    let history = store.load_history(&key, 10).unwrap();
    assert!(history.is_empty());
}

#[test]
fn test_cleanup_expired_messages() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let key = test_key();

    store.append_user_message(&key, "old msg", None).unwrap();
    store.append_user_message(&key, "recent msg", None).unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE messages
             SET created_at = datetime('now', '-10 days')
             WHERE content = 'old msg'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let deleted = store.cleanup_expired_messages(7).unwrap();
    assert_eq!(deleted, 1);

    let history = store.load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content, "recent msg");
}

#[test]
fn test_load_history_nonexistent_session() {
    let store = SessionStore::new(test_db());
    let key = SessionKey::new(Platform::Telegram, "unknown_guild", "unknown_chan");
    let history = store.load_history(&key, 10).unwrap();
    assert!(history.is_empty());
}

#[test]
fn test_load_history_preserves_author() {
    let store = SessionStore::new(test_db());
    let key = test_key();

    store
        .append_user_message(&key, "hello", Some("alice"))
        .unwrap();
    store.append_user_message(&key, "no author", None).unwrap();
    store.append_assistant_message(&key, "reply").unwrap();

    let history = store.load_history(&key, 10).unwrap();
    assert_eq!(history[0].author.as_deref(), Some("alice"));
    assert_eq!(history[1].author, None);
    assert_eq!(history[2].author.as_deref(), Some("goose"));
}

#[test]
fn test_export_session_includes_metadata_and_all_messages() {
    let store = SessionStore::new(test_db());
    let key = test_key();

    store
        .append_user_message(&key, "hello", Some("alice"))
        .unwrap();
    store.append_assistant_message(&key, "world").unwrap();
    store.set_active_team(&key, Some("feature-dev")).unwrap();

    let export = store
        .export_session(&key)
        .unwrap()
        .expect("session export should exist");

    assert_eq!(export.session_key, "discord:ns:guild123:channel456");
    assert_eq!(export.active_team.as_deref(), Some("feature-dev"));
    assert_eq!(export.message_count, 2);
    assert_eq!(export.messages[0].content, "hello");
    assert_eq!(export.messages[1].content, "world");
}

#[test]
fn test_export_session_returns_none_for_missing_session() {
    let store = SessionStore::new(test_db());
    let missing = SessionKey::new(Platform::Slack, "missing".to_string(), "session");

    let export = store.export_session(&missing).unwrap();

    assert!(export.is_none());
}

#[test]
fn test_export_session_includes_zero_message_sessions() {
    let store = SessionStore::new(test_db());
    let key = SessionKey::new(Platform::Slack, "workspace".to_string(), "empty");

    store.set_active_team(&key, Some("triage")).unwrap();

    let export = store
        .export_session(&key)
        .unwrap()
        .expect("session export should exist");

    assert_eq!(export.session_key, key.to_stable_id());
    assert_eq!(export.active_team.as_deref(), Some("triage"));
    assert_eq!(export.message_count, 0);
    assert!(export.messages.is_empty());
}

#[test]
fn test_export_sessions_filters_by_updated_at_range() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let older_key = SessionKey::new(Platform::Discord, "guild-a".to_string(), "alpha");
    let newer_key = SessionKey::new(Platform::Discord, "guild-b".to_string(), "beta");

    store
        .append_user_message(&older_key, "older", Some("alice"))
        .unwrap();
    store
        .append_user_message(&newer_key, "newer", Some("bob"))
        .unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-09 08:00:00'
             WHERE session_key = 'discord:ns:guild-a:alpha'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-11 09:00:00'
             WHERE session_key = 'discord:ns:guild-b:beta'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let exports = store
        .export_sessions(&SessionExportQuery {
            since: Some("2026-03-10 00:00:00".into()),
            until: Some("2026-03-11 23:59:59".into()),
            limit: 10,
        })
        .unwrap();

    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].session_key, "discord:ns:guild-b:beta");
}

#[test]
fn test_export_sessions_includes_exact_window_boundaries_and_empty_sessions() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let before_key = SessionKey::new(Platform::Discord, "guild-a".to_string(), "before");
    let since_key = SessionKey::new(Platform::Discord, "guild-b".to_string(), "since");
    let until_key = SessionKey::new(Platform::Slack, "workspace".to_string(), "until");
    let after_key = SessionKey::new(Platform::Discord, "guild-c".to_string(), "after");

    store
        .append_user_message(&before_key, "before", Some("alice"))
        .unwrap();
    store
        .append_user_message(&since_key, "since", Some("bob"))
        .unwrap();
    store.set_active_team(&until_key, Some("triage")).unwrap();
    store
        .append_user_message(&after_key, "after", Some("carol"))
        .unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-09 23:59:59'
             WHERE session_key = 'discord:ns:guild-a:before'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-10 00:00:00'
             WHERE session_key = 'discord:ns:guild-b:since'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-11 23:59:59'
             WHERE session_key = 'slack:ns:workspace:until'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-12 00:00:00'
             WHERE session_key = 'discord:ns:guild-c:after'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let exports = store
        .export_sessions(&SessionExportQuery {
            since: Some("2026-03-10 00:00:00".into()),
            until: Some("2026-03-11 23:59:59".into()),
            limit: 10,
        })
        .unwrap();

    assert_eq!(exports.len(), 2);
    assert_eq!(exports[0].session_key, until_key.to_stable_id());
    assert_eq!(exports[0].active_team.as_deref(), Some("triage"));
    assert_eq!(exports[0].message_count, 0);
    assert!(exports[0].messages.is_empty());
    assert_eq!(exports[1].session_key, since_key.to_stable_id());
    assert_eq!(exports[1].message_count, 1);
    assert_eq!(exports[1].messages[0].content, "since");
}

#[test]
fn test_export_sessions_respects_limit_and_descending_updated_at_order() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let oldest_key = SessionKey::new(Platform::Discord, "guild-a".to_string(), "alpha");
    let middle_key = SessionKey::new(Platform::Discord, "guild-b".to_string(), "beta");
    let newest_key = SessionKey::new(Platform::Discord, "guild-c".to_string(), "gamma");

    store
        .append_user_message(&oldest_key, "oldest", Some("alice"))
        .unwrap();
    store
        .append_user_message(&middle_key, "middle", Some("bob"))
        .unwrap();
    store
        .append_user_message(&newest_key, "newest", Some("carol"))
        .unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-09 08:00:00'
             WHERE session_key = 'discord:ns:guild-a:alpha'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-10 08:00:00'
             WHERE session_key = 'discord:ns:guild-b:beta'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET updated_at = '2026-03-11 08:00:00'
             WHERE session_key = 'discord:ns:guild-c:gamma'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let exports = store
        .export_sessions(&SessionExportQuery {
            since: None,
            until: None,
            limit: 2,
        })
        .unwrap();

    assert_eq!(exports.len(), 2);
    assert_eq!(exports[0].session_key, newest_key.to_stable_id());
    assert_eq!(exports[1].session_key, middle_key.to_stable_id());
}

#[test]
fn test_render_session_export_markdown_contains_metadata_and_message_content() {
    let markdown = render_session_export_markdown(&SessionExport {
        session_key: "discord:ns:guild123:channel456".into(),
        active_team: Some("feature-dev".into()),
        created_at: "2026-03-11 09:00:00".into(),
        updated_at: "2026-03-11 09:05:00".into(),
        message_count: 1,
        messages: vec![HistoryMessage {
            role: "user".into(),
            content: "hello markdown".into(),
            author: Some("alice".into()),
            created_at: "2026-03-11 09:01:00".into(),
        }],
    });

    assert!(markdown.contains("# OpenGoose Session Export"));
    assert!(markdown.contains("feature-dev"));
    assert!(markdown.contains("hello markdown"));
}

#[test]
fn test_render_batch_session_exports_markdown_reports_empty_range() {
    let markdown = render_batch_session_exports_markdown(
        &[],
        Some("2026-03-10 00:00:00"),
        Some("2026-03-11 00:00:00"),
    );

    assert!(markdown.contains("# OpenGoose Session Batch Export"));
    assert!(markdown.contains("No sessions matched"));
}

#[test]
fn test_set_active_team_upserts_session() {
    let store = SessionStore::new(test_db());
    let key = SessionKey::new(Platform::Slack, "ws1", "ch1");
    store.set_active_team(&key, Some("my-team")).unwrap();
    assert_eq!(store.get_active_team(&key).unwrap(), Some("my-team".into()));
}

#[test]
fn test_stats_include_active_sessions_duration_and_tokens() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let active_key = SessionKey::new(Platform::Discord, "guild-a".to_string(), "chan-a");
    let idle_key = SessionKey::new(Platform::Discord, "guild-b".to_string(), "chan-b");

    store
        .append_user_message(&active_key, "12345678", Some("alice"))
        .unwrap();
    store.append_assistant_message(&active_key, "1234").unwrap();
    store
        .append_user_message(&idle_key, "123456789012", Some("bob"))
        .unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-90 seconds'),
                 updated_at = datetime('now')
             WHERE session_key = 'discord:ns:guild-a:chan-a'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-2 hours', '-30 seconds'),
                 updated_at = datetime('now', '-2 hours')
             WHERE session_key = 'discord:ns:guild-b:chan-b'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 2);
    assert_eq!(stats.message_count, 3);
    assert_eq!(stats.estimated_token_count, 6);
    assert_eq!(stats.active_session_count, 1);
    assert!((stats.average_session_duration_seconds - 60.0).abs() < 1.0);
}

#[test]
fn test_list_session_metrics_returns_recent_breakdown() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let active_key = SessionKey::new(Platform::Slack, "workspace".to_string(), "alpha");
    let idle_key = SessionKey::new(Platform::Slack, "workspace".to_string(), "beta");

    store
        .append_user_message(&active_key, "abcdefgh", Some("alice"))
        .unwrap();
    store
        .append_assistant_message(&active_key, "ijklmnop")
        .unwrap();
    store.set_active_team(&active_key, Some("ops")).unwrap();
    store
        .append_user_message(&idle_key, "abcdefghij", Some("bob"))
        .unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-120 seconds'),
                 updated_at = datetime('now')
             WHERE session_key = 'slack:ns:workspace:alpha'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-3 hours', '-45 seconds'),
                 updated_at = datetime('now', '-3 hours')
             WHERE session_key = 'slack:ns:workspace:beta'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let metrics = store.list_session_metrics(10).unwrap();
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].session_key, "slack:ns:workspace:alpha");
    assert_eq!(metrics[0].message_count, 2);
    assert_eq!(metrics[0].estimated_token_count, 4);
    assert_eq!(metrics[0].duration_seconds, 120);
    assert!(metrics[0].active);
    assert_eq!(metrics[0].active_team.as_deref(), Some("ops"));

    assert_eq!(metrics[1].session_key, "slack:ns:workspace:beta");
    assert_eq!(metrics[1].message_count, 1);
    assert_eq!(metrics[1].estimated_token_count, 3);
    assert_eq!(metrics[1].duration_seconds, 45);
    assert!(!metrics[1].active);
}

#[test]
fn test_list_session_metrics_clamps_negative_duration_for_empty_sessions() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let key = SessionKey::new(Platform::Slack, "workspace".to_string(), "empty");

    store.set_active_team(&key, Some("ops")).unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-1 hour'),
                 updated_at = datetime('now', '-2 hours')
             WHERE session_key = 'slack:ns:workspace:empty'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let metric = store
        .list_session_metrics(10)
        .unwrap()
        .into_iter()
        .find(|item| item.session_key == key.to_stable_id())
        .expect("metric row should exist");

    assert_eq!(metric.active_team.as_deref(), Some("ops"));
    assert_eq!(metric.message_count, 0);
    assert_eq!(metric.estimated_token_count, 0);
    assert_eq!(metric.duration_seconds, 0);
    assert!(!metric.active);
}

#[test]
fn test_list_sessions() {
    let store = SessionStore::new(test_db());
    let key1 = SessionKey::new(Platform::Discord, "g1", "c1");
    let key2 = SessionKey::new(Platform::Slack, "ws1", "c2");

    store.append_user_message(&key1, "msg1", None).unwrap();
    store.append_user_message(&key2, "msg2", None).unwrap();

    let sessions = store.list_sessions(10).unwrap();
    assert_eq!(sessions.len(), 2);
    // Both sessions should be present (order may vary within the same second)
    let keys: Vec<&str> = sessions.iter().map(|s| s.session_key.as_str()).collect();
    assert!(keys.contains(&key1.to_stable_id().as_str()));
    assert!(keys.contains(&key2.to_stable_id().as_str()));
}

#[test]
fn test_list_sessions_orders_by_updated_at_and_preserves_metadata() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let older_key = SessionKey::new(Platform::Discord, "guild-a".to_string(), "alpha");
    let newer_key = SessionKey::new(Platform::Slack, "workspace".to_string(), "beta");

    store.set_active_team(&older_key, Some("ops")).unwrap();
    store.set_selected_model(&older_key, Some("gpt-5")).unwrap();
    store
        .append_user_message(&newer_key, "recent", None)
        .unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-4 hours'),
                 updated_at = datetime('now', '-3 hours')
             WHERE session_key = 'discord:ns:guild-a:alpha'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-2 hours'),
                 updated_at = datetime('now', '-10 minutes')
             WHERE session_key = 'slack:ns:workspace:beta'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let sessions = store.list_sessions(10).unwrap();
    assert_eq!(sessions[0].session_key, newer_key.to_stable_id());
    assert_eq!(sessions[1].session_key, older_key.to_stable_id());
    assert_eq!(sessions[1].active_team.as_deref(), Some("ops"));
    assert_eq!(sessions[1].selected_model.as_deref(), Some("gpt-5"));
}

#[test]
fn test_list_sessions_limit() {
    let store = SessionStore::new(test_db());
    for i in 0..5 {
        let key = SessionKey::new(Platform::Discord, format!("g{i}"), format!("c{i}"));
        store.append_user_message(&key, "msg", None).unwrap();
    }
    let sessions = store.list_sessions(2).unwrap();
    assert_eq!(sessions.len(), 2);
}

#[test]
fn test_stats() {
    let store = SessionStore::new(test_db());
    let key = test_key();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.message_count, 0);

    store.append_user_message(&key, "hello", None).unwrap();
    store.append_assistant_message(&key, "world").unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.message_count, 2);
}

#[test]
fn test_stats_include_zero_message_sessions() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let key = SessionKey::new(Platform::Slack, "workspace".to_string(), "empty");

    store.set_active_team(&key, Some("ops")).unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-5 minutes'),
                 updated_at = datetime('now', '-1 minute')
             WHERE session_key = 'slack:ns:workspace:empty'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.message_count, 0);
    assert_eq!(stats.estimated_token_count, 0);
    assert_eq!(stats.active_session_count, 1);
    assert!((stats.average_session_duration_seconds - 240.0).abs() < 1.0);
}

#[test]
fn test_stats_near_active_session_boundary() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let inside_window_key =
        SessionKey::new(Platform::Slack, "workspace".to_string(), "inside-window");
    let outside_window_key =
        SessionKey::new(Platform::Slack, "workspace".to_string(), "outside-window");

    store
        .set_active_team(&inside_window_key, Some("ops"))
        .unwrap();
    store
        .set_active_team(&outside_window_key, Some("ops"))
        .unwrap();

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-2 hours'),
                 updated_at = datetime('now', '-30 minutes', '+1 second')
             WHERE session_key = 'slack:ns:workspace:inside-window'",
        )
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE sessions
             SET created_at = datetime('now', '-2 hours'),
                 updated_at = datetime('now', '-30 minutes', '-1 second')
             WHERE session_key = 'slack:ns:workspace:outside-window'",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 2);
    assert_eq!(stats.message_count, 0);
    assert_eq!(stats.estimated_token_count, 0);
    assert_eq!(stats.active_session_count, 1);
}

// ── Edge case tests ────────────────────────────────────────────────

#[test]
fn load_history_with_zero_limit_returns_empty() {
    let store = SessionStore::new(test_db());
    let key = test_key();
    store.append_user_message(&key, "msg", None).unwrap();

    let history = store.load_history(&key, 0).unwrap();
    assert!(history.is_empty());
}

#[test]
fn get_selected_model_returns_none_for_nonexistent_session() {
    let store = SessionStore::new(test_db());
    let key = SessionKey::new(Platform::Custom("matrix".into()), "homeserver", "room-404");

    assert_eq!(store.get_selected_model(&key).unwrap(), None);
}

#[test]
fn set_and_get_selected_model_roundtrip() {
    let store = SessionStore::new(test_db());
    let key = test_key();

    store.set_selected_model(&key, Some("gpt-5")).unwrap();
    assert_eq!(
        store.get_selected_model(&key).unwrap(),
        Some("gpt-5".into())
    );

    store
        .set_selected_model(&key, Some("claude-sonnet-4-20250514"))
        .unwrap();
    assert_eq!(
        store.get_selected_model(&key).unwrap(),
        Some("claude-sonnet-4-20250514".into())
    );

    store.set_selected_model(&key, None).unwrap();
    assert_eq!(store.get_selected_model(&key).unwrap(), None);
}

#[test]
fn cleanup_with_no_expired_sessions_returns_zero() {
    let store = SessionStore::new(test_db());
    let key = test_key();
    store.append_user_message(&key, "fresh msg", None).unwrap();

    let deleted = store.cleanup(9999).unwrap();
    assert_eq!(deleted, 0);

    let history = store.load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 1);
}

#[test]
fn cleanup_expired_messages_with_zero_retention_deletes_all() {
    let db = test_db();
    let store = SessionStore::new(db.clone());
    let key = test_key();
    store.append_user_message(&key, "msg1", None).unwrap();
    store.append_user_message(&key, "msg2", None).unwrap();

    // Backdate all messages
    db.with(|conn| {
        diesel::sql_query("UPDATE messages SET created_at = datetime('now', '-1 day')")
            .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let deleted = store.cleanup_expired_messages(0).unwrap();
    assert_eq!(deleted, 2);
    assert!(store.load_history(&key, 10).unwrap().is_empty());
}

#[test]
fn export_sessions_with_no_filters_returns_all() {
    let store = SessionStore::new(test_db());
    let key1 = SessionKey::new(Platform::Discord, "g1", "c1");
    let key2 = SessionKey::new(Platform::Slack, "ws1", "c2");
    store.append_user_message(&key1, "msg1", None).unwrap();
    store.append_user_message(&key2, "msg2", None).unwrap();

    let exports = store
        .export_sessions(&SessionExportQuery {
            since: None,
            until: None,
            limit: 100,
        })
        .unwrap();

    assert_eq!(exports.len(), 2);
    assert!(exports.iter().all(|e| e.message_count == 1));
}

#[test]
fn list_sessions_on_empty_db_returns_empty() {
    let store = SessionStore::new(test_db());
    assert!(store.list_sessions(10).unwrap().is_empty());
}

#[test]
fn stats_on_empty_db_returns_zeroes() {
    let store = SessionStore::new(test_db());
    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.message_count, 0);
    assert_eq!(stats.estimated_token_count, 0);
    assert_eq!(stats.active_session_count, 0);
    assert_eq!(stats.average_session_duration_seconds, 0.0);
}

#[test]
fn list_session_metrics_on_empty_db_returns_empty() {
    let store = SessionStore::new(test_db());
    assert!(store.list_session_metrics(10).unwrap().is_empty());
}

#[test]
fn load_all_active_teams_on_empty_db_returns_empty() {
    let store = SessionStore::new(test_db());
    assert!(store.load_all_active_teams().unwrap().is_empty());
}

#[test]
fn selected_model_preserved_in_list_sessions() {
    let store = SessionStore::new(test_db());
    let key = test_key();
    store.append_user_message(&key, "msg", None).unwrap();
    store.set_selected_model(&key, Some("gpt-5")).unwrap();

    let sessions = store.list_sessions(10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].selected_model.as_deref(), Some("gpt-5"));
}

#[test]
fn test_render_session_export_markdown_no_active_team() {
    // active_team: None should render as "- Active team: -"
    let markdown = render_session_export_markdown(&SessionExport {
        session_key: "discord:ns:g:c".into(),
        active_team: None,
        created_at: "2026-03-19 10:00:00".into(),
        updated_at: "2026-03-19 10:01:00".into(),
        message_count: 0,
        messages: vec![],
    });

    assert!(markdown.contains("- Active team: -"), "expected '- Active team: -', got:\n{markdown}");
}

#[test]
fn test_render_session_export_markdown_message_with_no_author() {
    // author: None should produce "role · created_at" without an author segment
    let markdown = render_session_export_markdown(&SessionExport {
        session_key: "discord:ns:g:c".into(),
        active_team: None,
        created_at: "2026-03-19 10:00:00".into(),
        updated_at: "2026-03-19 10:01:00".into(),
        message_count: 1,
        messages: vec![HistoryMessage {
            role: "user".into(),
            content: "anonymous message".into(),
            author: None,
            created_at: "2026-03-19 10:00:30".into(),
        }],
    });

    assert!(markdown.contains("anonymous message"));
    // heading should be "user · 2026-03-19 10:00:30" — no author segment in between
    assert!(
        markdown.contains("user · 2026-03-19 10:00:30"),
        "expected 'user · <timestamp>' heading without author, got:\n{markdown}"
    );
}

#[test]
fn test_render_batch_session_exports_markdown_with_sessions_and_no_time_range() {
    // Non-empty batch, None since/until: verifies the session-loop path and that
    // missing time-range fields are simply omitted from the output.
    let exports = vec![
        SessionExport {
            session_key: "discord:ns:g:ch1".into(),
            active_team: Some("alpha".into()),
            created_at: "2026-03-19 09:00:00".into(),
            updated_at: "2026-03-19 09:05:00".into(),
            message_count: 1,
            messages: vec![HistoryMessage {
                role: "user".into(),
                content: "first session message".into(),
                author: Some("alice".into()),
                created_at: "2026-03-19 09:01:00".into(),
            }],
        },
        SessionExport {
            session_key: "discord:ns:g:ch2".into(),
            active_team: None,
            created_at: "2026-03-19 08:00:00".into(),
            updated_at: "2026-03-19 08:30:00".into(),
            message_count: 0,
            messages: vec![],
        },
    ];

    let markdown = render_batch_session_exports_markdown(&exports, None, None);

    assert!(markdown.contains("# OpenGoose Session Batch Export"));
    assert!(markdown.contains("- Sessions: 2"));
    // time-range lines must be absent when since/until are None
    assert!(!markdown.contains("- Since:"), "unexpected Since: line");
    assert!(!markdown.contains("- Until:"), "unexpected Until: line");
    // both session keys appear as headings
    assert!(markdown.contains("discord:ns:g:ch1"));
    assert!(markdown.contains("discord:ns:g:ch2"));
    // message content from the first session appears
    assert!(markdown.contains("first session message"));
}
