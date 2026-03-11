use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use opengoose_persistence::{
    AgentMessage, AgentMessageStatus, AgentMessageStore, Database, OrchestrationRun, QueueStats,
};

use crate::data::sessions::SessionRecord;
use crate::data::utils::{preview, progress_label, run_tone};
use crate::data::views::ActivityItem;

pub(super) fn build_dashboard_activities(
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

pub(super) fn activity_meta(message: &AgentMessage) -> String {
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

pub(super) fn synthetic_dashboard_activities(
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

fn message_tone(status: &AgentMessageStatus) -> &'static str {
    match status {
        AgentMessageStatus::Pending => "amber",
        AgentMessageStatus::Delivered => "cyan",
        AgentMessageStatus::Acknowledged => "sage",
    }
}
