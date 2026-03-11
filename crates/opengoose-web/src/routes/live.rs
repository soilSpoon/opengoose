use std::convert::Infallible;
use std::future::Future;
use std::time::Duration;

use async_stream::stream;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use opengoose_types::{AppEvent, AppEventKind};
use tokio::sync::{broadcast, watch};

use super::{PartialResult, datastar_patch_elements_event};

fn sse_response<S>(
    stream: S,
    keep_alive_text: &'static str,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send>
where
    S: Stream<Item = Result<Event, Infallible>> + Send + 'static,
{
    Sse::new(Box::pin(stream)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text(keep_alive_text),
    )
}

fn render_patch_event<RenderFn>(render_html: &RenderFn, error_html: &'static str) -> Event
where
    RenderFn: Fn() -> PartialResult,
{
    match render_html() {
        Ok(html) => datastar_patch_elements_event(&html),
        Err(_) => datastar_patch_elements_event(error_html),
    }
}

pub(crate) fn broadcast_live_sse<MatchFn, RenderFn>(
    mut rx: broadcast::Receiver<AppEvent>,
    initial: String,
    keep_alive_text: &'static str,
    fallback_interval: Option<Duration>,
    render_on_lagged: bool,
    matches_event: MatchFn,
    render_html: RenderFn,
    error_html: &'static str,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send>
where
    MatchFn: Fn(&AppEventKind) -> bool + Send + 'static,
    RenderFn: Fn() -> PartialResult + Send + 'static,
{
    let event_stream = stream! {
        yield Ok(datastar_patch_elements_event(&initial));

        if let Some(interval) = fallback_interval {
            let mut fallback = tokio::time::interval(interval);
            fallback.tick().await;

            loop {
                tokio::select! {
                    event = rx.recv() => match event {
                        Ok(app_event) if matches_event(&app_event.kind) => {
                            yield Ok(render_patch_event(&render_html, error_html));
                        }
                        Ok(_) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) if render_on_lagged => {
                            yield Ok(render_patch_event(&render_html, error_html));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    },
                    _ = fallback.tick() => {
                        yield Ok(render_patch_event(&render_html, error_html));
                    }
                }
            }
        } else {
            loop {
                match rx.recv().await {
                    Ok(app_event) if matches_event(&app_event.kind) => {
                        yield Ok(render_patch_event(&render_html, error_html));
                    }
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) if render_on_lagged => {
                        yield Ok(render_patch_event(&render_html, error_html));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    };

    sse_response(event_stream, keep_alive_text)
}

pub(crate) fn watch_live_sse<RenderFn, RenderFut>(
    mut changes: watch::Receiver<u64>,
    initial: String,
    keep_alive_text: &'static str,
    render_html: RenderFn,
    error_html: &'static str,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send>
where
    RenderFn: Fn() -> RenderFut + Send + 'static,
    RenderFut: Future<Output = PartialResult> + Send + 'static,
{
    let event_stream = stream! {
        yield Ok(datastar_patch_elements_event(&initial));

        loop {
            match changes.changed().await {
                Ok(()) => {
                    let event = match render_html().await {
                        Ok(html) => datastar_patch_elements_event(&html),
                        Err(_) => datastar_patch_elements_event(error_html),
                    };
                    yield Ok(event);
                }
                Err(_) => break,
            }
        }
    };

    sse_response(event_stream, keep_alive_text)
}
