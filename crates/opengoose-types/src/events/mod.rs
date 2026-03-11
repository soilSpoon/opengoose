use std::time::Instant;

mod bus;
mod display;
mod kind;

pub use bus::EventBus;
pub use kind::AppEventKind;

#[derive(Debug, Clone)]
pub struct AppEvent {
    pub kind: AppEventKind,
    pub timestamp: Instant,
}
