use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, PluginStore};
use opengoose_profiles::SkillStore;
use urlencoding::encode;

use super::PluginStatusFilter;
use super::detail::{build_plugin_detail, build_plugin_list_item, placeholder_plugin_detail};
use super::state::{PluginStatusCounts, build_plugin_state, collect_status_counts};
use crate::data::views::{Notice, PluginFilterItem, PluginsPageView};

pub(super) fn build_page(
    db: Arc<Database>,
    selected: Option<String>,
    filter: PluginStatusFilter,
    notice: Option<Notice>,
    install_source_path: String,
) -> Result<PluginsPageView> {
    let skill_store = SkillStore::new().ok();
    build_page_with_skill_store(
        db,
        selected,
        filter,
        notice,
        install_source_path,
        skill_store.as_ref(),
    )
}

pub(super) fn build_page_with_skill_store(
    db: Arc<Database>,
    selected: Option<String>,
    filter: PluginStatusFilter,
    notice: Option<Notice>,
    install_source_path: String,
    skill_store: Option<&SkillStore>,
) -> Result<PluginsPageView> {
    let states = PluginStore::new(db)
        .list()?
        .into_iter()
        .map(|plugin| build_plugin_state(plugin, skill_store))
        .collect::<Vec<_>>();
    let counts = collect_status_counts(&states);
    let filtered_states = states
        .iter()
        .filter(|state| filter.matches(state.bucket))
        .collect::<Vec<_>>();
    let selected_name = selected
        .filter(|target| {
            filtered_states
                .iter()
                .any(|state| state.plugin.name == *target)
        })
        .or_else(|| {
            filtered_states
                .first()
                .map(|state| state.plugin.name.clone())
        });
    let total_plugins = states.len();

    Ok(PluginsPageView {
        mode_label: if total_plugins == 0 {
            "No plugins installed".into()
        } else {
            format!(
                "{} operational · {} attention · {} disabled",
                counts.operational, counts.attention, counts.disabled
            )
        },
        mode_tone: if total_plugins == 0 {
            "neutral"
        } else if counts.attention > 0 {
            "amber"
        } else {
            "success"
        },
        filters: build_filter_items(counts, filter),
        plugins: filtered_states
            .iter()
            .map(|state| build_plugin_list_item(state, selected_name.as_deref(), filter))
            .collect(),
        selected: match selected_name.as_deref().and_then(|name| {
            filtered_states
                .iter()
                .find(|state| state.plugin.name == name)
        }) {
            Some(state) => build_plugin_detail(state, notice, install_source_path),
            None => placeholder_plugin_detail(notice, install_source_path, filter, total_plugins),
        },
    })
}

fn build_filter_items(
    counts: PluginStatusCounts,
    active_filter: PluginStatusFilter,
) -> Vec<PluginFilterItem> {
    [
        (
            PluginStatusFilter::All,
            counts.operational + counts.attention + counts.disabled,
        ),
        (PluginStatusFilter::Operational, counts.operational),
        (PluginStatusFilter::Attention, counts.attention),
        (PluginStatusFilter::Disabled, counts.disabled),
    ]
    .into_iter()
    .map(|(filter, count)| PluginFilterItem {
        label: filter.label().into(),
        count,
        tone: filter.tone(),
        page_url: plugins_page_url(None, filter),
        active: filter == active_filter,
    })
    .collect()
}

pub(super) fn plugins_page_url(selected: Option<&str>, filter: PluginStatusFilter) -> String {
    let mut query = Vec::new();
    if let Some(filter_value) = filter.query_value() {
        query.push(format!("status={}", encode(filter_value)));
    }
    if let Some(selected) = selected {
        query.push(format!("plugin={}", encode(selected)));
    }

    if query.is_empty() {
        "/plugins".into()
    } else {
        format!("/plugins?{}", query.join("&"))
    }
}
