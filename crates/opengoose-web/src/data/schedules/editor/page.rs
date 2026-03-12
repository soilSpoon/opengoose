use anyhow::{Context, Result};
use urlencoding::encode;

use super::super::catalog::ScheduleCatalog;
use super::super::selection::{NEW_SCHEDULE_KEY, Selection};
use super::form::{ScheduleDraft, build_draft_detail, build_existing_detail, build_new_detail};
use crate::data::views::{Notice, ScheduleEditorView, ScheduleListItem, SchedulesPageView};

pub(in crate::data::schedules) fn build_page(
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

pub(in crate::data::schedules) fn build_error_page(
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

fn build_schedule_list_item(
    schedule: &opengoose_persistence::Schedule,
    active_name: Option<&str>,
) -> ScheduleListItem {
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
