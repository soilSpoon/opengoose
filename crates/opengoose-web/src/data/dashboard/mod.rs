mod activity;
mod metrics;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use opengoose_persistence::{Database, MessageQueue, OrchestrationStore, RunStatus, SessionStore};

use self::activity::build_dashboard_activities;
use self::metrics::{build_duration_bars, build_status_segments, duration_stats};
use crate::data::runs::{build_run_list_items, mock_runs};
use crate::data::sessions::{build_session_list_items, live_sessions, mock_sessions};
use crate::data::utils::{queue_total, ratio_percent};
use crate::data::views::{
    AlertCard, DashboardView, GatewayCard, GatewayPanelView, HeroLiveIntroView, MetricCard,
    MetricGridView, MonitorBannerView,
};

/// Load all data needed for the dashboard page from the database.
pub fn load_dashboard(db: Arc<Database>) -> Result<DashboardView> {
    let session_store = SessionStore::new(db.clone());
    let session_stats = session_store.stats()?;
    let session_rows = session_store.list_sessions(4)?;

    let run_store = OrchestrationStore::new(db.clone());
    let recent_runs = run_store.list_runs(None, 12)?;

    let queue = MessageQueue::new(db.clone());
    let queue_stats = queue.stats()?;

    let using_mock = session_rows.is_empty()
        && recent_runs.is_empty()
        && queue_total(&queue_stats) == 0
        && session_stats.session_count == 0;

    let source_label = if using_mock {
        "Mock preview"
    } else {
        "Live runtime"
    };

    let session_records = if using_mock {
        mock_sessions()
    } else {
        live_sessions(&session_store, &session_rows)?
    };
    let run_records = if using_mock { mock_runs() } else { recent_runs };

    let running_count = run_records
        .iter()
        .filter(|run| run.status == RunStatus::Running)
        .count();
    let completed_count = run_records
        .iter()
        .filter(|run| run.status == RunStatus::Completed)
        .count();
    let failed_count = run_records
        .iter()
        .filter(|run| run.status == RunStatus::Failed)
        .count();
    let suspended_count = run_records
        .iter()
        .filter(|run| run.status == RunStatus::Suspended)
        .count();
    let terminal_total = completed_count + failed_count;
    let success_rate = ratio_percent(completed_count, terminal_total);
    let duration_stats = duration_stats(&run_records);
    let queue_backlog = queue_stats.pending + queue_stats.processing;
    let mode_label = if using_mock {
        "Mock preview".to_string()
    } else {
        "Live runtime".to_string()
    };
    let mode_tone = if using_mock { "neutral" } else { "success" };
    let stream_summary = if using_mock {
        "The dashboard is rendering seeded signals so the monitoring layout can be reviewed before live traffic exists.".to_string()
    } else {
        "Server-sent events stream fresh snapshots from SQLite-backed sessions, runs, queue traffic, and agent chatter every few seconds.".to_string()
    };
    let snapshot_label = format!("Snapshot {}", Utc::now().format("%H:%M:%S UTC"));

    let sessions = build_session_list_items(&session_records, None, source_label);
    let run_items = build_run_list_items(&run_records, None, source_label);
    let activities =
        build_dashboard_activities(db, &run_records, &session_records, &queue_stats, using_mock)?;

    let mut alerts = Vec::new();
    if using_mock {
        alerts.push(AlertCard {
            eyebrow: "Preview Mode".into(),
            title: "No runtime data yet".into(),
            description: "The dashboard is rendering seeded sessions, runs, and queue traffic so the UI can be reviewed before live traffic exists.".into(),
            tone: "neutral",
        });
    }
    if queue_stats.dead > 0 {
        alerts.push(AlertCard {
            eyebrow: "Queue Alert".into(),
            title: format!("{} dead-letter item(s)", queue_stats.dead),
            description: "Review the queue monitor to inspect retries and failed delegations."
                .into(),
            tone: "danger",
        });
    }
    if queue_backlog > 0 {
        alerts.push(AlertCard {
            eyebrow: "Queue Flow".into(),
            title: format!("{queue_backlog} item(s) still in flight"),
            description: "Pending and processing traffic is visible in the queue monitor and refreshes automatically through the SSE stream.".into(),
            tone: "amber",
        });
    }
    if terminal_total > 0 && success_rate < 80 {
        alerts.push(AlertCard {
            eyebrow: "Execution Risk".into(),
            title: format!("Success rate at {success_rate}%"),
            description: "Recent finished runs have started to trend down. Review the activity feed and queue errors before the backlog compounds.".into(),
            tone: "danger",
        });
    } else if !using_mock && running_count > 0 {
        alerts.push(AlertCard {
            eyebrow: "Runtime Active".into(),
            title: format!("{running_count} orchestration run(s) currently active"),
            description: "The dashboard is streaming run status, queue pressure, and agent chatter from the persisted runtime state.".into(),
            tone: "success",
        });
    }
    if alerts.is_empty() {
        alerts.push(AlertCard {
            eyebrow: "Steady State".into(),
            title: "No critical alerts in the latest snapshot".into(),
            description: "The dashboard remains live and ready for the next orchestration burst."
                .into(),
            tone: "neutral",
        });
    }

    Ok(DashboardView {
        intro: HeroLiveIntroView {
            id: String::new(),
            eyebrow: "Signal board".into(),
            title: "Track sessions, orchestration, and queue pressure from one surface.".into(),
            summary: "The dashboard stays server-rendered for resilience, then refreshes the live board whenever the runtime event stream moves.".into(),
            transport_label: "Live transport".into(),
            mode_tone,
            mode_label: mode_label.clone(),
            status_summary: stream_summary.clone(),
            status_id: String::new(),
            status_note:
                "Live snapshots re-render the board below as session, run, and queue events arrive."
                    .into(),
        },
        banner: MonitorBannerView {
            eyebrow: "Live snapshot".into(),
            title: "Runs, queue pressure, and agent chatter stay in one place.".into(),
            summary: stream_summary.clone(),
            mode_tone,
            mode_label: mode_label.clone(),
            stream_label: "Event stream + fallback sweep".into(),
            snapshot_label: snapshot_label.clone(),
        },
        metric_grid: MetricGridView {
            class_name: "metric-grid".into(),
            items: vec![
                MetricCard {
                    label: "Tracked sessions".into(),
                    value: session_stats.session_count.to_string(),
                    note: format!("{} stored messages", session_stats.message_count),
                    tone: "cyan",
                },
                MetricCard {
                    label: "Active runs".into(),
                    value: running_count.to_string(),
                    note: format!("{completed_count} completed in the latest window"),
                    tone: "amber",
                },
                MetricCard {
                    label: "Success rate".into(),
                    value: if terminal_total == 0 {
                        "No finished runs".into()
                    } else {
                        format!("{success_rate}%")
                    },
                    note: format!("{completed_count} complete / {failed_count} failed"),
                    tone: "sage",
                },
                MetricCard {
                    label: "Avg cycle".into(),
                    value: duration_stats
                        .average_label
                        .unwrap_or_else(|| "Awaiting data".into()),
                    note: duration_stats.note,
                    tone: "rose",
                },
            ],
        },
        queue_cards: vec![
            MetricCard {
                label: "Pending".into(),
                value: queue_stats.pending.to_string(),
                note: "Waiting for pickup".into(),
                tone: "amber",
            },
            MetricCard {
                label: "Processing".into(),
                value: queue_stats.processing.to_string(),
                note: "Currently being handled".into(),
                tone: "cyan",
            },
            MetricCard {
                label: "Completed".into(),
                value: queue_stats.completed.to_string(),
                note: "Resolved queue items".into(),
                tone: "sage",
            },
            MetricCard {
                label: "Dead letters".into(),
                value: queue_stats.dead.to_string(),
                note: "Needs operator attention".into(),
                tone: "rose",
            },
        ],
        run_segments: build_status_segments(vec![
            ("Running", running_count as i64, "cyan"),
            ("Completed", completed_count as i64, "sage"),
            ("Suspended", suspended_count as i64, "amber"),
            ("Failed", failed_count as i64, "rose"),
        ]),
        queue_segments: build_status_segments(vec![
            ("Pending", queue_stats.pending, "amber"),
            ("Processing", queue_stats.processing, "cyan"),
            ("Completed", queue_stats.completed, "sage"),
            ("Dead", queue_stats.dead, "rose"),
        ]),
        duration_bars: build_duration_bars(&run_records),
        activities,
        alerts,
        sessions,
        runs: run_items,
        gateway_panel: GatewayPanelView {
            title: "Gateway status".into(),
            subtitle: "Connection state for each channel adapter".into(),
            empty_hint: String::new(),
            cards: default_gateway_cards(),
        },
    })
}

fn default_gateway_cards() -> Vec<GatewayCard> {
    ["Discord", "Slack", "Telegram", "Matrix"]
        .iter()
        .map(|name| GatewayCard {
            platform: name.to_string(),
            state_label: "Disconnected".into(),
            state_tone: "neutral",
            uptime_label: "\u{2014}".into(),
            detail: "No connection data available".into(),
        })
        .collect()
}
