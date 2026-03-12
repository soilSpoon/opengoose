use super::support::{sample_queue_detail, sample_session_detail};
use crate::data::{BatchExportFormView, SelectOption, SessionListItem, SessionsPageView};
use crate::routes;

#[test]
fn sessions_template_renders_accessible_list_controls() {
    let detail = sample_session_detail();
    let detail_html =
        routes::test_support::render_session_detail(detail.clone()).expect("detail renders");
    let html = routes::test_support::render_sessions_page(
        SessionsPageView {
            mode_label: "Live runtime".into(),
            mode_tone: "success",
            live_stream_url: "/sessions/events?session=discord%3Adirect%3Achan-1".into(),
            batch_export: BatchExportFormView {
                action_url: "/api/sessions/export".into(),
                since: "7d".into(),
                until: "2026-03-10".into(),
                limit: 25,
                format_options: vec![
                    SelectOption {
                        value: "json".into(),
                        label: "JSON".into(),
                        selected: true,
                    },
                    SelectOption {
                        value: "md".into(),
                        label: "Markdown".into(),
                        selected: false,
                    },
                ],
                hint: "Provide at least one of since or until.".into(),
            },
            sessions: vec![SessionListItem {
                title: "ops / bridge".into(),
                subtitle: "feature-dev active · Live runtime".into(),
                preview: "Investigate the dashboard refresh cycle.".into(),
                updated_at: "2026-03-10 12:34".into(),
                badge: "DISCORD".into(),
                badge_tone: "cyan",
                page_url: "/sessions?session=ops".into(),
                active: true,
            }],
            selected: detail,
        },
        detail_html,
    )
    .expect("sessions template renders");

    assert!(html.contains("data-list-shell"));
    assert!(html.contains("Search sessions"));
    assert!(html.contains("data-list-item"));
    assert!(html.contains("data-detail-panel"));
    assert!(html.contains("Batch export"));
    assert!(html.contains("action=\"/api/sessions/export\""));
    assert!(html.contains("name=\"since\""));
    assert!(html.contains("Export Markdown"));
    assert!(html.contains("/api/sessions/discord%3Aops%3Abridge/export?format=json"));
    assert!(!html.contains("hx-get"));
}

#[test]
fn session_detail_template_renders_export_actions_with_empty_messages() {
    let mut detail = sample_session_detail();
    detail.messages.clear();
    detail.empty_hint = "This session has no persisted messages yet.".into();

    let html = routes::test_support::render_session_detail(detail).expect("detail renders");

    assert!(html.contains("Export JSON"));
    assert!(html.contains("Export Markdown"));
    assert!(html.contains("This session has no persisted messages yet."));
}

#[test]
fn queue_detail_template_renders_table_controls() {
    let html = routes::test_support::render_queue_detail(sample_queue_detail())
        .expect("queue detail renders");

    assert!(html.contains("data-table-shell"));
    assert!(html.contains("Search traffic"));
    assert!(html.contains("data-table-row"));
    assert!(html.contains("Retries high-low"));
}
