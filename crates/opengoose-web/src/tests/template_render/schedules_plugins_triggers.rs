use super::support::{
    sample_new_schedule_detail, sample_plugin_detail, sample_schedule_detail,
    sample_trigger_detail, sample_trigger_placeholder_detail,
};
use crate::data::{
    PluginFilterItem, PluginListItem, PluginsPageView, ScheduleListItem, SchedulesPageView,
    TriggerListItem, TriggersPageView,
};
use crate::routes;

#[test]
fn schedules_template_renders_form_actions_and_history() {
    let detail = sample_schedule_detail();
    let detail_html = routes::test_support::render_schedule_detail(detail.clone())
        .expect("schedule detail renders");
    let html = routes::test_support::render_schedules_page(
        SchedulesPageView {
            mode_label: "1 active of 1".into(),
            mode_tone: "success",
            schedules: vec![ScheduleListItem {
                title: "nightly-review".into(),
                subtitle: "feature-dev · default input".into(),
                preview: "0 0 * * * * · Next 2026-03-11 00:00:00".into(),
                source_label: "Last 2026-03-10 12:35".into(),
                status_label: "Enabled".into(),
                status_tone: "sage",
                page_url: "/schedules?schedule=nightly-review".into(),
                active: true,
            }],
            selected: detail,
            new_schedule_url: "/schedules?schedule=__new__".into(),
        },
        detail_html,
    )
    .expect("schedules template renders");

    assert!(html.contains("Search schedules"));
    assert!(html.contains("New schedule"));
    assert!(html.contains("data-list-item"));
    assert!(html.contains("Pause schedule"));
    assert!(html.contains("Recent matching runs"));
}

#[test]
fn schedule_detail_template_hides_destructive_controls_for_new_schedule() {
    let html = routes::test_support::render_schedule_detail(sample_new_schedule_detail())
        .expect("new schedule detail renders");

    assert!(html.contains("Create schedule"));
    assert!(html.contains("No teams are installed yet."));
    assert!(html.contains("No matching runs found for this schedule yet."));
    assert!(!html.contains("Delete schedule"));
    assert!(!html.contains("Enable schedule"));
}

#[test]
fn plugins_template_renders_install_and_lifecycle_controls() {
    let detail = sample_plugin_detail();
    let detail_html =
        routes::test_support::render_plugin_detail(detail.clone()).expect("plugin detail renders");
    let html = routes::test_support::render_plugins_page(
        PluginsPageView {
            mode_label: "1 operational · 0 attention · 0 disabled".into(),
            mode_tone: "success",
            filters: vec![
                PluginFilterItem {
                    label: "All".into(),
                    count: 1,
                    tone: "neutral",
                    page_url: "/plugins".into(),
                    active: true,
                },
                PluginFilterItem {
                    label: "Operational".into(),
                    count: 1,
                    tone: "success",
                    page_url: "/plugins?status=operational".into(),
                    active: false,
                },
            ],
            plugins: vec![PluginListItem {
                title: "ops-tools".into(),
                subtitle: "v1.2.3 · OG".into(),
                preview: "Operational helpers for dashboards and workflows.".into(),
                status_detail: "Runtime initialized and ready for operators.".into(),
                search_text: "skill ops-tools/check-health".into(),
                source_label: "/home/dh/.opengoose/plugins/ops-tools".into(),
                source_badge: "ops-tools".into(),
                status_label: "Ready".into(),
                status_tone: "success",
                page_url: "/plugins?plugin=ops-tools".into(),
                active: true,
            }],
            selected: detail,
        },
        detail_html,
    )
    .expect("plugins template renders");

    assert!(html.contains("Search plugins"));
    assert!(html.contains("Operational · 1"));
    assert!(html.contains("Install plugin"));
    assert!(html.contains("Disable plugin"));
    assert!(html.contains("Runtime verification"));
    assert!(html.contains("Confirm removal of"));
}

#[test]
fn trigger_detail_template_renders_placeholder_create_form() {
    let html = routes::test_support::render_trigger_detail(sample_trigger_placeholder_detail())
        .expect("trigger detail renders");

    assert!(html.contains("No triggers configured"));
    assert!(html.contains("trigger-create-form"));
    assert!(html.contains("Name, type, and team are required to create a trigger."));
    assert!(html.contains("Create trigger"));
}

#[test]
fn triggers_template_renders_actions_and_forms() {
    let detail = sample_trigger_detail();
    let detail_html = routes::test_support::render_trigger_detail(detail.clone())
        .expect("trigger detail renders");
    let html = routes::test_support::render_triggers_page(
        TriggersPageView {
            mode_label: "1 trigger(s)".into(),
            mode_tone: "success",
            triggers: vec![TriggerListItem {
                title: "on-pr-open".into(),
                subtitle: "Webhook".into(),
                team_label: "code-review".into(),
                status_label: "enabled".into(),
                status_tone: "success",
                last_fired: "2026-03-10 12:34".into(),
                page_url: "/triggers?trigger=on-pr-open".into(),
                active: true,
            }],
            selected: detail,
        },
        detail_html,
    )
    .expect("triggers template renders");

    assert!(html.contains("Search triggers"));
    assert!(html.contains("Run test"));
    assert!(html.contains("Save changes"));
    assert!(html.contains("Create new trigger"));
    assert!(html.contains("Delete trigger"));
}
