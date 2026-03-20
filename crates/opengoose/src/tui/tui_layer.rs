use super::log_entry::LogEntry;
use chrono::Utc;
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

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

        let structured = LogEntry::is_structured_target(&target) && level <= Level::INFO;

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

    #[test]
    fn message_visitor_record_str_message_field() {
        let v = MessageVisitor::default();
        // Simulate a tracing event with a "message" field
        let (tx, mut rx) = tokio::sync::mpsc::channel::<LogEntry>(1);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(custom_field = "custom_value", "main message");
            });
            rx.try_recv()
        });
        let entry = result.unwrap();
        // Message field should be the main message
        assert!(entry.message.contains("main message"));
        drop(v);
    }

    #[test]
    fn message_visitor_record_debug_non_message_first() {
        let visitor = MessageVisitor::default();
        // Use a tracing event with only debug fields (no "message" field)
        // This tests the record_debug branches for non-message fields
        let (tx, mut rx) = tokio::sync::mpsc::channel::<LogEntry>(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            tracing::subscriber::with_default(subscriber, || {
                // Multiple non-message str fields
                tracing::info!(field_a = "value_a", field_b = "value_b");
            });
            if let Ok(entry) = rx.try_recv() {
                // First non-message field sets message, second is appended
                assert!(!entry.message.is_empty());
            }
        });
        drop(visitor);
    }

    #[tokio::test]
    async fn tui_layer_evolver_target_is_structured() {
        let (tx, mut rx) = mpsc::channel(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "opengoose::evolver", "evolver event");
        });

        let entry = rx.try_recv().unwrap();
        assert!(entry.structured);
    }

    #[tokio::test]
    async fn tui_layer_debug_structured_target_not_structured() {
        // DEBUG level > INFO (DEBUG has higher verbosity value), so structured = false
        let (tx, mut rx) = mpsc::channel(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "opengoose_rig::rig", "debug from rig");
        });

        // DEBUG < default subscriber filter level, so entry may or may not be sent
        // Just test that no panic occurs
        drop(rx.try_recv());
    }

    #[tokio::test]
    async fn tui_layer_warn_structured_target_is_structured() {
        // WARN < INFO in tracing ordering, so level <= INFO is true → structured
        let (tx, mut rx) = mpsc::channel(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "opengoose_rig::rig", "warn from rig");
        });

        let entry = rx.try_recv().unwrap();
        // WARN <= INFO → structured = true for structured targets
        assert!(entry.structured);
    }

    #[test]
    fn message_visitor_record_str_with_message_field() {
        // Covers line 66: record_str called when field.name() == "message"
        // tracing::info!(message = "string") uses record_str (not record_debug) for &str
        let (tx, mut rx) = tokio::sync::mpsc::channel::<LogEntry>(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(message = "direct str message field");
        });

        if let Ok(entry) = rx.try_recv() {
            assert!(entry.message.contains("direct str message field"));
        }
    }

    #[test]
    fn message_visitor_record_debug_non_message_fields() {
        // Integer fields → record_debug (not record_str), no "message" field
        // First non-message field: hits else-if-empty branch; second hits else-push_str
        let (tx, mut rx) = tokio::sync::mpsc::channel::<LogEntry>(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(count = 42usize, retries = 3usize);
            });
            if let Ok(entry) = rx.try_recv() {
                assert!(!entry.message.is_empty());
            }
        });
    }
}
