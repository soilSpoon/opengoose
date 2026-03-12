use crate::stream_orchestrator::StreamResponder;
use crate::stream_responder::DraftHandle;
use std::sync::{Arc, Mutex};

/// Test implementation of StreamResponder that records calls.
pub(super) struct MockResponder {
    calls: Arc<Mutex<Vec<String>>>,
}

impl MockResponder {
    pub(super) fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                calls: calls.clone(),
            },
            calls,
        )
    }
}

#[async_trait::async_trait]
impl StreamResponder for MockResponder {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_message_len(&self) -> usize {
        2000
    }

    async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("create_draft:{channel_id}"));
        Ok(DraftHandle {
            message_id: "mock-msg-1".into(),
            channel_id: channel_id.into(),
        })
    }

    async fn update_draft(&self, _handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("update:{}", content.len()));
        Ok(())
    }

    async fn send_new_message(&self, _channel_id: &str, content: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("send_new:{}", content.len()));
        Ok(())
    }

    async fn finalize_draft(&self, _handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("finalize:{}", content.len()));
        Ok(())
    }
}

/// Mock that always returns an error from `update_draft`.
pub(super) struct FailingUpdateResponder {
    calls: Arc<Mutex<Vec<String>>>,
}

impl FailingUpdateResponder {
    pub(super) fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                calls: calls.clone(),
            },
            calls,
        )
    }
}

#[async_trait::async_trait]
impl StreamResponder for FailingUpdateResponder {
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_message_len(&self) -> usize {
        2000
    }
    async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("create_draft:{channel_id}"));
        Ok(DraftHandle {
            message_id: "msg".into(),
            channel_id: channel_id.into(),
        })
    }
    async fn update_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("rate limited"))
    }
    async fn send_new_message(&self, _channel_id: &str, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn finalize_draft(&self, _handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("finalize:{}", content.len()));
        Ok(())
    }
}

/// Mock that fails on create_draft.
pub(super) struct FailingCreateResponder;

#[async_trait::async_trait]
impl StreamResponder for FailingCreateResponder {
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_message_len(&self) -> usize {
        2000
    }
    async fn create_draft(&self, _channel_id: &str) -> anyhow::Result<DraftHandle> {
        Err(anyhow::anyhow!("channel not found"))
    }
    async fn update_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn send_new_message(&self, _channel_id: &str, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn finalize_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Mock that fails on finalize_draft.
pub(super) struct FailingFinalizeResponder {
    calls: Arc<Mutex<Vec<String>>>,
}

impl FailingFinalizeResponder {
    pub(super) fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                calls: calls.clone(),
            },
            calls,
        )
    }
}

#[async_trait::async_trait]
impl StreamResponder for FailingFinalizeResponder {
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_message_len(&self) -> usize {
        2000
    }
    async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("create_draft:{channel_id}"));
        Ok(DraftHandle {
            message_id: "msg".into(),
            channel_id: channel_id.into(),
        })
    }
    async fn update_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn send_new_message(&self, _channel_id: &str, _content: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn finalize_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("finalize failed"))
    }
}
