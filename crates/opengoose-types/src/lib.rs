//! Shared types, events, and utilities used across all OpenGoose crates.
//!
//! This crate is the common vocabulary of the system. It defines:
//! - [`Platform`] — the messaging platform a channel belongs to.
//! - [`AppEvent`] / [`EventBus`] — application-wide event broadcasting.
//! - [`ChannelMetricsStore`] — per-channel message and token metrics.
//! - [`StreamChunk`] / [`StreamId`] — streaming response primitives.
//! - [`YamlFileStore`] — generic YAML-backed persistent store.
//!
//! Most other crates depend on this crate; it must not depend on any of them.

mod error;
mod events;
mod health;
pub mod metrics;
mod naming;
mod platform;
mod plugin;
mod session;
pub mod streaming;
mod yaml_store;

pub use error::{YamlStoreError, is_transient_io_error};
pub use events::{AppEvent, AppEventKind, EventBus};
pub use health::{
    ComponentHealth, HealthComponents, HealthResponse, HealthStatus, ServiceProbeResponse,
};
pub use metrics::{ChannelMetricsSnapshot, ChannelMetricsStore};
pub use naming::sanitize_name;
pub use platform::Platform;
pub use plugin::PluginStatusSnapshot;
pub use session::SessionKey;
pub use streaming::{StreamChunk, StreamId, stream_channel};
pub use yaml_store::{YamlDefinition, YamlFileStore};

#[cfg(test)]
mod tests;
