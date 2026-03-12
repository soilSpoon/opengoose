use super::*;

#[test]
fn build_duration_bars_scales_with_run_length() {
    let runs = vec![
        sample_run(
            "run-a",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:30:00",
        ),
        sample_run(
            "run-b",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:05:00",
        ),
    ];

    let bars = build_duration_bars(&runs);
    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0].value, "30m 0s");
    assert!(bars[0].height > bars[1].height);
}

#[test]
fn duration_stats_returns_placeholder_when_no_runs_exist() {
    let stats = duration_stats(&[]);

    assert_eq!(stats.average_label, None);
    assert_eq!(
        stats.note,
        "Run duration will appear once persisted timestamps accumulate."
    );
}

#[test]
fn duration_stats_reports_average_and_longest_durations() {
    let stats = duration_stats(&[
        sample_run(
            "run-a",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:05:00",
        ),
        sample_run(
            "run-b",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:15:00",
        ),
    ]);

    assert_eq!(stats.average_label.as_deref(), Some("10m 0s"));
    assert_eq!(stats.note, "2 captured runs · longest 15m 0s");
}

#[test]
fn build_duration_bars_uses_fixed_height_for_zero_length_runs() {
    let bars = build_duration_bars(&[sample_run(
        "run-a",
        RunStatus::Completed,
        "2026-03-10 10:00:00",
        "2026-03-10 10:00:00",
    )]);

    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].value, "0s");
    assert_eq!(bars[0].height, 34);
}

#[test]
fn build_duration_bars_skips_runs_with_invalid_timestamps() {
    let bars = build_duration_bars(&[
        sample_run("bad-run", RunStatus::Completed, "not-a-time", "also-bad"),
        sample_run(
            "good-run",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:01:00",
        ),
    ]);

    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].label, "team-good-run");
}

#[test]
fn load_dashboard_returns_mock_preview_for_empty_runtime() {
    let dashboard = load_dashboard(test_db()).unwrap();

    assert_eq!(dashboard.intro.mode_label, "Mock preview");
    assert_eq!(dashboard.intro.mode_tone, "neutral");
    assert_eq!(dashboard.sessions.len(), 2);
    assert_eq!(dashboard.runs.len(), 3);
    assert_eq!(dashboard.gateway_panel.cards.len(), 4);
    assert_eq!(dashboard.alerts[0].eyebrow, "Preview Mode");
}
