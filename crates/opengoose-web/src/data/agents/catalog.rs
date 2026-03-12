use anyhow::Result;
use opengoose_profiles::{AgentProfile, ProfileStore, all_defaults as default_profiles};
use urlencoding::encode;

use super::detail::capability_line;
use crate::data::utils::source_badge;
use crate::data::views::AgentListItem;

#[derive(Clone)]
pub(super) struct ProfileCatalogEntry {
    pub(super) profile: AgentProfile,
    pub(super) source_label: String,
    pub(super) is_live: bool,
}

pub(super) struct CatalogMode {
    pub(super) label: &'static str,
    pub(super) tone: &'static str,
}

pub(super) fn load_profiles_catalog() -> Result<Vec<ProfileCatalogEntry>> {
    let store = ProfileStore::new()?;
    let names = store.list()?;
    if names.is_empty() {
        return Ok(default_profiles()
            .into_iter()
            .map(|profile| ProfileCatalogEntry {
                profile,
                source_label: "Bundled default".into(),
                is_live: false,
            })
            .collect());
    }

    names
        .into_iter()
        .map(|name| {
            let profile = store.get(&name)?;
            Ok(ProfileCatalogEntry {
                profile,
                source_label: store.profile_path(&name),
                is_live: true,
            })
        })
        .collect()
}

pub(super) fn catalog_mode(entries: &[ProfileCatalogEntry]) -> CatalogMode {
    if entries.iter().all(|entry| !entry.is_live) {
        CatalogMode {
            label: "Bundled defaults",
            tone: "neutral",
        }
    } else {
        CatalogMode {
            label: "Installed catalog",
            tone: "success",
        }
    }
}

pub(super) fn build_agent_list_items(
    entries: &[ProfileCatalogEntry],
    selected_name: &str,
) -> Vec<AgentListItem> {
    entries
        .iter()
        .map(|entry| AgentListItem {
            title: entry.profile.title.clone(),
            subtitle: entry
                .profile
                .description
                .clone()
                .unwrap_or_else(|| "No profile description provided.".into()),
            capability: capability_line(&entry.profile),
            source_label: entry.source_label.clone(),
            source_badge: source_badge(&entry.source_label),
            page_url: format!("/agents?agent={}", encode(&entry.profile.title)),
            active: entry.profile.title == selected_name,
        })
        .collect()
}
