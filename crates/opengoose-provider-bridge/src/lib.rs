//! Bridge between OpenGoose and the Goose AI provider system.
//!
//! Exposes a simplified view of Goose provider metadata ([`ProviderSummary`],
//! [`ConfigKeySummary`]) without pulling the full Goose dependency tree into
//! every crate. Also provides `list_providers` and `activate_provider`
//! helpers that configure secrets and launch the provider backend.

mod service;
#[cfg(test)]
mod tests;
mod types;

pub use service::GooseProviderService;
pub use types::{ConfigKeySummary, ProviderSummary};
