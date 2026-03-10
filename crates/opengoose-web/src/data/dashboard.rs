use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use opengoose_persistence::{
    AgentMessage, AgentMessageStatus, AgentMessageStore, Database, MessageQueue, OrchestrationRun,
    OrchestrationStore, QueueStats, RunStatus, SessionStore,
};

use crate::data::runs::{build_run_list_items, mock_runs};
use crate::data::sessions::{
    SessionRecord, build_session_list_items, live_sessions, mock_sessions,
};
use crate::data::utils::{
    format_duration, preview, progress_label, queue_total, ratio_percent, run_duration_seconds,
    run_tone,
};
use crate::data::views::{
    ActivityItem, AlertCard, DashboardView, MetricCard, StatusSegment, TrendBar,
};

struct DurationStats {
    average_label: Option<String>,
    note: String,
}

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
        mode_label: if using_mock {
            "Mock preview".into()
        } else {
            "Live runtime".into()
        },
        mode_tone: if using_mock { "neutral" } else { "success" },
        stream_summary: if using_mock {
            "The dashboard is rendering seeded signals so the monitoring layout can be reviewed before live traffic exists.".into()
        } else {
            "Server-sent events stream fresh snapshots from SQLite-backed sessions, runs, queue traffic, and agent chatter every few seconds.".into()
        },
        snapshot_label: format!("Snapshot {}", Utc::now().format("%H:%M:%S UTC")),
        metrics: vec![
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
    })
}

fn build_dashboard_activities(
    db: Arc<Database>,
    runs: &[OrchestrationRun],
    sessions: &[SessionRecord],
    queue_stats: &QueueStats,
    using_mock: bool,
) -> Result<Vec<ActivityItem>> {
    let store = AgentMessageStore::new(db);
    let messages = store.list_recent_global(8)?;
    if !messages.is_empty() {
        return Ok(messages
            .into_iter()
            .map(|message| {
                let meta = activity_meta(&message);
                let detail = preview(&message.payload, 132);
                let tone = message_tone(&message.status);
                ActivityItem {
                    actor: message.from_agent,
                    meta,
                    detail,
                    timestamp: message.created_at,
                    tone,
                }
            })
            .collect());
    }

    if using_mock {
        return Ok(mock_dashboard_activities());
    }

    Ok(synthetic_dashboard_activities(runs, sessions, queue_stats))
}

fn activity_meta(message: &AgentMessage) -> String {
    if let Some(target) = &message.to_agent {
        format!(
            "Directed to {target} · {} · {}",
            message.session_key,
            message.status.as_str()
        )
    } else if let Some(channel) = &message.channel {
        format!(
            "Published to #{channel} · {} · {}",
            message.session_key,
            message.status.as_str()
        )
    } else {
        format!("{} · {}", message.session_key, message.status.as_str())
    }
}

fn mock_dashboard_activities() -> Vec<ActivityItem> {
    vec![
        ActivityItem {
            actor: "architect".into(),
            meta: "Published to #ops · discord:ns:studio-a:ops-bridge · delivered".into(),
            detail: "Signal board framing approved. Keep the shell resilient, then layer live fragments through SSE.".into(),
            timestamp: "2026-03-10 10:29".into(),
            tone: "cyan",
        },
        ActivityItem {
            actor: "developer".into(),
            meta: "Directed to reviewer · discord:ns:studio-a:ops-bridge · pending".into(),
            detail: "Handing off the live dashboard layout and queue telemetry for review.".into(),
            timestamp: "2026-03-10 10:23".into(),
            tone: "amber",
        },
        ActivityItem {
            actor: "reviewer".into(),
            meta: "Directed to developer · telegram:direct:founder-42 · acknowledged".into(),
            detail: "One migration note is still missing, otherwise the monitoring pass is ready.".into(),
            timestamp: "2026-03-10 09:44".into(),
            tone: "sage",
        },
    ]
}

fn synthetic_dashboard_activities(
    runs: &[OrchestrationRun],
    sessions: &[SessionRecord],
    queue_stats: &QueueStats,
) -> Vec<ActivityItem> {
    let mut items: Vec<ActivityItem> = runs
        .iter()
        .take(4)
        .map(|run| ActivityItem {
            actor: run.team_name.clone(),
            meta: format!("{} · {}", run.status.as_str(), progress_label(run)),
            detail: preview(&run.input, 132),
            timestamp: run.updated_at.clone(),
            tone: run_tone(&run.status),
        })
        .collect();

    items.extend(
        sessions
            .iter()
            .filter_map(|session| session.messages.last().map(|message| (session, message)))
            .take(2)
            .map(|(session, message)| ActivityItem {
                actor: session.summary.session_key.clone(),
                meta: format!(
                    "{} · {}",
                    session
                        .summary
                        .active_team
                        .clone()
                        .unwrap_or_else(|| "no active team".into()),
                    message.role
                ),
                detail: preview(&message.content, 132),
                timestamp: message.created_at.clone(),
                tone: if message.role == "assistant" {
                    "cyan"
                } else {
                    "neutral"
                },
            }),
    );

    if queue_stats.dead > 0 {
        items.push(ActivityItem {
            actor: "queue-monitor".into(),
            meta: "dead letters detected".into(),
            detail: format!(
                "{} item(s) require manual intervention before they can be retried.",
                queue_stats.dead
            ),
            timestamp: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            tone: "rose",
        });
    }

    items.truncate(6);
    items
}

fn duration_stats(runs: &[OrchestrationRun]) -> DurationStats {
    let durations: Vec<i64> = runs.iter().filter_map(run_duration_seconds).collect();
    if durations.is_empty() {
        return DurationStats {
            average_label: None,
            note: "Run duration will appear once persisted timestamps accumulate.".into(),
        };
    }

    let average = durations.iter().sum::<i64>() / durations.len() as i64;
    let max = durations.iter().copied().max().unwrap_or(average);
    DurationStats {
        average_label: Some(format_duration(average)),
        note: format!(
            "{} captured runs · longest {}",
            durations.len(),
            format_duration(max)
        ),
    }
}

fn build_status_segments(segments: Vec<(&str, i64, &'static str)>) -> Vec<StatusSegment> {
    let segment_count = segments.len().max(1) as u8;
    let total = segments.iter().map(|(_, value, _)| *value).sum::<i64>();
    segments
        .into_iter()
        .filter(|(_, value, _)| *value > 0 || total == 0)
        .map(|(label, value, tone)| StatusSegment {
            label: label.into(),
            value: value.to_string(),
            tone,
            width: if total == 0 {
                100 / segment_count
            } else {
                ((value as f32 / total as f32) * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as u8
            },
        })
        .collect()
}

fn build_duration_bars(runs: &[OrchestrationRun]) -> Vec<TrendBar> {
    let durations: Vec<(&OrchestrationRun, i64)> = runs
        .iter()
        .take(6)
        .filter_map(|run| run_duration_seconds(run).map(|duration| (run, duration)))
        .collect();
    let max = durations
        .iter()
        .map(|(_, duration)| *duration)
        .max()
        .unwrap_or(0);

    durations
        .into_iter()
        .map(|(run, duration)| TrendBar {
            label: preview(&run.team_name, 18),
            value: format_duration(duration),
            detail: run.status.as_str().into(),
            tone: run_tone(&run.status),
            height: if max == 0 {
                34
            } else {
                ((duration as f32 / max as f32) * 100.0)
                    .round()
                    .clamp(24.0, 100.0) as u8
            },
        })
        .collect()
}

fn message_tone(status: &AgentMessageStatus) -> &'static str {
    match status {
        AgentMessageStatus::Pending => "amber",
        AgentMessageStatus::Delivered => "cyan",
        AgentMessageStatus::Acknowledged => "sage",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_run(
        team_run_id: &str,
        status: RunStatus,
        created_at: &str,
        updated_at: &str,
    ) -> OrchestrationRun {
        OrchestrationRun {
            team_run_id: team_run_id.into(),
            session_key: "discord:test:ops".into(),
            team_name: format!("team-{team_run_id}"),
            workflow: "chain".into(),
            input: "Investigate the live dashboard state".into(),
            status,
            current_step: 1,
            total_steps: 3,
            result: None,
            created_at: created_at.into(),
            updated_at: updated_at.into(),
        }
    }

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
}
