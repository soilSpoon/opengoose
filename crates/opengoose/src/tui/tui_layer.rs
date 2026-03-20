use super::log_entry::LogEntry;
use chrono::Utc;
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// tracing Layer: 이벤트를 LogEntry로 변환하여 mpsc 채널로 전송.
/// on_event()는 동기 함수이므로 try_send() 사용.
/// 채널 가득 차면 조용히 버림 (파일에는 전체 기록됨).
pub struct TuiLayer {
    tx: mpsc::Sender<LogEntry>,
}

impl TuiLayer {
    pub fn new(tx: mpsc::Sender<LogEntry>) -> Self {
        Self { tx }
    }
}

impl<S: Subscriber> Layer<S> for TuiLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = *metadata.level();
        let target = metadata.target().to_string();

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let structured =
            LogEntry::is_structured_target(&target) && level <= Level::INFO;

        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            target,
            message: visitor.message,
            structured,
        };

        // 동기 전송 — 채널 가득 차면 drop
        let _ = self.tx.try_send(entry);
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else if self.message.is_empty() {
            self.message = format!("{} = {:?}", field.name(), value);
        } else {
            self.message
                .push_str(&format!(" {} = {:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else if self.message.is_empty() {
            self.message = format!("{} = {}", field.name(), value);
        } else {
            self.message
                .push_str(&format!(" {} = {}", field.name(), value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn tui_layer_sends_log_entry() {
        let (tx, mut rx) = mpsc::channel(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "opengoose_rig::rig", "test message");
        });

        let entry = rx.try_recv().unwrap();
        assert!(entry.structured);
        assert_eq!(entry.level, Level::INFO);
        assert!(entry.message.contains("test message"));
    }

    #[tokio::test]
    async fn tui_layer_does_not_panic_when_channel_full() {
        let (tx, _rx) = mpsc::channel(1);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            for _ in 0..100 {
                tracing::info!("flood");
            }
        });
        // No panic = success
    }

    #[tokio::test]
    async fn non_structured_target_is_not_structured() {
        let (tx, mut rx) = mpsc::channel(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "goose::agents", "some agent event");
        });

        let entry = rx.try_recv().unwrap();
        assert!(!entry.structured);
    }
}
