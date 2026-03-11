//! End-to-end integration tests exercising the CLI → Core → Persistence
//! workflow for sessions, messages, and conversation history.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;

use opengoose_persistence::{AgentMessageStore, Database, SessionStore};
use opengoose_types::{Platform, SessionKey};
use tempfile::TempDir;

fn test_env() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let goose_root = temp.path().join("goose");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&goose_root).unwrap();
    (temp, home, goose_root)
}

fn run_cli(home: &Path, goose_root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opengoose"))
        .args(args)
        .env("HOME", home)
        .env("GOOSE_PATH_ROOT", goose_root)
        .env("GOOSE_DISABLE_KEYRING", "1")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn open_db(home: &Path) -> Arc<Database> {
    let db_path = home.join(".opengoose").join("sessions.db");
    Arc::new(Database::open_at(db_path).unwrap())
}

// ── Session creation via message send ───────────────────────────────────────

#[test]
fn directed_message_creates_session_and_persists() {
    let (_temp, home, goose_root) = test_env();

    let send = run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "frontend", "--to", "backend",
            "deploy request",
        ],
    );
    assert!(send.status.success(), "stderr: {}", stderr(&send));
    assert!(stdout(&send).contains("Directed message sent"));

    let db = open_db(&home);
    let store = AgentMessageStore::new(db);
    let msgs = store.list_recent("cli:local:default", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].from_agent, "frontend");
    assert_eq!(msgs[0].to_agent.as_deref(), Some("backend"));
    assert_eq!(msgs[0].payload, "deploy request");
}

#[test]
fn channel_message_creates_session_and_persists() {
    let (_temp, home, goose_root) = test_env();

    let send = run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "ops-bot", "--channel", "alerts",
            "disk usage 90%",
        ],
    );
    assert!(send.status.success(), "stderr: {}", stderr(&send));
    assert!(stdout(&send).contains("Channel message published"));

    let db = open_db(&home);
    let store = AgentMessageStore::new(db);
    let history = store
        .channel_history("cli:local:default", "alerts", None)
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].from_agent, "ops-bot");
    assert_eq!(history[0].channel.as_deref(), Some("alerts"));
    assert_eq!(history[0].payload, "disk usage 90%");
}

// ── Message listing via CLI ─────────────────────────────────────────────────

#[test]
fn message_list_shows_all_recent_messages() {
    let (_temp, home, goose_root) = test_env();

    for (from, to, payload) in [
        ("alice", "bob", "hello bob"),
        ("bob", "alice", "hello alice"),
        ("alice", "bob", "how are you"),
    ] {
        let send = run_cli(
            &home,
            &goose_root,
            &["message", "send", "--from", from, "--to", to, payload],
        );
        assert!(send.status.success(), "stderr: {}", stderr(&send));
    }

    let list = run_cli(&home, &goose_root, &["message", "list"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("alice"));
    assert!(list_stdout.contains("bob"));
    assert!(list_stdout.contains("hello bob"));
    assert!(list_stdout.contains("hello alice"));
    assert!(list_stdout.contains("how are you"));
    assert!(list_stdout.contains("3 message(s)"));
}

#[test]
fn message_list_filters_by_agent() {
    let (_temp, home, goose_root) = test_env();

    run_cli(
        &home,
        &goose_root,
        &["message", "send", "--from", "alice", "--to", "bob", "msg1"],
    );
    run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "charlie", "--to", "dave", "msg2",
        ],
    );

    let list = run_cli(
        &home,
        &goose_root,
        &["message", "list", "--agent", "bob"],
    );
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("msg1"));
    assert!(!list_stdout.contains("msg2"));
}

#[test]
fn message_list_filters_by_channel() {
    let (_temp, home, goose_root) = test_env();

    run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "bot", "--channel", "ops",
            "ops alert",
        ],
    );
    run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "bot", "--channel", "dev",
            "dev update",
        ],
    );

    let list = run_cli(
        &home,
        &goose_root,
        &["message", "list", "--channel", "ops"],
    );
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("ops alert"));
    assert!(!list_stdout.contains("dev update"));
}

// ── Pending message workflow ────────────────────────────────────────────────

#[test]
fn pending_messages_workflow_end_to_end() {
    let (_temp, home, goose_root) = test_env();

    run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "frontend", "--to", "backend",
            "task 1",
        ],
    );
    run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "monitor", "--to", "backend",
            "task 2",
        ],
    );

    let pending = run_cli(&home, &goose_root, &["message", "pending", "backend"]);
    assert!(pending.status.success());
    let pending_stdout = stdout(&pending);
    assert!(pending_stdout.contains("Pending messages for 'backend':"));
    assert!(pending_stdout.contains("frontend"));
    assert!(pending_stdout.contains("task 1"));
    assert!(pending_stdout.contains("monitor"));
    assert!(pending_stdout.contains("task 2"));
    assert!(pending_stdout.contains("2 pending message(s)"));

    let no_pending = run_cli(&home, &goose_root, &["message", "pending", "frontend"]);
    assert!(no_pending.status.success());
    assert!(stdout(&no_pending).contains("No pending messages for 'frontend'."));
}

// ── Session persistence via SessionStore ────────────────────────────────────

#[test]
fn session_store_conversation_round_trip() {
    let (_temp, home, _goose_root) = test_env();

    let db = open_db(&home);
    let store = SessionStore::new(db);
    let key = SessionKey::new(Platform::Discord, "guild-1", "channel-1");

    store
        .append_user_message(&key, "What is Rust?", Some("alice"))
        .unwrap();
    store
        .append_assistant_message(&key, "Rust is a systems programming language.")
        .unwrap();
    store
        .append_user_message(&key, "Tell me more", Some("alice"))
        .unwrap();
    store
        .append_assistant_message(&key, "Rust focuses on safety and performance.")
        .unwrap();

    let history = store.load_history(&key, 100).unwrap();
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].role, "user");
    assert_eq!(history[0].content, "What is Rust?");
    assert_eq!(history[0].author.as_deref(), Some("alice"));
    assert_eq!(history[1].role, "assistant");
    assert_eq!(history[1].content, "Rust is a systems programming language.");
    assert_eq!(history[1].author.as_deref(), Some("goose"));
    assert_eq!(history[2].role, "user");
    assert_eq!(history[3].role, "assistant");

    let sessions = store.list_sessions(10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_key, key.to_stable_id());
}

#[test]
fn session_store_history_limit_returns_most_recent() {
    let (_temp, home, _goose_root) = test_env();

    let db = open_db(&home);
    let store = SessionStore::new(db);
    let key = SessionKey::new(Platform::Telegram, "group-1", "chat-1");

    for i in 0..20 {
        store
            .append_user_message(&key, &format!("message {i}"), Some("user"))
            .unwrap();
    }

    let history = store.load_history(&key, 5).unwrap();
    assert_eq!(history.len(), 5);
    assert_eq!(history[0].content, "message 15");
    assert_eq!(history[4].content, "message 19");
}

#[test]
fn session_store_multiple_sessions_isolated() {
    let (_temp, home, _goose_root) = test_env();

    let db = open_db(&home);
    let store = SessionStore::new(db);

    let key_a = SessionKey::new(Platform::Discord, "guild-a", "ch-a");
    let key_b = SessionKey::new(Platform::Slack, "ws-b", "ch-b");

    store
        .append_user_message(&key_a, "discord msg", Some("alice"))
        .unwrap();
    store
        .append_user_message(&key_b, "slack msg", Some("bob"))
        .unwrap();

    let history_a = store.load_history(&key_a, 10).unwrap();
    assert_eq!(history_a.len(), 1);
    assert_eq!(history_a[0].content, "discord msg");

    let history_b = store.load_history(&key_b, 10).unwrap();
    assert_eq!(history_b.len(), 1);
    assert_eq!(history_b[0].content, "slack msg");

    let sessions = store.list_sessions(10).unwrap();
    assert_eq!(sessions.len(), 2);
}

#[test]
fn session_store_stats_reflect_all_activity() {
    let (_temp, home, _goose_root) = test_env();

    let db = open_db(&home);
    let store = SessionStore::new(db);

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.message_count, 0);

    let key1 = SessionKey::new(Platform::Discord, "g1", "c1");
    let key2 = SessionKey::new(Platform::Discord, "g2", "c2");

    store.append_user_message(&key1, "msg1", None).unwrap();
    store.append_assistant_message(&key1, "reply1").unwrap();
    store.append_user_message(&key2, "msg2", None).unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 2);
    assert_eq!(stats.message_count, 3);
}

// ── Active team management ──────────────────────────────────────────────────

#[test]
fn session_store_active_team_lifecycle() {
    let (_temp, home, _goose_root) = test_env();

    let db = open_db(&home);
    let store = SessionStore::new(db);
    let key = SessionKey::new(Platform::Discord, "guild-1", "channel-1");

    assert_eq!(store.get_active_team(&key).unwrap(), None);

    store.set_active_team(&key, Some("code-review")).unwrap();
    assert_eq!(
        store.get_active_team(&key).unwrap(),
        Some("code-review".into())
    );

    store
        .append_user_message(&key, "review this PR", Some("dev"))
        .unwrap();
    store
        .append_assistant_message(&key, "I'll review it now.")
        .unwrap();

    let history = store.load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 2);

    store.set_active_team(&key, None).unwrap();
    assert_eq!(store.get_active_team(&key).unwrap(), None);

    let history = store.load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 2);
}

// ── CLI + persistence combined workflow ─────────────────────────────────────

#[test]
fn full_cli_message_and_persistence_workflow() {
    let (_temp, home, goose_root) = test_env();

    // 1. Send directed messages via CLI
    let send1 = run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "frontend", "--to", "backend",
            "please process order #42",
        ],
    );
    assert!(send1.status.success());

    let send2 = run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "backend", "--to", "frontend",
            "order #42 processed",
        ],
    );
    assert!(send2.status.success());

    // 2. Send channel message via CLI
    let send3 = run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "backend", "--channel", "status",
            "order #42 complete",
        ],
    );
    assert!(send3.status.success());

    // 3. List all messages via CLI
    let list = run_cli(&home, &goose_root, &["message", "list"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("3 message(s)"));

    // 4. Check pending for frontend (should have 1 from backend)
    let pending = run_cli(&home, &goose_root, &["message", "pending", "frontend"]);
    assert!(pending.status.success());
    let pending_stdout = stdout(&pending);
    assert!(pending_stdout.contains("order #42 processed"));

    // 5. Verify directly via persistence layer
    let db = open_db(&home);
    let agent_store = AgentMessageStore::new(db.clone());
    let all_msgs = agent_store.list_recent("cli:local:default", 10).unwrap();
    assert_eq!(all_msgs.len(), 3);

    // 6. Write conversation history directly to persistence
    let session_store = SessionStore::new(db);
    let key = SessionKey::new(Platform::Custom("cli".into()), "local", "default");

    session_store
        .append_user_message(&key, "user input", Some("operator"))
        .unwrap();
    session_store
        .append_assistant_message(&key, "goose response")
        .unwrap();

    let history = session_store.load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].content, "user input");
    assert_eq!(history[1].content, "goose response");

    // 7. Verify stats
    let stats = session_store.stats().unwrap();
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.message_count, 2);
}

// ── Custom session key via CLI ──────────────────────────────────────────────

#[test]
fn custom_session_key_isolates_messages() {
    let (_temp, home, goose_root) = test_env();

    run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "a", "--to", "b",
            "--session", "session-alpha", "alpha msg",
        ],
    );
    run_cli(
        &home,
        &goose_root,
        &[
            "message", "send", "--from", "a", "--to", "b",
            "--session", "session-beta", "beta msg",
        ],
    );

    let list_default = run_cli(&home, &goose_root, &["message", "list"]);
    assert!(stdout(&list_default).contains("No messages found."));

    let list_a = run_cli(
        &home,
        &goose_root,
        &["message", "list", "--session", "session-alpha"],
    );
    let list_a_stdout = stdout(&list_a);
    assert!(list_a_stdout.contains("alpha msg"));
    assert!(!list_a_stdout.contains("beta msg"));

    let list_b = run_cli(
        &home,
        &goose_root,
        &["message", "list", "--session", "session-beta"],
    );
    let list_b_stdout = stdout(&list_b);
    assert!(list_b_stdout.contains("beta msg"));
    assert!(!list_b_stdout.contains("alpha msg"));

    let pending_a = run_cli(
        &home,
        &goose_root,
        &["message", "pending", "b", "--session", "session-alpha"],
    );
    assert!(stdout(&pending_a).contains("alpha msg"));

    let pending_b = run_cli(
        &home,
        &goose_root,
        &["message", "pending", "b", "--session", "session-beta"],
    );
    assert!(stdout(&pending_b).contains("beta msg"));
}

// ── Session cleanup (public API) ────────────────────────────────────────────

#[test]
fn session_store_cleanup_large_window_preserves_recent() {
    let (_temp, home, _goose_root) = test_env();

    let db = open_db(&home);
    let store = SessionStore::new(db);

    let key = SessionKey::new(Platform::Discord, "g1", "c1");
    store.append_user_message(&key, "message 1", None).unwrap();
    store.append_assistant_message(&key, "reply 1").unwrap();

    let stats_before = store.stats().unwrap();
    assert_eq!(stats_before.session_count, 1);
    assert_eq!(stats_before.message_count, 2);

    // Cleanup with a large window — freshly created sessions survive
    let deleted = store.cleanup(24 * 365).unwrap();
    assert_eq!(deleted, 0);

    let stats = store.stats().unwrap();
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.message_count, 2);
}

#[test]
fn session_store_message_retention_large_window_preserves_recent() {
    let (_temp, home, _goose_root) = test_env();

    let db = open_db(&home);
    let store = SessionStore::new(db);

    let key = SessionKey::new(Platform::Discord, "g1", "c1");
    store.append_user_message(&key, "message 1", None).unwrap();
    store.append_user_message(&key, "message 2", None).unwrap();

    // Cleanup with a large retention window — fresh messages survive
    let deleted = store.cleanup_expired_messages(365).unwrap();
    assert_eq!(deleted, 0);

    let history = store.load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 2);
}
