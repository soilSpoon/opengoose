#![recursion_limit = "256"]

/// Dashboard view-model structs and data loaders for the HTML templates.
pub mod data;
/// Typed error types for web handlers with HTTP status code mapping.
pub mod error;
mod handlers;
mod live;
pub mod middleware;
/// OpenAPI 3.0 spec builder and Swagger UI handler.
pub mod openapi;
mod pages;
mod routes;
/// Server configuration types (bind address, TLS paths).
pub mod server;
mod state;
#[cfg(test)]
pub(crate) mod test_support;
mod tls;

/// Re-exported error type for web API and page handlers.
pub use error::WebError;
pub use routes::render_dashboard_live_partial;
pub use server::WebOptions;
/// Re-exported shared application state for all handlers.
pub use state::AppState;
/// Alias kept for backward compatibility.
pub use state::AppState as SharedAppState;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;
use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};

use crate::handlers::remote_agents::RemoteGatewayState;
use crate::server::PageState;

/// Start the web dashboard and JSON API server.
///
/// Binds to the address in `options`, serves HTML pages, static assets,
/// REST endpoints under `/api/`, and the remote-agent WebSocket gateway.
pub async fn serve(options: WebOptions) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let remote_state = Arc::new(RemoteGatewayState {
        registry: RemoteAgentRegistry::new(RemoteConfig::default()),
    });
    let api_state = AppState::new(db.clone())?;
    let state = PageState {
        db: db.clone(),
        remote_registry: remote_state.registry.clone(),
        channel_metrics: api_state.channel_metrics.clone(),
        event_bus: api_state.event_bus.clone(),
    };
    live::spawn_live_event_watcher(state.db.clone(), api_state.event_bus.clone());

    let app = routes::app_router(state, api_state, remote_state);

    tls::start_server(options, app).await
}

#[cfg(test)]
mod tests {
    use crate::data::{
        ActivityItem, AlertCard, DashboardView, MessageBubble, MetaRow, MetricCard, Notice,
        QueueDetailView, QueueMessageView, RunListItem, ScheduleEditorView, ScheduleHistoryItem,
        ScheduleListItem, SchedulesPageView, SelectOption, SessionDetailView, SessionListItem,
        SessionsPageView, StatusSegment, TrendBar, WorkflowAutomationView, WorkflowDetailView,
        WorkflowListItem, WorkflowRunView, WorkflowStepView, WorkflowsPageView,
    };
    use crate::routes;

    fn sample_dashboard() -> DashboardView {
        DashboardView {
            mode_label: "Live runtime".into(),
            mode_tone: "success",
            stream_summary: "Server-sent events stream fresh snapshots.".into(),
            snapshot_label: "Snapshot 12:34:56 UTC".into(),
            metrics: vec![MetricCard {
                label: "Active runs".into(),
                value: "2".into(),
                note: "1 completed in the latest window".into(),
                tone: "amber",
            }],
            queue_cards: vec![MetricCard {
                label: "Pending".into(),
                value: "4".into(),
                note: "Waiting for pickup".into(),
                tone: "cyan",
            }],
            run_segments: vec![StatusSegment {
                label: "Running".into(),
                value: "2".into(),
                tone: "cyan",
                width: 100,
            }],
            queue_segments: vec![StatusSegment {
                label: "Pending".into(),
                value: "4".into(),
                tone: "amber",
                width: 100,
            }],
            duration_bars: vec![TrendBar {
                label: "feature-dev".into(),
                value: "7m 12s".into(),
                detail: "running".into(),
                tone: "cyan",
                height: 76,
            }],
            activities: vec![ActivityItem {
                actor: "frontend-engineer".into(),
                meta: "Directed to reviewer".into(),
                detail: "Live dashboard shell refreshed over SSE.".into(),
                timestamp: "2026-03-10 12:34".into(),
                tone: "cyan",
            }],
            alerts: vec![AlertCard {
                eyebrow: "Runtime Active".into(),
                title: "2 orchestration runs currently active".into(),
                description: "The dashboard is streaming run status and queue pressure.".into(),
                tone: "success",
            }],
            sessions: vec![SessionListItem {
                title: "ops / bridge".into(),
                subtitle: "feature-dev active · Live runtime".into(),
                preview: "Review the live dashboard state.".into(),
                updated_at: "2026-03-10 12:34".into(),
                badge: "DISCORD".into(),
                badge_tone: "cyan",
                page_url: "/sessions?session=ops".into(),
                active: false,
            }],
            runs: vec![RunListItem {
                title: "feature-dev".into(),
                subtitle: "chain workflow · Live runtime".into(),
                updated_at: "2026-03-10 12:34".into(),
                progress_label: "2/4 steps".into(),
                badge: "RUNNING".into(),
                badge_tone: "cyan",
                page_url: "/runs?run=run-1".into(),
                queue_page_url: "/queue?run=run-1".into(),
                active: false,
            }],
            gateways: vec![],
        }
    }

    fn sample_session_detail() -> SessionDetailView {
        SessionDetailView {
            title: "Session ops".into(),
            subtitle: "discord / ops".into(),
            source_label: "Live runtime".into(),
            meta: vec![MetaRow {
                label: "Stable key".into(),
                value: "discord:ops:bridge".into(),
            }],
            messages: vec![MessageBubble {
                role_label: "Assistant".into(),
                author_label: "frontend-engineer".into(),
                timestamp: "2026-03-10 12:34".into(),
                content: "Dashboard panel updated.".into(),
                tone: "accent",
                alignment: "right",
            }],
            empty_hint: "No messages yet.".into(),
        }
    }

    fn sample_queue_detail() -> QueueDetailView {
        QueueDetailView {
            title: "Queue run-1".into(),
            subtitle: "feature-dev / chain".into(),
            source_label: "Live runtime".into(),
            status_cards: vec![MetricCard {
                label: "Pending".into(),
                value: "1".into(),
                note: "Waiting for recipients".into(),
                tone: "amber",
            }],
            messages: vec![QueueMessageView {
                sender: "planner".into(),
                recipient: "developer".into(),
                kind: "task".into(),
                status_label: "pending".into(),
                status_tone: "amber",
                created_at: "2026-03-10 12:35".into(),
                retry_text: "1/3".into(),
                content: "Implement the dashboard controls.".into(),
                error: "waiting for pickup".into(),
            }],
            dead_letters: vec![],
            empty_hint: "No queue traffic yet.".into(),
        }
    }

    fn sample_workflow_detail() -> WorkflowDetailView {
        WorkflowDetailView {
            title: "feature-dev".into(),
            subtitle: "Build and verify product changes with a chained team.".into(),
            source_label: "Live registry".into(),
            status_label: "Running".into(),
            status_tone: "cyan",
            meta: vec![MetaRow {
                label: "Pattern".into(),
                value: "Chain".into(),
            }],
            steps: vec![WorkflowStepView {
                title: "Step 1 · planner".into(),
                detail: "Shape the implementation plan.".into(),
                badge: "Sequential".into(),
                badge_tone: "cyan",
            }],
            automations: vec![WorkflowAutomationView {
                kind: "Schedule".into(),
                title: "nightly-review".into(),
                detail: "0 0 * * * · team feature-dev".into(),
                note: "Next 2026-03-11 00:00".into(),
                status_label: "Enabled".into(),
                status_tone: "sage",
            }],
            recent_runs: vec![WorkflowRunView {
                title: "Run run-1".into(),
                detail: "2/4 steps · Still executing".into(),
                updated_at: "2026-03-10 12:35".into(),
                status_label: "Running".into(),
                status_tone: "cyan",
                page_url: "/runs?run=run-1".into(),
            }],
            yaml: "title: feature-dev".into(),
            trigger_api_url: "/api/workflows/feature-dev/trigger".into(),
            trigger_input: "Manual run requested".into(),
        }
    }

    fn sample_schedule_detail() -> ScheduleEditorView {
        ScheduleEditorView {
            title: "nightly-review".into(),
            subtitle: "Adjust cadence, target team, and run input without leaving the dashboard."
                .into(),
            source_label: "Live schedule store".into(),
            original_name: "nightly-review".into(),
            name: "nightly-review".into(),
            cron_expression: "0 0 * * * *".into(),
            team_name: "feature-dev".into(),
            input: String::new(),
            enabled: true,
            is_new: false,
            name_locked: true,
            meta: vec![MetaRow {
                label: "Next fire".into(),
                value: "2026-03-11 00:00:00".into(),
            }],
            team_options: vec![SelectOption {
                value: "feature-dev".into(),
                label: "feature-dev".into(),
                selected: true,
            }],
            history: vec![ScheduleHistoryItem {
                title: "run-1".into(),
                detail: "chain workflow · Scheduled run: nightly-review".into(),
                updated_at: "2026-03-10 12:35".into(),
                status_label: "completed".into(),
                status_tone: "sage",
                page_url: "/runs?run=run-1".into(),
            }],
            history_hint: "No matching runs found for this schedule yet.".into(),
            notice: Some(Notice {
                text: "Schedule saved.".into(),
                tone: "success",
            }),
            save_label: "Save changes".into(),
            toggle_label: "Pause schedule".into(),
            delete_label: "nightly-review".into(),
        }
    }

    #[test]
    fn dashboard_live_template_renders_monitoring_sections() {
        let html = routes::test_support::render_dashboard_live(sample_dashboard())
            .expect("dashboard live partial renders");

        assert!(html.contains("Execution mix"));
        assert!(html.contains("Queue mix"));
        assert!(html.contains("Agent activity"));
        assert!(html.contains("Live runtime"));
        assert!(html.contains("feature-dev"));
    }

    #[test]
    fn sessions_template_renders_accessible_list_controls() {
        let detail = sample_session_detail();
        let detail_html =
            routes::test_support::render_session_detail(detail.clone()).expect("detail renders");
        let html = routes::test_support::render_sessions_page(
            SessionsPageView {
                mode_label: "Live runtime".into(),
                mode_tone: "success",
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

        assert!(html.contains("/assets/styles/shared.css"));
        assert!(html.contains("/assets/styles/detail.css"));
        assert!(html.contains("data-list-shell"));
        assert!(html.contains("Search sessions"));
        assert!(html.contains("data-list-item"));
        assert!(html.contains("data-detail-panel"));
        assert!(!html.contains("hx-get"));
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

        assert!(html.contains("/assets/styles/shared.css"));
        assert!(html.contains("/assets/styles/detail.css"));
        assert!(html.contains("/assets/styles/schedules.css"));
        assert!(html.contains("Search schedules"));
        assert!(html.contains("New schedule"));
        assert!(html.contains("Pause schedule"));
        assert!(html.contains("Recent matching runs"));
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

        assert!(html.contains("/assets/styles/shared.css"));
        assert!(html.contains("/assets/styles/detail.css"));
        assert!(html.contains("Search workflows"));
        assert!(html.contains("data-workflow-trigger"));
        assert!(html.contains("/api/workflows/feature-dev/trigger"));
        assert!(html.contains("Recent runs"));
    }

    use crate::handlers;
    use crate::handlers::dashboard::get_dashboard;
    use crate::handlers::test_support::make_state;
    use crate::routes::health::{
        MetricsResponse, QueueMetrics, RunMetrics, SessionMetrics, health as health_handler,
        live as live_handler, ready as ready_handler,
    };
    use crate::state::AppState;
    use axum::{
        Json, Router,
        body::Body,
        body::to_bytes,
        extract::State,
        http::{Method, Request, StatusCode, Uri},
        routing::{get, post},
    };
    use opengoose_persistence::RunStatus;
    use serde_json::Value;
    use tower::ServiceExt;

    async fn api_metrics(
        State(state): State<AppState>,
    ) -> Result<Json<MetricsResponse>, (StatusCode, Json<serde_json::Value>)> {
        let session_stats = state
            .session_store
            .stats()
            .map_err(|e| routes::api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

        let recent_runs = state
            .orchestration_store
            .list_runs(None, 200)
            .map_err(|e| routes::api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

        let running = recent_runs
            .iter()
            .filter(|r| r.status == RunStatus::Running)
            .count();
        let completed = recent_runs
            .iter()
            .filter(|r| r.status == RunStatus::Completed)
            .count();
        let failed = recent_runs
            .iter()
            .filter(|r| r.status == RunStatus::Failed)
            .count();
        let suspended = recent_runs
            .iter()
            .filter(|r| r.status == RunStatus::Suspended)
            .count();

        Ok(Json(MetricsResponse {
            sessions: SessionMetrics {
                total: session_stats.session_count,
                messages: session_stats.message_count,
            },
            queue: QueueMetrics {
                pending: 0,
                processing: 0,
                completed: 0,
                failed: 0,
                dead: 0,
            },
            runs: RunMetrics {
                running,
                completed,
                failed,
                suspended,
            },
        }))
    }

    fn api_router() -> Router {
        let state = make_state();

        Router::new()
            .route("/api/health", get(health_handler))
            .route("/api/health/ready", get(ready_handler))
            .route("/api/health/live", get(live_handler))
            .route("/api/sessions", get(handlers::sessions::list_sessions))
            .route(
                "/api/sessions/{session_key}/messages",
                get(handlers::sessions::get_messages),
            )
            .route("/api/runs", get(handlers::runs::list_runs))
            .route("/api/agents", get(handlers::agents::list_agents))
            .route("/api/teams", get(handlers::teams::list_teams))
            .route("/api/workflows", get(handlers::workflows::list_workflows))
            .route(
                "/api/workflows/{name}",
                get(handlers::workflows::get_workflow),
            )
            .route(
                "/api/workflows/{name}/trigger",
                post(handlers::workflows::trigger_workflow),
            )
            .route("/api/dashboard", get(get_dashboard))
            .route("/api/metrics", get(api_metrics))
            .with_state(state)
    }

    async fn read_json(response: axum::response::Response) -> Value {
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body should be readable");
        serde_json::from_slice(&body).expect("response body should be json")
    }

    #[tokio::test]
    async fn api_health_returns_ok() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/health"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = read_json(response).await;
        assert_eq!(payload["status"], "healthy");
        assert!(payload["components"]["gateways"].is_object());
    }

    #[tokio::test]
    async fn api_ready_and_live_return_probe_payloads() {
        let app = api_router();

        let ready = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/health/ready"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("ready request should be handled");

        assert_eq!(ready.status(), StatusCode::OK);
        let ready_body = read_json(ready).await;
        assert_eq!(ready_body["status"], "healthy");

        let live = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/health/live"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("live request should be handled");

        assert_eq!(live.status(), StatusCode::OK);
        let live_body = read_json(live).await;
        assert_eq!(live_body["status"], "healthy");
        assert!(live_body.get("checked_at").is_some());
    }

    #[tokio::test]
    async fn api_dashboard_and_metrics_return_object_payloads() {
        let app = api_router();
        let dashboard = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/dashboard"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("dashboard request should succeed");

        assert_eq!(dashboard.status(), StatusCode::OK);
        let dashboard_body = read_json(dashboard).await;
        assert!(dashboard_body.get("session_count").is_some());

        let metrics = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/metrics"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("metrics request should succeed");

        assert_eq!(metrics.status(), StatusCode::OK);
        let metrics_body = read_json(metrics).await;
        assert!(metrics_body.get("sessions").is_some());
        assert!(metrics_body.get("runs").is_some());
    }

    #[tokio::test]
    async fn api_session_and_run_lists_are_arrays() {
        let app = api_router();

        let sessions = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/sessions?limit=10"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("sessions request should succeed");
        assert_eq!(sessions.status(), StatusCode::OK);
        let sessions_body = read_json(sessions).await;
        assert!(sessions_body.is_array());

        let runs = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?limit=10"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("runs request should succeed");
        assert_eq!(runs.status(), StatusCode::OK);
        let runs_body = read_json(runs).await;
        assert!(runs_body.is_array());

        let teams = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/teams"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("teams request should succeed");
        assert_eq!(teams.status(), StatusCode::OK);
        let teams_body = read_json(teams).await;
        assert!(teams_body.is_array());

        let workflows = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/workflows"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("workflows request should succeed");
        assert_eq!(workflows.status(), StatusCode::OK);
        let workflows_body = read_json(workflows).await;
        assert!(workflows_body.is_array());
    }

    #[tokio::test]
    async fn api_session_messages_returns_empty_array_for_missing_session() {
        let app = api_router();
        let messages = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static(
                        "/api/sessions/discord%3Aguild%3Achannel/messages?limit=5",
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("messages request should succeed");

        assert_eq!(messages.status(), StatusCode::OK);
        let body = read_json(messages).await;
        assert!(body.is_array());
    }

    // ── Full API router (includes alerts + fallback) ──────────────────────

    fn full_api_router() -> Router {
        let state = make_state();

        Router::new()
            .route("/api/health", get(health_handler))
            .route("/api/health/ready", get(ready_handler))
            .route("/api/health/live", get(live_handler))
            .route("/api/sessions", get(handlers::sessions::list_sessions))
            .route(
                "/api/sessions/{session_key}/messages",
                get(handlers::sessions::get_messages),
            )
            .route("/api/runs", get(handlers::runs::list_runs))
            .route("/api/agents", get(handlers::agents::list_agents))
            .route("/api/teams", get(handlers::teams::list_teams))
            .route("/api/workflows", get(handlers::workflows::list_workflows))
            .route(
                "/api/workflows/{name}",
                get(handlers::workflows::get_workflow),
            )
            .route(
                "/api/workflows/{name}/trigger",
                post(handlers::workflows::trigger_workflow),
            )
            .route("/api/dashboard", get(get_dashboard))
            .route("/api/metrics", get(api_metrics))
            .route("/api/alerts", get(handlers::alerts::list_alerts))
            .route(
                "/api/alerts",
                axum::routing::post(handlers::alerts::create_alert),
            )
            .route(
                "/api/alerts/{name}",
                axum::routing::delete(handlers::alerts::delete_alert),
            )
            .route("/api/alerts/history", get(handlers::alerts::alert_history))
            .route(
                "/api/alerts/test",
                axum::routing::post(handlers::alerts::test_alerts),
            )
            .fallback(|| async { StatusCode::NOT_FOUND })
            .with_state(state)
    }

    // ── Alert endpoint integration tests ──────────────────────────────────

    #[tokio::test]
    async fn api_alerts_list_returns_empty_array() {
        let app = full_api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/alerts"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn api_alerts_create_and_list_round_trip() {
        let app = full_api_router();

        // Create an alert rule
        let create_body = serde_json::json!({
            "name": "high-backlog",
            "description": "Queue backlog is too high",
            "metric": "queue_backlog",
            "condition": "gt",
            "threshold": 100.0
        });

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("create request should succeed");

        assert_eq!(create_response.status(), StatusCode::OK);
        let created = read_json(create_response).await;
        assert_eq!(created["name"], "high-backlog");
        assert_eq!(created["metric"], "queue_backlog");
        assert_eq!(created["condition"], "gt");
        assert_eq!(created["threshold"], 100.0);
        assert_eq!(created["enabled"], true);
        assert!(created["id"].is_string());

        // List should now contain the new rule
        let list_response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/alerts"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("list request should succeed");

        assert_eq!(list_response.status(), StatusCode::OK);
        let list = read_json(list_response).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "high-backlog");
    }

    #[tokio::test]
    async fn api_alerts_create_rejects_invalid_metric() {
        let app = full_api_router();

        let body = serde_json::json!({
            "name": "bad-metric",
            "metric": "nonexistent_metric",
            "condition": "gt",
            "threshold": 50.0
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let err = read_json(response).await;
        assert!(err["error"].as_str().unwrap().contains("unknown metric"));
    }

    #[tokio::test]
    async fn api_alerts_create_rejects_invalid_condition() {
        let app = full_api_router();

        let body = serde_json::json!({
            "name": "bad-condition",
            "metric": "failed_runs",
            "condition": "neq",
            "threshold": 10.0
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let err = read_json(response).await;
        assert!(err["error"].as_str().unwrap().contains("unknown condition"));
    }

    #[tokio::test]
    async fn api_alerts_delete_nonexistent_returns_not_found() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(Uri::from_static("/api/alerts/no-such-alert"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_alerts_history_returns_empty_array() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/alerts/history"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn api_alerts_test_returns_metrics_and_triggered() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts/test"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.get("metrics").is_some());
        assert!(body.get("triggered").is_some());
        assert!(body["triggered"].is_array());
    }

    #[tokio::test]
    async fn api_missing_workflow_trigger_returns_not_found() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/workflows/no-such-workflow/trigger"))
                    .header("content-type", "application/json")
                    .body(Body::from(br#"{"input":"run"}"#.to_vec()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // ── Fallback / 404 test ───────────────────────────────────────────────

    #[tokio::test]
    async fn api_unknown_route_returns_not_found() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/does-not-exist"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // ── Sessions handler integration tests ────────────────────────────────

    #[tokio::test]
    async fn api_sessions_list_with_explicit_limit_returns_array() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/sessions?limit=5"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
    }

    #[tokio::test]
    async fn api_sessions_invalid_limit_returns_bad_request() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/sessions?limit=abc"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_session_messages_invalid_limit_returns_bad_request() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static(
                        "/api/sessions/discord%3Aguild%3Achannel/messages?limit=notanumber",
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ── Runs handler integration tests ────────────────────────────────────

    #[tokio::test]
    async fn api_runs_with_running_status_filter_returns_array() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?status=running"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
    }

    #[tokio::test]
    async fn api_runs_with_completed_status_filter_returns_array() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?status=completed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
    }

    #[tokio::test]
    async fn api_runs_invalid_status_filter_returns_unprocessable() {
        // Invalid status values are rejected by input validation (OPE-67).
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?status=not_a_real_status"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn api_runs_invalid_limit_returns_bad_request() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?limit=notanumber"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ── Alerts handler extended integration tests ─────────────────────────

    #[tokio::test]
    async fn api_alerts_create_and_delete_round_trip() {
        let app = full_api_router();

        let create_body = serde_json::json!({
            "name": "delete-me",
            "metric": "failed_runs",
            "condition": "gt",
            "threshold": 5.0
        });

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("create should succeed");

        assert_eq!(create_response.status(), StatusCode::OK);
        let created = read_json(create_response).await;
        assert_eq!(created["name"], "delete-me");

        let delete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(Uri::from_static("/api/alerts/delete-me"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("delete should succeed");

        assert_eq!(delete_response.status(), StatusCode::OK);
        let deleted = read_json(delete_response).await;
        assert_eq!(deleted["deleted"], "delete-me");

        // Verify it is gone — a second delete returns 404.
        let gone_response = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(Uri::from_static("/api/alerts/delete-me"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("second delete should be handled");

        assert_eq!(gone_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_alerts_create_missing_required_field_returns_unprocessable() {
        let app = full_api_router();

        // `threshold` is missing — JSON body is structurally valid but
        // deserialization will fail, causing Axum to return 422.
        let body = serde_json::json!({
            "name": "incomplete",
            "metric": "failed_runs",
            "condition": "gt"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn api_alerts_create_malformed_json_returns_bad_request() {
        // Syntactically invalid JSON → Axum 0.8 returns 400 Bad Request.
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(b"{not valid json}".as_ref()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // --- WebOptions TLS config tests ---

    #[test]
    fn web_options_plain_has_no_tls() {
        use std::net::{Ipv4Addr, SocketAddr};
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 8080));
        let opts = crate::WebOptions::plain(addr);
        assert_eq!(opts.bind, addr);
        assert!(opts.tls_cert_path.is_none());
        assert!(opts.tls_key_path.is_none());
    }

    #[test]
    fn web_options_default_has_no_tls() {
        let opts = crate::WebOptions::default();
        assert!(opts.tls_cert_path.is_none());
        assert!(opts.tls_key_path.is_none());
    }

    #[test]
    fn web_options_with_tls_paths_set() {
        use std::net::{Ipv4Addr, SocketAddr};
        let opts = crate::WebOptions {
            bind: SocketAddr::from((Ipv4Addr::LOCALHOST, 8443)),
            tls_cert_path: Some("/etc/ssl/cert.pem".into()),
            tls_key_path: Some("/etc/ssl/key.pem".into()),
        };
        assert_eq!(
            opts.tls_cert_path.unwrap().to_str().unwrap(),
            "/etc/ssl/cert.pem"
        );
        assert_eq!(
            opts.tls_key_path.unwrap().to_str().unwrap(),
            "/etc/ssl/key.pem"
        );
    }

    #[test]
    fn web_options_is_clone() {
        use std::net::{Ipv4Addr, SocketAddr};
        let opts = crate::WebOptions {
            bind: SocketAddr::from((Ipv4Addr::LOCALHOST, 9443)),
            tls_cert_path: Some("/cert.pem".into()),
            tls_key_path: Some("/key.pem".into()),
        };
        let cloned = opts.clone();
        assert_eq!(cloned.tls_cert_path, opts.tls_cert_path);
        assert_eq!(cloned.tls_key_path, opts.tls_key_path);
    }
}
