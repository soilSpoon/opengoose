use axum::extract::{Query, State};
use axum::response::Html;

use super::super::support::{
    page_state, run_async, save_run, save_session, session_key, test_db, with_pages_home,
};
use super::{
    AgentQuery, RunQuery, SessionQuery, WorkflowQuery, agents, queue, runs, sessions, workflows,
};

#[tokio::test]
async fn sessions_handler_invalid_selection_falls_back_to_live_session() {
    let db = test_db();
    let selected_session = session_key("chan-1");
    save_session(db.clone(), &selected_session, Some("reviewers"));

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
    assert!(html.contains(&selected_session.to_stable_id()));
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

#[test]
fn agents_handler_renders_bundled_defaults_for_unknown_selection() {
    with_pages_home(|| {
        run_async(async {
            let Html(html) = agents(Query(AgentQuery {
                agent: Some("missing-agent".into()),
            }))
            .await
            .expect("handler should render");

            assert!(html.contains("Bundled defaults"));
            assert!(html.contains("aria-current=\"page\""));
        });
    });
}

#[test]
fn workflows_handler_renders_bundled_defaults_for_unknown_selection() {
    with_pages_home(|| {
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
