use opengoose_persistence::Plugin;
use opengoose_profiles::SkillStore;
use opengoose_teams::plugin::plugin_status_snapshot;
use opengoose_types::PluginStatusSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatusFilter {
    All,
    Operational,
    Attention,
    Disabled,
}

impl PluginStatusFilter {
    pub fn from_query(value: Option<&str>) -> Self {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            Some("operational") => Self::Operational,
            Some("attention") => Self::Attention,
            Some("disabled") => Self::Disabled,
            _ => Self::All,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Operational => "Operational",
            Self::Attention => "Attention",
            Self::Disabled => "Disabled",
        }
    }

    pub(super) fn tone(self) -> &'static str {
        match self {
            Self::All => "neutral",
            Self::Operational => "success",
            Self::Attention => "amber",
            Self::Disabled => "neutral",
        }
    }

    pub(super) fn query_value(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Operational => Some("operational"),
            Self::Attention => Some("attention"),
            Self::Disabled => Some("disabled"),
        }
    }

    pub(super) fn matches(self, bucket: PluginStatusBucket) -> bool {
        match self {
            Self::All => true,
            Self::Operational => bucket == PluginStatusBucket::Operational,
            Self::Attention => bucket == PluginStatusBucket::Attention,
            Self::Disabled => bucket == PluginStatusBucket::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PluginStatusBucket {
    Operational,
    Attention,
    Disabled,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PluginStatusCounts {
    pub(super) operational: usize,
    pub(super) attention: usize,
    pub(super) disabled: usize,
}

#[derive(Clone)]
pub(super) struct PluginState {
    pub(super) plugin: Plugin,
    pub(super) snapshot: PluginStatusSnapshot,
    pub(super) bucket: PluginStatusBucket,
    pub(super) status_label: String,
    pub(super) status_tone: &'static str,
    pub(super) lifecycle_label: String,
    pub(super) lifecycle_tone: &'static str,
    pub(super) runtime_label: String,
    pub(super) runtime_tone: &'static str,
    pub(super) status_summary: String,
}

pub(super) fn build_plugin_state(plugin: Plugin, skill_store: Option<&SkillStore>) -> PluginState {
    let snapshot = plugin_status_snapshot(&plugin, skill_store);
    let requires_runtime = snapshot
        .capabilities
        .iter()
        .any(|capability| capability == "skill" || capability == "channel_adapter");
    let lifecycle_label = if plugin.enabled {
        "Enabled"
    } else {
        "Disabled"
    }
    .to_string();
    let lifecycle_tone = if plugin.enabled { "sage" } else { "neutral" };
    let runtime_note = snapshot.runtime_note.clone().unwrap_or_default();

    let (bucket, status_label, status_tone, runtime_label, runtime_tone, status_summary) =
        if !plugin.enabled {
            (
                PluginStatusBucket::Disabled,
                "Disabled".to_string(),
                "neutral",
                "Runtime paused".to_string(),
                "neutral",
                "Runtime checks pause while the plugin is disabled.".to_string(),
            )
        } else if snapshot.runtime_initialized {
            (
                PluginStatusBucket::Operational,
                "Ready".to_string(),
                "success",
                "Runtime initialized".to_string(),
                "success",
                if snapshot.registered_skills.is_empty() {
                    "Declared runtime capabilities are available.".to_string()
                } else {
                    format!(
                        "{} declared skill(s) are registered in the active runtime.",
                        snapshot.registered_skills.len()
                    )
                },
            )
        } else if !requires_runtime {
            (
                PluginStatusBucket::Operational,
                "Installed".to_string(),
                "cyan",
                "No runtime required".to_string(),
                "cyan",
                "This plugin does not declare a live runtime capability.".to_string(),
            )
        } else if !snapshot.missing_skills.is_empty() {
            (
                PluginStatusBucket::Attention,
                "Missing skills".to_string(),
                "danger",
                format!("{} skill(s) missing", snapshot.missing_skills.len()),
                "danger",
                format!(
                    "{} declared skill(s) are missing from the active runtime.",
                    snapshot.missing_skills.len()
                ),
            )
        } else if runtime_note.contains("manifest unavailable") {
            (
                PluginStatusBucket::Attention,
                "Manifest missing".to_string(),
                "danger",
                "Manifest unavailable".to_string(),
                "danger",
                "The plugin manifest could not be loaded from disk.".to_string(),
            )
        } else if runtime_note.contains("channel adapter runtime loading is not implemented yet") {
            (
                PluginStatusBucket::Attention,
                "Adapter pending".to_string(),
                "amber",
                "Channel adapter pending".to_string(),
                "amber",
                "Channel adapter loading is not wired into the runtime yet.".to_string(),
            )
        } else if runtime_note.contains("skill store unavailable") {
            (
                PluginStatusBucket::Attention,
                "Runtime unknown".to_string(),
                "amber",
                "Skill store unavailable".to_string(),
                "amber",
                "The active skill store could not be loaded for verification.".to_string(),
            )
        } else {
            (
                PluginStatusBucket::Attention,
                "Needs attention".to_string(),
                "amber",
                "Runtime pending".to_string(),
                "amber",
                if runtime_note.is_empty() {
                    "Runtime initialization still needs operator attention.".to_string()
                } else {
                    runtime_note.clone()
                },
            )
        };

    PluginState {
        plugin,
        snapshot,
        bucket,
        status_label,
        status_tone,
        lifecycle_label,
        lifecycle_tone,
        runtime_label,
        runtime_tone,
        status_summary,
    }
}

pub(super) fn collect_status_counts(states: &[PluginState]) -> PluginStatusCounts {
    states
        .iter()
        .fold(PluginStatusCounts::default(), |mut counts, state| {
            match state.bucket {
                PluginStatusBucket::Operational => counts.operational += 1,
                PluginStatusBucket::Attention => counts.attention += 1,
                PluginStatusBucket::Disabled => counts.disabled += 1,
            }
            counts
        })
}

pub(super) fn build_plugin_search_text(state: &PluginState) -> String {
    let capabilities = state.snapshot.capabilities.join(" ");
    let registered_skills = state.snapshot.registered_skills.join(" ");
    let missing_skills = state.snapshot.missing_skills.join(" ");

    [
        state.status_summary.as_str(),
        state.snapshot.runtime_note.as_deref().unwrap_or_default(),
        capabilities.as_str(),
        registered_skills.as_str(),
        missing_skills.as_str(),
    ]
    .into_iter()
    .filter(|segment| !segment.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}
