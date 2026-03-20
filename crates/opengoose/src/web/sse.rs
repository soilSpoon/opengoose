use axum::extract::State;
use axum::response::sse::{Event, Sse};
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use super::AppState;

pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(()) => Some(Ok(Event::default().event("board_changed").data(""))),
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_board::Board;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn events_handler_creates_stream() {
        let (tx, _) = broadcast::channel(8);
        let app_state = AppState {
            board: std::sync::Arc::new(Board::in_memory().await.unwrap()),
            tx,
        };
        let _stream = events(State(app_state)).await;
    }

    #[tokio::test]
    async fn events_handler_with_sent_event() {
        let (tx, _rx) = broadcast::channel(8);
        // Send before subscribing → receiver will get lag error (Err path in filter_map)
        tx.send(()).ok();

        let app_state = AppState {
            board: std::sync::Arc::new(Board::in_memory().await.unwrap()),
            tx,
        };
        // Just verify no panic creating the SSE response
        let _sse = events(State(app_state)).await;
    }

    /// Covers sse.rs:15 — the production `Err(_) => None` arm in the filter_map closure.
    ///
    /// Strategy: call the real `events()` handler via an in-process axum router, overflow
    /// the broadcast channel (capacity 1) after the handler subscribes but before the
    /// stream is polled, then collect the response body to drive the stream.
    /// The first poll of BroadcastStream returns Lagged → filter_map returns None (line 15).
    #[tokio::test]
    async fn events_production_filter_map_err_branch_via_service() {
        use http_body_util::BodyExt as _;
        use tower::ServiceExt as _;

        // Capacity 1 makes it easy to cause a lag: two sends overflow it.
        let (tx, _) = broadcast::channel::<()>(1);

        let state = AppState {
            board: std::sync::Arc::new(Board::in_memory().await.unwrap()),
            tx: tx.clone(),
        };

        let app = axum::Router::new()
            .route("/api/events", axum::routing::get(events))
            .with_state(state);

        let request = axum::extract::Request::builder()
            .uri("/api/events")
            .body(axum::body::Body::empty())
            .unwrap();

        // Process the request: events() subscribes its receiver from tx, then returns
        // the Sse wrapper.  The inner stream is not yet polled at this point.
        let response: axum::response::Response = app.oneshot(request).await.unwrap();

        // With the subscription now live, overflow the channel.
        // send 1: msg1 (rx cursor = 0, buffer has [msg1])
        // send 2: msg2 overwrites msg1 (capacity 1), rx cursor still at 0 → rx is lagged
        tx.send(()).ok();
        tx.send(()).ok();
        // Drop the last sender so the channel closes → BroadcastStream will terminate.
        drop(tx);

        // Consuming the response body drives the SSE stream:
        //   poll 1 → BroadcastStream: Err(Lagged(1)) → filter_map Err branch (line 15) → None
        //   poll 2 → BroadcastStream: Ok(()) from msg2 → filter_map Ok branch → SSE event
        //   poll 3 → BroadcastStream: channel closed → None → stream ends
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&body).unwrap_or("");
        assert!(
            text.contains("board_changed"),
            "expected board_changed event, got: {text}"
        );
    }
}
