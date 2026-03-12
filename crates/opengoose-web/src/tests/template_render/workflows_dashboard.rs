use super::support::sample_workflow_detail;
use crate::data::{WorkflowListItem, WorkflowsPageView};
use crate::fixtures::sample_dashboard_view;
use crate::routes;

#[test]
fn dashboard_live_template_renders_monitoring_sections() {
    let html = routes::test_support::render_dashboard_live(sample_dashboard_view())
        .expect("dashboard live partial renders");

    assert!(html.contains("Execution mix"));
    assert!(html.contains("Queue mix"));
    assert!(html.contains("Agent activity"));
    assert!(html.contains("Live runtime"));
    assert!(html.contains("feature-dev"));
}

#[test]
fn workflows_template_renders_trigger_controls() {
    let detail = sample_workflow_detail();
    let detail_html = routes::test_support::render_workflow_detail(detail.clone())
        .expect("workflow detail renders");
    let html = routes::test_support::render_workflows_page(
        WorkflowsPageView {
            mode_label: "Live registry".into(),
            mode_tone: "success",
            workflows: vec![WorkflowListItem {
                title: "feature-dev".into(),
                subtitle: "Build and verify product changes with a chained team.".into(),
                preview: "1/1 enabled · 0 configured · planner · developer".into(),
                source_label: "Live registry".into(),
                status_label: "Running".into(),
                status_tone: "cyan",
                page_url: "/workflows?workflow=feature-dev".into(),
                active: true,
            }],
            selected: detail,
        },
        detail_html,
    )
    .expect("workflows template renders");

    assert!(html.contains("Search workflows"));
    assert!(html.contains("data-bind:workflow-trigger-input"));
    assert!(html.contains("/workflows/feature-dev/trigger"));
    assert!(html.contains("Recent runs"));
}
