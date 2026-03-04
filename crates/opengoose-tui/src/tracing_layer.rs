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
