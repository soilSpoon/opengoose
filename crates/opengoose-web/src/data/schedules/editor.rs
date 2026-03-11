use anyhow::{Context, Result};
use opengoose_persistence::{OrchestrationRun, Schedule};
use opengoose_teams::scheduler;
use urlencoding::encode;

use super::catalog::ScheduleCatalog;
use super::selection::{NEW_SCHEDULE_KEY, Selection};
use crate::data::utils::{preview, run_tone};
use crate::data::views::{
    MetaRow, Notice, ScheduleEditorView, ScheduleHistoryItem, ScheduleListItem, SchedulesPageView,
    SelectOption,
};

pub(super) struct ScheduleDraft {
    pub(super) original_name: Option<String>,
    pub(super) name: String,
    pub(super) cron_expression: String,
    pub(super) team_name: String,
    pub(super) input: String,
    pub(super) enabled: bool,
}

pub(super) fn build_page(
    catalog: &ScheduleCatalog,
    selection: Selection,
    detail_override: Option<ScheduleEditorView>,
) -> Result<SchedulesPageView> {
    let total = catalog.schedules.len();
    let enabled = catalog
        .schedules
        .iter()
        .filter(|schedule| schedule.enabled)
        .count();
    let active_name = match &selection {
        Selection::Existing(name) => Some(name.as_str()),
        Selection::New => None,
    };

    Ok(SchedulesPageView {
        mode_label: if total == 0 {
            "Ready for first schedule".into()
        } else {
            format!("{enabled} active of {total}")
        },
        mode_tone: if total == 0 {
            "neutral"
        } else if enabled > 0 {
            "success"
        } else {
            "amber"
        },
        schedules: catalog
            .schedules
            .iter()
            .map(|schedule| build_schedule_list_item(schedule, active_name))
            .collect(),
        selected: if let Some(detail) = detail_override {
            detail
        } else {
            match selection {
                Selection::Existing(name) => build_existing_detail(
                    catalog
                        .schedules
                        .iter()
                        .find(|schedule| schedule.name == name)
                        .context("selected schedule missing")?,
                    catalog,
                    None,
                ),
                Selection::New => build_new_detail(catalog, None, None),
            }
        },
        new_schedule_url: format!("/schedules?schedule={NEW_SCHEDULE_KEY}"),
    })
}

pub(super) fn build_error_page(
    catalog: &ScheduleCatalog,
    draft: &ScheduleDraft,
    message: impl Into<String>,
) -> Result<SchedulesPageView> {
    let selection = draft
        .original_name
        .as_ref()
        .map(|name| Selection::Existing(name.clone()))
        .unwrap_or(Selection::New);
    let detail = build_draft_detail(
        catalog,
        draft,
        Some(Notice {
            text: message.into(),
            tone: "danger",
        }),
    );
    build_page(catalog, selection, Some(detail))
}

fn build_schedule_list_item(schedule: &Schedule, active_name: Option<&str>) -> ScheduleListItem {
    let next_label = schedule
        .next_run_at
        .as_deref()
        .map(|value| format!("Next {value}"))
        .unwrap_or_else(|| {
            if schedule.enabled {
                "Next fire pending".into()
            } else {
                "Paused".into()
            }
        });
    ScheduleListItem {
        title: schedule.name.clone(),
        subtitle: format!(
            "{} · {}",
            schedule.team_name,
            if schedule.input.is_empty() {
                "default input"
            } else {
                "custom input"
            }
        ),
        preview: format!("{} · {}", schedule.cron_expression, next_label),
        source_label: schedule
            .last_run_at
            .as_deref()
            .map(|value| format!("Last {value}"))
            .unwrap_or_else(|| "Never run".into()),
        status_label: if schedule.enabled {
            "Enabled".into()
        } else {
            "Paused".into()
        },
        status_tone: if schedule.enabled { "sage" } else { "neutral" },
        page_url: format!("/schedules?schedule={}", encode(&schedule.name)),
        active: active_name
            .map(|name| name == schedule.name.as_str())
            .unwrap_or(false),
    }
}

fn build_existing_detail(
    schedule: &Schedule,
    catalog: &ScheduleCatalog,
    notice: Option<Notice>,
) -> ScheduleEditorView {
    let history = build_history(schedule, &catalog.runs);
    ScheduleEditorView {
        title: schedule.name.clone(),
        subtitle: "Adjust cadence, target team, and run input without leaving the dashboard."
            .into(),
        source_label: "Live schedule store".into(),
        original_name: schedule.name.clone(),
        name: schedule.name.clone(),
        cron_expression: schedule.cron_expression.clone(),
        team_name: schedule.team_name.clone(),
        input: schedule.input.clone(),
        enabled: schedule.enabled,
        is_new: false,
        name_locked: true,
        meta: vec![
            MetaRow {
                label: "Team".into(),
                value: schedule.team_name.clone(),
            },
            MetaRow {
                label: "Status".into(),
                value: if schedule.enabled {
                    "Enabled".into()
                } else {
                    "Paused".into()
                },
            },
            MetaRow {
                label: "Effective input".into(),
                value: preview(&effective_schedule_input(schedule), 72),
            },
            MetaRow {
                label: "Last run".into(),
                value: schedule
                    .last_run_at
                    .clone()
                    .unwrap_or_else(|| "No executions recorded.".into()),
            },
            MetaRow {
                label: "Next fire".into(),
                value: schedule
                    .next_run_at
                    .clone()
                    .unwrap_or_else(|| "Not scheduled".into()),
            },
            MetaRow {
                label: "Updated".into(),
                value: schedule.updated_at.clone(),
            },
        ],
        team_options: build_team_options(&catalog.installed_teams, Some(&schedule.team_name)),
        history,
        history_hint: "No matching runs found for this schedule yet.".into(),
        notice,
        save_label: "Save changes".into(),
        toggle_label: if schedule.enabled {
            "Pause schedule".into()
        } else {
            "Enable schedule".into()
        },
        delete_label: schedule.name.clone(),
    }
}

fn build_new_detail(
    catalog: &ScheduleCatalog,
    draft: Option<&ScheduleDraft>,
    notice: Option<Notice>,
) -> ScheduleEditorView {
    let selected_team = draft
        .map(|value| value.team_name.clone())
        .filter(|value| !value.is_empty())
        .or_else(|| catalog.installed_teams.first().cloned())
        .unwrap_or_default();

    ScheduleEditorView {
        title: "Create schedule".into(),
        subtitle: "Define a cron automation for an installed team and verify its next fire time."
            .into(),
        source_label: "New draft".into(),
        original_name: String::new(),
        name: draft.map(|value| value.name.clone()).unwrap_or_default(),
        cron_expression: draft
            .map(|value| value.cron_expression.clone())
            .unwrap_or_else(|| "0 0 * * * *".into()),
        team_name: selected_team.clone(),
        input: draft.map(|value| value.input.clone()).unwrap_or_default(),
        enabled: draft.map(|value| value.enabled).unwrap_or(true),
        is_new: true,
        name_locked: false,
        meta: vec![
            MetaRow {
                label: "Installed teams".into(),
                value: catalog.installed_teams.len().to_string(),
            },
            MetaRow {
                label: "Existing schedules".into(),
                value: catalog.schedules.len().to_string(),
            },
            MetaRow {
                label: "Cron syntax".into(),
                value: "sec min hour day month weekday".into(),
            },
            MetaRow {
                label: "First fire".into(),
                value: "Computed when the schedule is saved.".into(),
            },
        ],
        team_options: build_team_options(&catalog.installed_teams, Some(&selected_team)),
        history: vec![],
        history_hint: "Create this schedule to see recent matching runs.".into(),
        notice,
        save_label: "Create schedule".into(),
        toggle_label: "Enable schedule".into(),
        delete_label: String::new(),
    }
}

fn build_draft_detail(
    catalog: &ScheduleCatalog,
    draft: &ScheduleDraft,
    notice: Option<Notice>,
) -> ScheduleEditorView {
    if let Some(original_name) = draft.original_name.as_ref()
        && let Some(schedule) = catalog
            .schedules
            .iter()
            .find(|schedule| schedule.name == *original_name)
    {
        let history = build_history(schedule, &catalog.runs);
        return ScheduleEditorView {
            title: schedule.name.clone(),
            subtitle: "Fix the validation error and save again.".into(),
            source_label: "Draft changes".into(),
            original_name: schedule.name.clone(),
            name: schedule.name.clone(),
            cron_expression: draft.cron_expression.clone(),
            team_name: draft.team_name.clone(),
            input: draft.input.clone(),
            enabled: draft.enabled,
            is_new: false,
            name_locked: true,
            meta: vec![
                MetaRow {
                    label: "Team".into(),
                    value: draft.team_name.clone(),
                },
                MetaRow {
                    label: "Status".into(),
                    value: if draft.enabled {
                        "Enabled".into()
                    } else {
                        "Paused".into()
                    },
                },
                MetaRow {
                    label: "Last run".into(),
                    value: schedule
                        .last_run_at
                        .clone()
                        .unwrap_or_else(|| "No executions recorded.".into()),
                },
                MetaRow {
                    label: "Next fire".into(),
                    value: if draft.enabled {
                        scheduler::next_fire_time(&draft.cron_expression).unwrap_or_else(|| {
                            "Fix the cron expression to compute the next fire.".into()
                        })
                    } else {
                        "Not scheduled".into()
                    },
                },
            ],
            team_options: build_team_options(&catalog.installed_teams, Some(&draft.team_name)),
            history,
            history_hint: "No matching runs found for this schedule yet.".into(),
            notice,
            save_label: "Save changes".into(),
            toggle_label: if draft.enabled {
                "Pause schedule".into()
            } else {
                "Enable schedule".into()
            },
            delete_label: schedule.name.clone(),
        };
    }

    build_new_detail(catalog, Some(draft), notice)
}

fn build_team_options(
    installed_teams: &[String],
    selected_team: Option<&str>,
) -> Vec<SelectOption> {
    let mut names = installed_teams.to_vec();
    if let Some(selected_team) = selected_team
        && !selected_team.is_empty()
        && !names.iter().any(|team| team == selected_team)
    {
        names.push(selected_team.to_string());
        names.sort();
    }

    names
        .into_iter()
        .map(|team| SelectOption {
            selected: selected_team
                .map(|selected| selected == team.as_str())
                .unwrap_or(false),
            label: team.clone(),
            value: team,
        })
        .collect()
}

fn build_history(schedule: &Schedule, runs: &[OrchestrationRun]) -> Vec<ScheduleHistoryItem> {
    let expected_input = effective_schedule_input(schedule);
    runs.iter()
        .filter(|run| run.team_name == schedule.team_name && run.input == expected_input)
        .take(8)
        .map(|run| ScheduleHistoryItem {
            title: run.team_run_id.clone(),
            detail: format!("{} workflow · {}", run.workflow, preview(&run.input, 60)),
            updated_at: run.updated_at.clone(),
            status_label: run.status.as_str().replace('_', " "),
            status_tone: run_tone(&run.status),
            page_url: format!("/runs?run={}", encode(&run.team_run_id)),
        })
        .collect()
}

fn effective_schedule_input(schedule: &Schedule) -> String {
    if schedule.input.is_empty() {
        format!("Scheduled run: {}", schedule.name)
    } else {
        schedule.input.clone()
    }
}

pub(super) fn normalize_input(input: String, max_bytes: usize) -> String {
    if input.trim().is_empty() {
        String::new()
    } else {
        truncate_to_byte_boundary(&input, max_bytes)
    }
}

pub(super) fn normalize_trimmed_field(value: &str, max_bytes: usize) -> String {
    truncate_to_byte_boundary(value.trim(), max_bytes)
}

pub(super) fn normalize_optional_field(value: Option<&str>, max_bytes: usize) -> Option<String> {
    value
        .map(|item| normalize_trimmed_field(item, max_bytes))
        .filter(|item| !item.is_empty())
}

pub(super) fn trimmed_len_exceeds(value: &str, max_bytes: usize) -> bool {
    value.trim().len() > max_bytes
}

fn truncate_to_byte_boundary(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }

    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
