use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use serde::Deserialize;

use super::snapshot::{EventFilter, serialize_app_event};
use crate::handlers::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    pub types: Option<String>,
}

pub(super) fn build_event_stream(
    mut rx: tokio::sync::broadcast::Receiver<opengoose_types::AppEvent>,
    filter: EventFilter,
) -> impl Stream<Item = Result<Event, Infallible>> + Send {
    stream! {
        loop {
            match rx.recv().await {
                Ok(app_event) => {
                    if let Some(event) = serialize_app_event(&app_event.kind, &filter) {
                        yield Ok(Event::default().event(event.event.as_str()).data(event.data));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

/// GET /api/events — subscribe to live app events as SSE.
pub async fn stream_events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, AppError> {
    let filter = EventFilter::parse(query.types.as_deref())?;
    let event_stream = build_event_stream(state.event_bus.subscribe(), filter);

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-events"),
    ))
}
