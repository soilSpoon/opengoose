use opengoose_persistence::{OrchestrationRun, Schedule};
use opengoose_teams::scheduler;
use urlencoding::encode;

use super::super::catalog::ScheduleCatalog;
use super::options::build_team_options;
use crate::data::utils::{preview, run_tone};
use crate::data::views::{MetaRow, Notice, ScheduleEditorView, ScheduleHistoryItem};

pub(in crate::data::schedules) struct ScheduleDraft {
    pub(in crate::data::schedules) original_name: Option<String>,
    pub(in crate::data::schedules) name: String,
    pub(in crate::data::schedules) cron_expression: String,
    pub(in crate::data::schedules) team_name: String,
    pub(in crate::data::schedules) input: String,
    pub(in crate::data::schedules) enabled: bool,
}

pub(super) fn build_existing_detail(
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

pub(super) fn build_new_detail(
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

pub(super) fn build_draft_detail(
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

#[cfg(test)]
mod tests {
    use opengoose_persistence::RunStatus;

    use super::super::super::catalog::ScheduleCatalog;
    use super::*;

    #[test]
    fn build_history_uses_effective_schedule_input_and_limits_matches() {
        let schedule = sample_schedule();
        let mut runs = (0..10)
            .map(|index| sample_run(&format!("run-{index}"), "ops", "Scheduled run: nightly-ops"))
            .collect::<Vec<_>>();
        runs.push(sample_run(
            "wrong-team",
            "infra",
            "Scheduled run: nightly-ops",
        ));
        runs.push(sample_run("wrong-input", "ops", "different input"));

        let history = build_history(&schedule, &runs);

        assert_eq!(history.len(), 8);
        assert_eq!(history[0].title, "run-0");
        assert_eq!(history[7].title, "run-7");
    }

    #[test]
    fn build_draft_detail_falls_back_to_new_detail_for_missing_original_schedule() {
        let catalog = ScheduleCatalog {
            schedules: vec![],
            runs: vec![],
            installed_teams: vec!["ops".into()],
        };
        let detail = build_draft_detail(
            &catalog,
            &ScheduleDraft {
                original_name: Some("missing".into()),
                name: "missing".into(),
                cron_expression: "0 0 * * * *".into(),
                team_name: "ops".into(),
                input: String::new(),
                enabled: true,
            },
            None,
        );

        assert!(detail.is_new);
        assert_eq!(detail.title, "Create schedule");
        assert_eq!(detail.team_name, "ops");
    }

    fn sample_schedule() -> Schedule {
        Schedule {
            id: 1,
            name: "nightly-ops".into(),
            cron_expression: "0 0 * * * *".into(),
            team_name: "ops".into(),
            input: String::new(),
            enabled: true,
            last_run_at: None,
            next_run_at: Some("2026-03-12 00:00:00".into()),
            created_at: "2026-03-11 00:00:00".into(),
            updated_at: "2026-03-11 00:00:00".into(),
        }
    }

    fn sample_run(id: &str, team_name: &str, input: &str) -> OrchestrationRun {
        OrchestrationRun {
            team_run_id: id.into(),
            session_key: format!("session-{id}"),
            team_name: team_name.into(),
            workflow: "chain".into(),
            input: input.into(),
            status: RunStatus::Completed,
            current_step: 1,
            total_steps: 1,
            result: None,
            created_at: "2026-03-11 00:00:00".into(),
            updated_at: "2026-03-11 00:00:00".into(),
        }
    }
}
