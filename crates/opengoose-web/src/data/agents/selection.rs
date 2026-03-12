use anyhow::{Context, Result};

use super::catalog::ProfileCatalogEntry;
use crate::data::utils::choose_selected_name;

pub(super) fn find_selected_entry(
    entries: &[ProfileCatalogEntry],
    selected: Option<String>,
) -> Result<&ProfileCatalogEntry> {
    let selected_name = choose_selected_name(
        entries
            .iter()
            .map(|entry| entry.profile.title.clone())
            .collect(),
        selected,
    );

    entries
        .iter()
        .find(|entry| entry.profile.title == selected_name)
        .context("selected agent missing")
}
