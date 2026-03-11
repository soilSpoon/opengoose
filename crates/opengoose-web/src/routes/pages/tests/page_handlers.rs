use axum::extract::{Query, State};
use axum::response::Html;
use opengoose_persistence::{PluginStore, ScheduleStore, TriggerStore};
use opengoose_types::{Platform, SessionKey};

use super::super::catalog::{
    agents, plugins, queue, runs, schedules, sessions, triggers, workflows,
};
use super::super::catalog_forms::{
    AgentQuery, PluginQuery, RunQuery, ScheduleQuery, SessionQuery, TriggerQuery, WorkflowQuery,
};
use super::super::dashboard::dashboard;
use super::support::{
    TEMP_HOME_PREFIX, page_state, run_async, save_run, save_session, save_team, test_db,
};
use crate::test_support::with_temp_home;

#[tokio::test]
async fn dashboard_handler_renders_mock_preview() {
    let Html(html) = dashboard(State(page_state(test_db())))
        .await
        .expect("handler should render");

    assert!(html.contains("Mock preview"));
    assert!(html.contains("No runtime data yet"));
}

#[tokio::test]
async fn sessions_handler_invalid_selection_falls_back_to_live_session() {
    let db = test_db();
    let session_key = SessionKey::new(Platform::Discord, "ops", "chan-1");
    save_session(db.clone(), &session_key, Some("reviewers"));

    let Html(html) = sessions(
        State(page_state(db)),
        Query(SessionQuery {
            session: Some("discord:ns:missing:session".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("Live runtime"));
    assert!(html.contains("Session chan-1"));
    assert!(html.contains(&session_key.to_stable_id()));
}

#[tokio::test]
async fn runs_handler_invalid_selection_falls_back_to_live_run() {
    let db = test_db();
    save_run(db.clone(), "run-live-1");

    let Html(html) = runs(
        State(page_state(db)),
        Query(RunQuery {
            run: Some("missing-run".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("Live runtime"));
    assert!(html.contains("Run run-live-1"));
    assert!(html.contains("ops / chain"));
}

#[tokio::test]
async fn plugins_handler_renders_installed_plugin_detail() {
    let db = test_db();
    PluginStore::new(db.clone())
        .install(
            "ops-tools",
            "1.2.3",
            "/tmp/ops-tools",
            Some("OG"),
            Some("Operational helpers"),
            "skill,channel_adapter",
        )
        .expect("plugin should seed");

    let Html(html) = plugins(
        State(page_state(db)),
        Query(PluginQuery {
            plugin: Some("ops-tools".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("1 plugin(s) installed"));
    assert!(html.contains("ops-tools"));
    assert!(html.contains("Disable plugin"));
}

#[test]
fn agents_handler_renders_bundled_defaults_for_unknown_selection() {
    with_temp_home(TEMP_HOME_PREFIX, || {
        run_async(async {
            let Html(html) = agents(
                State(page_state(test_db())),
                Query(AgentQuery {
                    agent: Some("missing-agent".into()),
                }),
            )
            .await
            .expect("handler should render");

            assert!(html.contains("Bundled defaults"));
            assert!(html.contains("aria-current=\"page\""));
        });
    });
}

#[test]
fn workflows_handler_renders_bundled_defaults_for_unknown_selection() {
    with_temp_home(TEMP_HOME_PREFIX, || {
        run_async(async {
            let Html(html) = workflows(
                State(page_state(test_db())),
                Query(WorkflowQuery {
                    workflow: Some("missing-workflow".into()),
                }),
            )
            .await
            .expect("handler should render");

            assert!(html.contains("Bundled defaults"));
            assert!(html.contains("Workflow detail"));
            assert!(html.contains("aria-current=\"page\""));
        });
    });
}

#[tokio::test]
async fn triggers_handler_invalid_selection_falls_back_to_existing_trigger() {
    let db = test_db();
    TriggerStore::new(db.clone())
        .create("incoming", "webhook_received", "{}", "ops", "")
        .expect("trigger should seed");

    let Html(html) = triggers(
        State(page_state(db)),
        Query(TriggerQuery {
            trigger: Some("missing-trigger".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("1 trigger(s)"));
    assert!(html.contains("incoming"));
    assert!(html.contains("webhook_received"));
}

#[tokio::test]
async fn queue_handler_invalid_selection_falls_back_to_live_run() {
    let db = test_db();
    save_run(db.clone(), "run-queue-1");

    let Html(html) = queue(
        State(page_state(db)),
        Query(RunQuery {
            run: Some("missing-run".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("Live runtime"));
    assert!(html.contains("Queue run-queue-1"));
    assert!(html.contains("No queue traffic has been recorded for this run yet."));
}

#[test]
fn schedules_handler_renders_existing_schedule() {
    with_temp_home(TEMP_HOME_PREFIX, || {
        save_team("ops");
        let db = test_db();
        ScheduleStore::new(db.clone())
            .create(
                "nightly-ops",
                "0 0 * * * *",
                "ops",
                "",
                Some("2026-03-11 00:00:00"),
            )
            .expect("schedule should seed");

        run_async(async {
            let Html(html) = schedules(
                State(page_state(db)),
                Query(ScheduleQuery {
                    schedule: Some("nightly-ops".into()),
                }),
            )
            .await
            .expect("handler should render");

            assert!(html.contains("nightly-ops"));
            assert!(html.contains("Recent matching runs"));
        });
    });
}
