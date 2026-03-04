use opengoose_types::{AppEventKind, EventBus};
use tracing::field::{Field, Visit};
use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

pub struct TuiTracingLayer {
    event_bus: EventBus,
}

impl TuiTracingLayer {
    pub fn new(event_bus: EventBus) -> Self {
        Self { event_bus }
    }
}

struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else if self.message.is_empty() {
            self.message = format!("{}: {:?}", field.name(), value);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

impl<S: Subscriber> Layer<S> for TuiTracingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let level = event.metadata().level().to_string();
        let mut visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);

        if visitor.message.is_empty() {
            visitor.message = event.metadata().target().to_string();
        }

        self.event_bus.emit(AppEventKind::TracingEvent {
            level,
            message: visitor.message,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn test_tui_tracing_layer_captures_info() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let layer = TuiTracingLayer::new(bus);

        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("test message");
        });

        let event = rx.try_recv().unwrap();
        match &event.kind {
            AppEventKind::TracingEvent { level, message } => {
                assert_eq!(level, "INFO");
                assert!(message.contains("test message"));
            }
            other => panic!("expected TracingEvent, got: {:?}", other),
        }
    }

    #[test]
    fn test_tui_tracing_layer_captures_error() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let layer = TuiTracingLayer::new(bus);

        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!("oh no");
        });

        let event = rx.try_recv().unwrap();
        match &event.kind {
            AppEventKind::TracingEvent { level, message } => {
                assert_eq!(level, "ERROR");
                assert!(message.contains("oh no"));
            }
            other => panic!("expected TracingEvent, got: {:?}", other),
        }
    }

    #[test]
    fn test_tui_tracing_layer_debug_field() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let layer = TuiTracingLayer::new(bus);

        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(count = 42, "counted");
        });

        let event = rx.try_recv().unwrap();
        match &event.kind {
            AppEventKind::TracingEvent { message, .. } => {
                assert!(message.contains("counted"));
            }
            _ => panic!("expected TracingEvent"),
        }
    }

    #[test]
    fn test_tui_tracing_layer_no_message_uses_target() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let layer = TuiTracingLayer::new(bus);

        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            // Using event! macro with no message field
            tracing::event!(tracing::Level::WARN, answer = 42);
        });

        let event = rx.try_recv().unwrap();
        match &event.kind {
            AppEventKind::TracingEvent { level, message } => {
                assert_eq!(level, "WARN");
                // Should have the field or target
                assert!(!message.is_empty());
            }
            _ => panic!("expected TracingEvent"),
        }
    }

    #[test]
    fn test_message_visitor_record_str() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let layer = TuiTracingLayer::new(bus);

        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(message = "str message");
        });

        let event = rx.try_recv().unwrap();
        match &event.kind {
            AppEventKind::TracingEvent { message, .. } => {
                assert!(message.contains("str message"));
            }
            _ => panic!("expected TracingEvent"),
        }
    }
}
