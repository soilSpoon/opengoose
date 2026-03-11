#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use diesel::prelude::*;
    use opengoose_types::{Platform, SessionKey};

    use crate::SessionStore;
    use crate::db::Database;
    use crate::session_store::export::{
        render_batch_session_exports_markdown, render_session_export_markdown,
    };
    use crate::session_store::types::{HistoryMessage, SessionExport, SessionExportQuery};

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

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
}
