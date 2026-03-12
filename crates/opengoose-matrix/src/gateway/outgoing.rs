use async_trait::async_trait;
use tracing::{debug, error, warn};

use goose::gateway::PlatformUser;

use opengoose_core::message_utils::{split_message, truncate_for_display};
use opengoose_core::{DraftHandle, StreamResponder};

use crate::types::{edit_content, text_content};

use super::{MATRIX_MAX_LEN, MatrixGateway};

impl MatrixGateway {
    /// Send a plain-text message to a room, splitting if needed.
    pub(super) async fn post_message(&self, room_id: &str, text: &str) -> anyhow::Result<()> {
        debug!(room_id = %room_id, text_len = text.len(), "posting matrix message");
        for chunk in split_message(text, MATRIX_MAX_LEN) {
            let content = text_content(chunk);
            if let Err(e) = self.send_event(room_id, &content).await {
                warn!(%e, %room_id, "failed to send matrix message chunk");
            }
        }
        Ok(())
    }

    pub(super) async fn send_outgoing_text(
        &self,
        user: &PlatformUser,
        body: &str,
    ) -> anyhow::Result<()> {
        let channel_id = self
            .bridge
            .route_outgoing_text(&user.user_id, body, "matrix")
            .await;

        if let Err(e) = self.post_message(&channel_id, body).await {
            error!(%e, "failed to send matrix message");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// StreamResponder trait
// ---------------------------------------------------------------------------

#[async_trait]
impl StreamResponder for MatrixGateway {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_message_len(&self) -> usize {
        MATRIX_MAX_LEN
    }

    async fn create_draft(&self, room_id: &str) -> anyhow::Result<DraftHandle> {
        debug!(room_id = %room_id, "creating matrix draft");
        let content = text_content("Thinking...");
        let event_id = self.send_event(room_id, &content).await?;
        debug!(room_id = %room_id, event_id = %event_id, "matrix draft created");
        Ok(DraftHandle {
            message_id: event_id,
            channel_id: room_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        debug!(
            room_id = %handle.channel_id,
            event_id = %handle.message_id,
            content_len = content.len(),
            "updating matrix draft"
        );
        let display = truncate_for_display(content, MATRIX_MAX_LEN);
        let ev_content = edit_content(&handle.message_id, display);
        self.send_event(&handle.channel_id, &ev_content).await?;
        Ok(())
    }

    async fn send_new_message(&self, room_id: &str, content: &str) -> anyhow::Result<()> {
        self.post_message(room_id, content).await
    }

    // finalize_draft uses the default implementation from StreamResponder
}
