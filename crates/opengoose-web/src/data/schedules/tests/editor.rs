use super::*;

#[test]
fn save_schedule_creates_a_new_schedule() {
    with_temp_home(|| {
        save_team("ops");

        let page = save_schedule(test_db(), new_schedule_input("nightly-ops"))
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
                cron_expression: "not-a-cron".into(),
                input: "ship it".into(),
                ..new_schedule_input("broken")
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
fn save_schedule_rejects_empty_name() {
    with_temp_home(|| {
        save_team("ops");

        let page = save_schedule(
            test_db(),
            ScheduleSaveInput {
                name: "   ".into(),
                ..new_schedule_input("ignored")
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
                cron_expression: "  ".into(),
                ..new_schedule_input("my-schedule")
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
                team_name: "  ".into(),
                ..new_schedule_input("my-schedule")
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
                team_name: "ghost-team".into(),
                ..new_schedule_input("my-schedule")
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
        seed_schedule(db.clone(), "original-name");

        let page = save_schedule(
            db,
            ScheduleSaveInput {
                original_name: Some("original-name".into()),
                name: "renamed-name".into(),
                ..new_schedule_input("renamed-name")
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
        seed_schedule(db.clone(), "nightly-ops");

        let page = save_schedule(
            db,
            ScheduleSaveInput {
                original_name: Some("nightly-ops".into()),
                cron_expression: "0 12 * * * *".into(),
                input: "updated input".into(),
                enabled: false,
                ..new_schedule_input("nightly-ops")
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
fn normalize_input_returns_empty_for_whitespace_only() {
    assert_eq!(normalize_input("   ".into(), MAX_SCHEDULE_INPUT_BYTES), "");
    assert_eq!(normalize_input("\t\n".into(), MAX_SCHEDULE_INPUT_BYTES), "");
    assert_eq!(normalize_input(String::new(), MAX_SCHEDULE_INPUT_BYTES), "");
}

#[test]
fn normalize_input_preserves_non_empty_content() {
    assert_eq!(
        normalize_input("hello world".into(), MAX_SCHEDULE_INPUT_BYTES),
        "hello world"
    );
    assert_eq!(
        normalize_input("  leading space".into(), MAX_SCHEDULE_INPUT_BYTES),
        "  leading space"
    );
}

#[test]
fn normalize_input_caps_allocation_size() {
    let input = "1234567890".repeat(1_024);
    let normalized = normalize_input(input, 256);
    assert_eq!(normalized.len(), 256);
}

#[test]
fn save_schedule_rejects_overlong_name() {
    with_shared_temp_home("schedule-too-long-name", || {
        let page = save_schedule(
            test_db(),
            ScheduleSaveInput {
                name: "a".repeat(MAX_SCHEDULE_NAME_BYTES + 1),
                team_name: "feature-dev".into(),
                ..new_schedule_input("placeholder")
            },
        )
        .expect("page should render validation error");

        assert_eq!(
            page.selected.notice.expect("validation notice").text,
            format!(
                "Schedule name must be {} bytes or less.",
                MAX_SCHEDULE_NAME_BYTES
            )
        );
    });
}
