mod mutation;
mod query;
mod shared;
mod validation;
mod view;

pub use mutation::{delete_schedule, save_schedule, toggle_schedule};
pub use query::load_schedules_page;
pub use shared::ScheduleSaveInput;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use opengoose_persistence::{Database, OrchestrationStore, RunStatus, ScheduleStore};
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore};

    use super::validation::normalize_input;
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

            toggle_schedule(db.clone(), "paused-schedule".into())
                .expect("first toggle should succeed");

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

            let page = load_schedules_page(db, Some(super::shared::NEW_SCHEDULE_KEY.into()))
                .expect("page should load");

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
