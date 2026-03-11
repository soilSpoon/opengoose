use std::sync::Arc;

use anyhow::{Context, Result};
use opengoose_persistence::{
    Database, OrchestrationRun, OrchestrationStore, Schedule, ScheduleStore, ScheduleUpdate,
};
use opengoose_teams::{TeamStore, scheduler};
use urlencoding::encode;

use crate::data::utils::{preview, run_tone};
use crate::data::views::{
    MetaRow, Notice, ScheduleEditorView, ScheduleHistoryItem, ScheduleListItem, SchedulesPageView,
    SelectOption,
};

const NEW_SCHEDULE_KEY: &str = "__new__";

pub struct ScheduleSaveInput {
    pub original_name: Option<String>,
    pub name: String,
    pub cron_expression: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
}

struct ScheduleCatalog {
    schedules: Vec<Schedule>,
    runs: Vec<OrchestrationRun>,
    installed_teams: Vec<String>,
}

enum Selection {
    Existing(String),
    New,
}

struct ScheduleDraft {
    original_name: Option<String>,
    name: String,
    cron_expression: String,
    team_name: String,
    input: String,
    enabled: bool,
}

/// Load the schedules page view-model, optionally selecting a schedule by name.
pub fn load_schedules_page(
    db: Arc<Database>,
    selected: Option<String>,
) -> Result<SchedulesPageView> {
    let catalog = load_catalog(db)?;
    let selection = resolve_selection(&catalog, selected);
    build_page(&catalog, selection, None)
}

/// Create or update a schedule and return the refreshed page view.
pub fn save_schedule(db: Arc<Database>, input: ScheduleSaveInput) -> Result<SchedulesPageView> {
    let catalog = load_catalog(db.clone())?;
    let draft = ScheduleDraft {
        original_name: input
            .original_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        name: input.name.trim().to_string(),
        cron_expression: input.cron_expression.trim().to_string(),
        team_name: input.team_name.trim().to_string(),
        input: normalize_input(input.input),
        enabled: input.enabled,
    };

    if draft.name.is_empty() {
        return build_error_page(&catalog, &draft, "Schedule name is required.");
    }
    if draft.cron_expression.is_empty() {
        return build_error_page(&catalog, &draft, "Cron expression is required.");
    }
    if draft.team_name.is_empty() {
        return build_error_page(&catalog, &draft, "Choose an installed team before saving.");
    }
    if !catalog
        .installed_teams
        .iter()
        .any(|team| team == &draft.team_name)
    {
        return build_error_page(
            &catalog,
            &draft,
            "The selected team is not installed. Save a team definition first.",
        );
    }
    if let Err(error) = scheduler::validate_cron(&draft.cron_expression) {
        return build_error_page(&catalog, &draft, error);
    }
    if let Some(original_name) = draft.original_name.as_ref()
        && draft.name != *original_name
    {
        return build_error_page(
            &catalog,
            &draft,
            "Schedule names are immutable. Create a new schedule instead.",
        );
    }

    let next_run_at = if draft.enabled {
        scheduler::next_fire_time(&draft.cron_expression)
    } else {
        None
    };
    let store = ScheduleStore::new(db.clone());

    let result = if let Some(original_name) = draft.original_name.as_ref() {
        store.update(
            original_name,
            ScheduleUpdate {
                name: &draft.name,
                cron_expression: &draft.cron_expression,
                team_name: &draft.team_name,
                input: &draft.input,
                enabled: draft.enabled,
                next_run_at: next_run_at.as_deref(),
            },
        )
    } else {
        store
            .create(
                &draft.name,
                &draft.cron_expression,
                &draft.team_name,
                &draft.input,
                next_run_at.as_deref(),
            )
            .map(|_| true)
    };

    match result {
        Ok(true) => {
            let mut page = load_schedules_page(db, Some(draft.name.clone()))?;
            page.selected.notice = Some(Notice {
                text: if draft.original_name.is_some() {
                    "Schedule saved.".into()
                } else {
                    "Schedule created.".into()
                },
                tone: "success",
            });
            Ok(page)
        }
        Ok(false) => build_error_page(
            &catalog,
            &draft,
            "The selected schedule no longer exists. Reload and try again.",
        ),
        Err(error) => build_error_page(&catalog, &draft, error.to_string()),
    }
}

/// Flip a schedule between enabled and paused states.
pub fn toggle_schedule(db: Arc<Database>, name: String) -> Result<SchedulesPageView> {
    let store = ScheduleStore::new(db.clone());
    let Some(schedule) = store.get_by_name(&name)? else {
        let mut page = load_schedules_page(db, Some(NEW_SCHEDULE_KEY.into()))?;
        page.selected.notice = Some(Notice {
            text: format!("Schedule `{name}` was not found."),
            tone: "danger",
        });
        return Ok(page);
    };

    let enabled = !schedule.enabled;
    if enabled && let Err(error) = scheduler::validate_cron(&schedule.cron_expression) {
        let mut page = load_schedules_page(db, Some(schedule.name.clone()))?;
        page.selected.notice = Some(Notice {
            text: error,
            tone: "danger",
        });
        return Ok(page);
    }

    let next_run_at = if enabled {
        scheduler::next_fire_time(&schedule.cron_expression)
    } else {
        None
    };
    store.update(
        &schedule.name,
        ScheduleUpdate {
            name: &schedule.name,
            cron_expression: &schedule.cron_expression,
            team_name: &schedule.team_name,
            input: &schedule.input,
            enabled,
            next_run_at: next_run_at.as_deref(),
        },
    )?;

    let mut page = load_schedules_page(db, Some(schedule.name.clone()))?;
    page.selected.notice = Some(Notice {
        text: if enabled {
            "Schedule enabled.".into()
        } else {
            "Schedule paused.".into()
        },
        tone: "success",
    });
    Ok(page)
}

/// Delete a schedule after explicit confirmation.
pub fn delete_schedule(
    db: Arc<Database>,
    name: String,
    confirmed: bool,
) -> Result<SchedulesPageView> {
    if !confirmed {
        let mut page = load_schedules_page(db, Some(name.clone()))?;
        page.selected.notice = Some(Notice {
            text: "Check the confirmation box before deleting a schedule.".into(),
            tone: "danger",
        });
        return Ok(page);
    }

    let removed = ScheduleStore::new(db.clone()).remove(&name)?;
    let mut page = load_schedules_page(db, None)?;
    page.selected.notice = Some(Notice {
        text: if removed {
            format!("Deleted schedule `{name}`.")
        } else {
            format!("Schedule `{name}` was already removed.")
        },
        tone: if removed { "success" } else { "danger" },
    });
    Ok(page)
}

fn load_catalog(db: Arc<Database>) -> Result<ScheduleCatalog> {
    let schedules = ScheduleStore::new(db.clone()).list()?;
    let runs = OrchestrationStore::new(db).list_runs(None, 200)?;
    let installed_teams = load_installed_team_names()?;
    Ok(ScheduleCatalog {
        schedules,
        runs,
        installed_teams,
    })
}

fn load_installed_team_names() -> Result<Vec<String>> {
    let mut names = TeamStore::new()?.list()?;
    names.sort();
    Ok(names)
}

fn resolve_selection(catalog: &ScheduleCatalog, selected: Option<String>) -> Selection {
    match selected.as_deref() {
        Some(NEW_SCHEDULE_KEY) => Selection::New,
        Some(target)
            if catalog
                .schedules
                .iter()
                .any(|schedule| schedule.name == target) =>
        {
            Selection::Existing(target.to_string())
        }
        _ => catalog
            .schedules
            .first()
            .map(|schedule| Selection::Existing(schedule.name.clone()))
            .unwrap_or(Selection::New),
    }
}

fn build_page(
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

fn build_error_page(
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

fn normalize_input(input: String) -> String {
    if input.trim().is_empty() {
        String::new()
    } else {
        input
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use opengoose_persistence::{Database, OrchestrationStore, RunStatus};
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore};

    use super::*;
    use crate::test_support::with_temp_home as with_shared_temp_home;

    fn with_temp_home(test: impl FnOnce()) {
        with_shared_temp_home("opengoose-schedules-home", test);
    }

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().expect("in-memory db should open"))
    }

    fn save_team(name: &str) {
        TeamStore::new()
            .expect("team store should open")
            .save(
                &TeamDefinition {
                    version: "1.0.0".into(),
                    title: name.into(),
                    description: Some(format!("{name} team")),
                    workflow: OrchestrationPattern::Chain,
                    agents: vec![TeamAgent {
                        profile: "tester".into(),
                        role: Some("validate setup".into()),
                    }],
                    router: None,
                    fan_out: None,
                },
                true,
            )
            .expect("team should save");
    }

    #[test]
    fn load_schedules_page_without_rows_selects_new_draft() {
        with_temp_home(|| {
            save_team("ops");

            let page = load_schedules_page(test_db(), None).expect("page should load");

            assert!(page.schedules.is_empty());
            assert!(page.selected.is_new);
            assert_eq!(page.selected.title, "Create schedule");
            assert_eq!(page.selected.team_options.len(), 1);
        });
    }

    #[test]
    fn save_schedule_creates_a_new_schedule() {
        with_temp_home(|| {
            save_team("ops");

            let page = save_schedule(
                test_db(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "nightly-ops".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("save should succeed");

            assert_eq!(page.schedules.len(), 1);
            assert_eq!(page.selected.name, "nightly-ops");
            assert!(page.selected.enabled);
            assert_eq!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|notice| notice.text.as_str()),
                Some("Schedule created.")
            );
        });
    }

    #[test]
    fn save_schedule_rejects_invalid_cron_and_preserves_draft() {
        with_temp_home(|| {
            save_team("ops");

            let page = save_schedule(
                test_db(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "broken".into(),
                    cron_expression: "not-a-cron".into(),
                    team_name: "ops".into(),
                    input: "ship it".into(),
                    enabled: true,
                },
            )
            .expect("invalid cron should return a draft page");

            assert!(page.schedules.is_empty());
            assert!(page.selected.is_new);
            assert_eq!(page.selected.name, "broken");
            assert_eq!(page.selected.cron_expression, "not-a-cron");
            assert_eq!(
                page.selected.notice.as_ref().map(|notice| notice.tone),
                Some("danger")
            );
        });
    }

    #[test]
    fn toggle_schedule_flips_enabled_state() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();
            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "nightly-ops".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed schedule should save");

            let page = toggle_schedule(db, "nightly-ops".into()).expect("toggle should succeed");

            assert!(!page.selected.enabled);
            assert_eq!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|notice| notice.text.as_str()),
                Some("Schedule paused.")
            );
        });
    }

    #[test]
    fn delete_schedule_requires_confirmation() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();
            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "nightly-ops".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed schedule should save");

            let page =
                delete_schedule(db, "nightly-ops".into(), false).expect("delete should render");

            assert_eq!(page.schedules.len(), 1);
            assert_eq!(
                page.selected.notice.as_ref().map(|notice| notice.tone),
                Some("danger")
            );
        });
    }

    #[test]
    fn load_schedules_page_builds_history_from_matching_runs() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();
            ScheduleStore::new(db.clone())
                .create(
                    "nightly-ops",
                    "0 0 * * * *",
                    "ops",
                    "",
                    Some("2026-03-11 00:00:00"),
                )
                .expect("schedule should seed");
            OrchestrationStore::new(db.clone())
                .create_run(
                    "run-1",
                    "session-1",
                    "ops",
                    "chain",
                    "Scheduled run: nightly-ops",
                    1,
                )
                .expect("run should seed");
            OrchestrationStore::new(db.clone())
                .complete_run("run-1", "done")
                .expect("run should complete");

            let page =
                load_schedules_page(db, Some("nightly-ops".into())).expect("page should load");

            assert_eq!(page.selected.history.len(), 1);
            assert_eq!(page.selected.history[0].title, "run-1");
            assert_eq!(
                page.selected.history[0].status_label,
                RunStatus::Completed.as_str()
            );
        });
    }

    #[test]
    fn save_schedule_rejects_empty_name() {
        with_temp_home(|| {
            save_team("ops");

            let page = save_schedule(
                test_db(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "   ".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("should return error page");

            assert!(page.schedules.is_empty());
            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.tone),
                Some("danger")
            );
            assert!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|n| n.text.contains("name"))
                    .unwrap_or(false)
            );
        });
    }

    #[test]
    fn save_schedule_rejects_empty_cron_expression() {
        with_temp_home(|| {
            save_team("ops");

            let page = save_schedule(
                test_db(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "my-schedule".into(),
                    cron_expression: "  ".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("should return error page");

            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.tone),
                Some("danger")
            );
            assert!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|n| n.text.contains("Cron"))
                    .unwrap_or(false)
            );
        });
    }

    #[test]
    fn save_schedule_rejects_empty_team_name() {
        with_temp_home(|| {
            save_team("ops");

            let page = save_schedule(
                test_db(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "my-schedule".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "  ".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("should return error page");

            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.tone),
                Some("danger")
            );
            assert!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|n| n.text.contains("team"))
                    .unwrap_or(false)
            );
        });
    }

    #[test]
    fn save_schedule_rejects_uninstalled_team() {
        with_temp_home(|| {
            save_team("ops");

            let page = save_schedule(
                test_db(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "my-schedule".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ghost-team".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("should return error page");

            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.tone),
                Some("danger")
            );
            assert!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|n| n.text.contains("not installed"))
                    .unwrap_or(false)
            );
        });
    }

    #[test]
    fn save_schedule_rejects_name_mutation_on_existing() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "original-name".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed should succeed");

            let page = save_schedule(
                db,
                ScheduleSaveInput {
                    original_name: Some("original-name".into()),
                    name: "renamed-name".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("should return error page");

            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.tone),
                Some("danger")
            );
            assert!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|n| n.text.contains("immutable"))
                    .unwrap_or(false)
            );
        });
    }

    #[test]
    fn save_schedule_updates_existing_when_disabled() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "nightly-ops".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed should succeed");

            let page = save_schedule(
                db,
                ScheduleSaveInput {
                    original_name: Some("nightly-ops".into()),
                    name: "nightly-ops".into(),
                    cron_expression: "0 12 * * * *".into(),
                    team_name: "ops".into(),
                    input: "updated input".into(),
                    enabled: false,
                },
            )
            .expect("update should succeed");

            assert_eq!(page.selected.name, "nightly-ops");
            assert!(!page.selected.enabled);
            assert_eq!(page.selected.cron_expression, "0 12 * * * *");
            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.text.as_str()),
                Some("Schedule saved.")
            );
        });
    }

    #[test]
    fn toggle_schedule_enables_paused_schedule() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "paused-schedule".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed should succeed");

            // First toggle: enabled → paused.
            toggle_schedule(db.clone(), "paused-schedule".into())
                .expect("first toggle should succeed");

            // Second toggle: paused → enabled.
            let page = toggle_schedule(db, "paused-schedule".into())
                .expect("second toggle should succeed");

            assert!(page.selected.enabled);
            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.text.as_str()),
                Some("Schedule enabled.")
            );
        });
    }

    #[test]
    fn toggle_schedule_returns_danger_notice_for_missing_schedule() {
        with_temp_home(|| {
            save_team("ops");

            let page =
                toggle_schedule(test_db(), "nonexistent".into()).expect("should render error page");

            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.tone),
                Some("danger")
            );
            assert!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|n| n.text.contains("nonexistent"))
                    .unwrap_or(false)
            );
        });
    }

    #[test]
    fn delete_schedule_removes_with_confirmation() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "to-delete".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed should succeed");

            let page =
                delete_schedule(db, "to-delete".into(), true).expect("delete should succeed");

            assert!(page.schedules.is_empty());
            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.tone),
                Some("success")
            );
            assert!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|n| n.text.contains("to-delete"))
                    .unwrap_or(false)
            );
        });
    }

    #[test]
    fn delete_schedule_handles_already_removed_schedule() {
        with_temp_home(|| {
            save_team("ops");

            let page =
                delete_schedule(test_db(), "ghost".into(), true).expect("delete should render");

            assert_eq!(
                page.selected.notice.as_ref().map(|n| n.tone),
                Some("danger")
            );
            assert!(
                page.selected
                    .notice
                    .as_ref()
                    .map(|n| n.text.contains("ghost"))
                    .unwrap_or(false)
            );
        });
    }

    #[test]
    fn load_schedules_page_auto_selects_first_existing_schedule() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "alpha".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed should succeed");

            let page = load_schedules_page(db, None).expect("page should load");

            assert!(!page.selected.is_new);
            assert_eq!(page.selected.name, "alpha");
        });
    }

    #[test]
    fn load_schedules_page_selects_new_draft_when_new_key_passed() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "existing".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed should succeed");

            let page =
                load_schedules_page(db, Some(NEW_SCHEDULE_KEY.into())).expect("page should load");

            assert!(page.selected.is_new);
            assert_eq!(page.selected.title, "Create schedule");
        });
    }

    #[test]
    fn mode_label_reflects_enabled_and_total_counts() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "active-one".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("first schedule should save");

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "paused-one".into(),
                    cron_expression: "0 6 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("second schedule should save");

            // Disable the second schedule via toggle (create always starts enabled).
            toggle_schedule(db.clone(), "paused-one".into()).expect("toggle should succeed");

            let page = load_schedules_page(db, None).expect("page should load");

            assert_eq!(page.mode_label, "1 active of 2");
            assert_eq!(page.mode_tone, "success");
        });
    }

    #[test]
    fn mode_tone_is_amber_when_all_schedules_paused() {
        with_temp_home(|| {
            save_team("ops");
            let db = test_db();

            save_schedule(
                db.clone(),
                ScheduleSaveInput {
                    original_name: None,
                    name: "paused".into(),
                    cron_expression: "0 0 * * * *".into(),
                    team_name: "ops".into(),
                    input: String::new(),
                    enabled: true,
                },
            )
            .expect("seed should save");

            // Toggle off so the only schedule is paused.
            toggle_schedule(db.clone(), "paused".into()).expect("toggle should succeed");

            let page = load_schedules_page(db, None).expect("page should load");

            assert_eq!(page.mode_label, "0 active of 1");
            assert_eq!(page.mode_tone, "amber");
        });
    }

    #[test]
    fn normalize_input_returns_empty_for_whitespace_only() {
        assert_eq!(normalize_input("   ".into()), "");
        assert_eq!(normalize_input("\t\n".into()), "");
        assert_eq!(normalize_input(String::new()), "");
    }

    #[test]
    fn normalize_input_preserves_non_empty_content() {
        assert_eq!(normalize_input("hello world".into()), "hello world");
        assert_eq!(normalize_input("  leading space".into()), "  leading space");
    }
}
