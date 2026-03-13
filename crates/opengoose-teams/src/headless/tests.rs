use std::future::Future;
use std::sync::{Arc, Mutex};

use anyhow::bail;
use uuid::Uuid;

use opengoose_persistence::{Database, OrchestrationStore, SessionStore, WorkItemStore};
use opengoose_types::{EventBus, Platform, SessionKey};

use crate::store::TeamStore;
use crate::team::{CommunicationMode, OrchestrationPattern, TeamAgent, TeamDefinition};

use super::config::HeadlessConfig;
use super::resume::find_parent_work_item;
use super::resume::{resume_headless, resume_headless_with};
use super::run::{run_headless, run_headless_with};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home(test: impl FnOnce() + std::panic::UnwindSafe) {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_home =
        std::env::temp_dir().join(format!("opengoose-headless-home-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_home).unwrap();

    let saved_home = std::env::var("HOME").ok();

    unsafe {
        std::env::set_var("HOME", &temp_home);
    }

    let result = std::panic::catch_unwind(test);

    unsafe {
        match saved_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }

    let _ = std::fs::remove_dir_all(&temp_home);

    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

fn run_async_test(test: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(test);
}

fn save_test_team(name: &str) {
    let store = TeamStore::new().unwrap();
    store
        .save(
            &TeamDefinition {
                version: "1.0.0".into(),
                title: name.into(),
                description: Some("test team".into()),
                goal: None,
                workflow: OrchestrationPattern::Chain,
                agents: vec![TeamAgent {
                    profile: "tester".into(),
                    role: Some("validate setup".into()),
                }],
                router: None,
                fan_out: None,
                communication_mode: CommunicationMode::default(),
            },
            false,
        )
        .unwrap();
}

fn seed_suspended_run(db: &Arc<Database>, run_id: &str, team_name: &str) -> (String, i32) {
    let session_key = SessionKey::new(Platform::Custom("cli".into()), "headless", run_id);
    let session_id = session_key.to_stable_id();
    let orchestration = OrchestrationStore::new(db.clone());
    orchestration
        .create_run(run_id, &session_id, team_name, "chain", "resume me", 1)
        .unwrap();
    orchestration.suspend_incomplete().unwrap();

    let parent_id = WorkItemStore::new(db.clone())
        .create(&session_id, run_id, &format!("Team: {team_name}"), None)
        .unwrap();

    (session_id, parent_id)
}

#[test]
fn run_headless_sets_up_context_before_execution() {
    with_temp_home(|| {
        save_test_team("demo-team");

        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let bus = EventBus::new(16);

            let (run_id, result) = run_headless_with(
                "demo-team",
                "hello world",
                db,
                bus,
                None,
                |team: TeamDefinition, _profile_store, input, ctx| async move {
                    assert_eq!(team.name(), "demo-team");
                    assert_eq!(ctx.team_run_id, ctx.session_key.channel_id);
                    assert_eq!(input, "hello world");

                    let history = ctx.sessions().load_history(&ctx.session_key, 10)?;
                    assert_eq!(history.len(), 1);
                    assert_eq!(history[0].role, "user");
                    assert_eq!(history[0].content, "hello world");
                    assert_eq!(history[0].author.as_deref(), Some("cli"));

                    Ok(format!("ok:{}:{}", ctx.team_run_id, history[0].content))
                },
            )
            .await
            .unwrap();

            assert_eq!(result, format!("ok:{run_id}:hello world"));
        });
    });
}

#[test]
fn run_headless_returns_team_not_found_error() {
    with_temp_home(|| {
        run_async_test(async {
            let err = run_headless(HeadlessConfig::new(
                "missing-team",
                "hello",
                Arc::new(Database::open_in_memory().unwrap()),
                EventBus::new(16),
            ))
            .await
            .unwrap_err();

            assert!(err.to_string().contains("missing-team"));
        });
    });
}

#[test]
fn run_headless_propagates_execution_errors() {
    with_temp_home(|| {
        save_test_team("demo-team");

        run_async_test(async {
            let err = run_headless_with(
                "demo-team",
                "hello",
                Arc::new(Database::open_in_memory().unwrap()),
                EventBus::new(16),
                None,
                |_team, _profile_store, _input, _ctx| async move { bail!("boom") },
            )
            .await
            .unwrap_err();

            assert_eq!(err.to_string(), "boom");
        });
    });
}

#[test]
fn resume_headless_resolves_parent_and_session_context() {
    with_temp_home(|| {
        save_test_team("demo-team");

        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let bus = EventBus::new(16);
            let run_id = "run-123";
            let (session_id, parent_id) = seed_suspended_run(&db, run_id, "demo-team");

            let result = resume_headless_with(
                run_id,
                db,
                bus,
                |team, _profile_store, ctx, resolved_parent_id| async move {
                    assert_eq!(team.name(), "demo-team");
                    assert_eq!(ctx.team_run_id, run_id);
                    assert_eq!(ctx.session_key.to_stable_id(), session_id);
                    assert_eq!(resolved_parent_id, parent_id);
                    Ok(format!("resumed:{resolved_parent_id}"))
                },
            )
            .await
            .unwrap();

            assert_eq!(result, format!("resumed:{parent_id}"));
        });
    });
}

#[test]
fn resume_headless_returns_run_not_found_error() {
    with_temp_home(|| {
        run_async_test(async {
            let err = resume_headless(
                "missing-run",
                Arc::new(Database::open_in_memory().unwrap()),
                EventBus::new(16),
            )
            .await
            .unwrap_err();

            assert!(err.to_string().contains("run 'missing-run' not found"));
        });
    });
}

#[test]
fn resume_headless_rejects_non_suspended_runs() {
    with_temp_home(|| {
        save_test_team("demo-team");

        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let session_key = SessionKey::new(Platform::Custom("cli".into()), "headless", "run-1");
            let orchestration = OrchestrationStore::new(db.clone());
            orchestration
                .create_run(
                    "run-1",
                    &session_key.to_stable_id(),
                    "demo-team",
                    "chain",
                    "hi",
                    1,
                )
                .unwrap();

            let err = resume_headless("run-1", db, EventBus::new(16))
                .await
                .unwrap_err();

            assert!(err.to_string().contains("is not suspended"));
            assert!(err.to_string().contains("running"));
        });
    });
}

#[test]
fn resume_headless_requires_parent_work_item() {
    with_temp_home(|| {
        save_test_team("demo-team");

        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let run_id = "run-no-parent";
            let session_key = SessionKey::new(Platform::Custom("cli".into()), "headless", run_id);
            let orchestration = OrchestrationStore::new(db.clone());
            orchestration
                .create_run(
                    run_id,
                    &session_key.to_stable_id(),
                    "demo-team",
                    "chain",
                    "resume me",
                    1,
                )
                .unwrap();
            orchestration.suspend_incomplete().unwrap();

            let err = resume_headless(run_id, db, EventBus::new(16))
                .await
                .unwrap_err();

            assert!(err.to_string().contains("no parent work item found"));
        });
    });
}

#[test]
fn find_parent_work_item_returns_parent_id() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let session_key = SessionKey::new(Platform::Custom("cli".into()), "headless", "run-parent");
    let session_id = session_key.to_stable_id();
    let session_store = SessionStore::new(db.clone());
    session_store
        .append_user_message(&session_key, "hello", Some("cli"))
        .unwrap();

    let work_items = WorkItemStore::new(db.clone());
    let parent_id = work_items
        .create(&session_id, "run-parent", "Team: demo", None)
        .unwrap();
    work_items
        .create(&session_id, "run-parent", "Child", Some(parent_id))
        .unwrap();

    assert_eq!(find_parent_work_item(&db, "run-parent").unwrap(), parent_id);
}
