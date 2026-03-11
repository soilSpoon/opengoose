use axum::extract::State;
use axum::response::Html;

use super::super::dashboard::dashboard;
use super::support::{page_state, test_db};

#[tokio::test]
async fn dashboard_handler_renders_mock_preview() {
    let Html(html) = dashboard(State(page_state(test_db())))
        .await
        .expect("handler should render");

    assert!(html.contains("Mock preview"));
    assert!(html.contains("No runtime data yet"));
}
