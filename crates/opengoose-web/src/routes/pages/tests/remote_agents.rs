use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Html;

use super::super::remote_agents::{remote_agents, websocket_url};
use super::support::{page_state, test_db};

#[tokio::test]
async fn remote_agents_handler_renders_empty_registry() {
    let mut headers = HeaderMap::new();
    headers.insert("host", "opengoose.test".parse().expect("host header"));

    let Html(html) = remote_agents(State(page_state(test_db())), headers)
        .await
        .expect("handler should render");

    assert!(html.contains("No remote agents are connected right now."));
    assert!(html.contains("ws://opengoose.test/api/agents/connect"));
    assert!(html.contains(
        "data-init=\"@get('/remote-agents/events', { openWhenHidden: true, retry: 'always' })\""
    ));
}

#[tokio::test]
async fn remote_agents_handler_renders_registered_agents() {
    let state = page_state(test_db());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    state
        .remote_registry
        .register(
            "remote-a".into(),
            vec!["execute".into(), "relay".into()],
            "ws://remote-a:9000".into(),
            tx,
        )
        .await
        .expect("agent should register");

    let mut headers = HeaderMap::new();
    headers.insert("host", "dashboard.local".parse().expect("host header"));

    let Html(html) = remote_agents(State(state), headers)
        .await
        .expect("handler should render");

    assert!(html.contains("remote-a"));
    assert!(html.contains("execute"));
    assert!(html.contains("ws://remote-a:9000"));
    assert!(html.contains("/remote-agents/remote-a/disconnect"));
    assert!(html.contains("Disconnect"));
}

#[test]
fn websocket_url_prefers_forwarded_https_headers() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-host",
        "goose.example.com".parse().expect("forwarded host"),
    );
    headers.insert(
        "x-forwarded-proto",
        "https".parse().expect("forwarded proto"),
    );
    headers.insert("host", "localhost:3000".parse().expect("host header"));

    assert_eq!(
        websocket_url(&headers),
        "wss://goose.example.com/api/agents/connect"
    );
}
