use anyhow::Context;
use tokio::sync::broadcast;
use tracing::{debug, debug_span, warn};

use opengoose_types::StreamChunk;

use crate::message_utils::truncate_for_display;
use crate::stream_responder::StreamResponder;
use crate::throttle::ThrottlePolicy;

/// Drive a streaming response: consume token chunks from the broadcast
/// receiver, throttle updates, and edit the draft message on the platform.
///
/// Returns the full accumulated text on success.
///
/// # Arguments
/// * `responder` — platform-specific implementation that creates/edits messages
/// * `channel_id` — target channel for the draft message
/// * `rx` — receiver of [`StreamChunk`] events from the LLM/engine
/// * `throttle` — platform-appropriate rate limiting policy
/// * `max_display_len` — platform message size limit for intermediate updates
pub async fn drive_stream(
    responder: &dyn StreamResponder,
    channel_id: &str,
    mut rx: broadcast::Receiver<StreamChunk>,
    mut throttle: ThrottlePolicy,
    max_display_len: usize,
) -> anyhow::Result<String> {
    let span = debug_span!(
        "drive_stream",
        channel_id = %channel_id,
        max_display_len = %max_display_len,
    )
    .entered();
    drop(span);

    let handle = responder.create_draft(channel_id).await?;
    let mut buffer = String::new();

    loop {
        match rx.recv().await {
            Ok(StreamChunk::Delta(delta)) => {
                buffer.push_str(&delta);
                if throttle.should_update(buffer.len()) {
                    let display = truncate_for_display(&buffer, max_display_len);
                    if let Err(e) = responder.update_draft(&handle, display).await {
                        warn!(%e, "failed to update draft, continuing to buffer");
                    }
                    throttle.record_update(buffer.len());
                }
            }
            Ok(StreamChunk::Done) => {
                debug!(
                    buffer_len = buffer.len(),
                    "stream completed, finalizing draft"
                );
                responder.finalize_draft(&handle, &buffer).await?;
                break;
            }
            Ok(StreamChunk::Error(e)) => {
                debug!(error = %e, buffer_len = buffer.len(), "stream error received");
                let error_msg = format!("{buffer}\n\n--- Error: {e}");
                let display = truncate_for_display(&error_msg, max_display_len);
                if let Err(finalize_err) = responder.finalize_draft(&handle, display).await {
                    return Err(finalize_err)
                        .context(format!("failed to finalize draft after stream error: {e}"));
                }
                return Err(anyhow::anyhow!(e));
            }
            Err(broadcast::error::RecvError::Closed) => {
                // Sender dropped without sending Done — finalize what we have,
                // but surface the unexpected closure to the caller.
                if !buffer.is_empty() {
                    responder
                        .finalize_draft(&handle, &buffer)
                        .await
                        .context("failed to finalize draft after stream closed unexpectedly")?;
                }
                return Err(anyhow::anyhow!(
                    "stream closed before completion for channel {channel_id}"
                ));
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(skipped = n, "stream receiver lagged, some tokens lost");
                continue;
            }
        }
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests;
