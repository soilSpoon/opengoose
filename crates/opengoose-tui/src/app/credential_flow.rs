use anyhow::Result;
use opengoose_provider_bridge::GooseProviderService;
use opengoose_secrets::{ConfigFile, SecretKey};
use tokio::sync::oneshot;

use super::state::*;

mod flow;
mod persistence;
mod provider_selection;

#[cfg(test)]
mod tests;
