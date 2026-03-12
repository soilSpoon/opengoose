use chrono::{NaiveDateTime, Utc};
use opengoose_persistence::{MessageStatus, OrchestrationRun, QueueStats, RunStatus, WorkStatus};

pub(super) fn run_duration_seconds(run: &OrchestrationRun) -> Option<i64> {
    let started = parse_timestamp(&run.created_at)?;
    let finished = match run.status {
        RunStatus::Running => Utc::now().naive_utc(),
        RunStatus::Completed | RunStatus::Failed | RunStatus::Suspended => {
            parse_timestamp(&run.updated_at)?
        }
    };

    let duration = finished.signed_duration_since(started).num_seconds();
    Some(duration.max(0))
}

fn parse_timestamp(value: &str) -> Option<NaiveDateTime> {
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
}

pub(super) fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

pub(super) fn ratio_percent(numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        0
    } else {
        ((numerator as f32 / denominator as f32) * 100.0).round() as usize
    }
}

pub(super) fn choose_selected_name(options: Vec<String>, selected: Option<String>) -> String {
    selected
        .filter(|target| options.iter().any(|candidate| candidate == target))
        .unwrap_or_else(|| options[0].clone())
}

pub(super) fn choose_selected_run(runs: &[OrchestrationRun], selected: Option<String>) -> String {
    selected
        .filter(|target| runs.iter().any(|run| run.team_run_id == *target))
        .unwrap_or_else(|| runs[0].team_run_id.clone())
}

pub(super) fn queue_total(stats: &QueueStats) -> i64 {
    stats.pending + stats.processing + stats.completed + stats.failed + stats.dead
}

pub(super) fn progress_label(run: &OrchestrationRun) -> String {
    format!("{}/{} steps", run.current_step, run.total_steps)
}

pub(super) fn preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut truncated = text.chars().take(max_chars).collect::<String>();
    truncated.push('…');
    truncated
}

pub(super) fn source_badge(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.len() <= 24 {
        return trimmed.to_string();
    }

    if let Some(path) = trimmed.strip_prefix("Saved in ") {
        return format!("Saved in {}", path_leaf(path));
    }

    let leaf = path_leaf(trimmed);
    if leaf != trimmed {
        return leaf;
    }

    preview(trimmed, 24)
}

fn path_leaf(value: &str) -> String {
    value
        .replace('\\', "/")
        .split('/')
        .rfind(|segment| !segment.is_empty())
        .unwrap_or(value)
        .to_string()
}

pub(super) fn platform_tone(platform: &str) -> &'static str {
    match platform {
        "discord" => "cyan",
        "telegram" => "sage",
        "slack" => "amber",
        _ => "neutral",
    }
}

pub(super) fn run_tone(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "cyan",
        RunStatus::Completed => "sage",
        RunStatus::Failed => "rose",
        RunStatus::Suspended => "amber",
    }
}

pub(super) fn work_tone(status: &WorkStatus) -> &'static str {
    match status {
        WorkStatus::Pending => "amber",
        WorkStatus::InProgress => "cyan",
        WorkStatus::Completed => "sage",
        WorkStatus::Failed => "rose",
        WorkStatus::Cancelled => "neutral",
        WorkStatus::Compacted => "neutral",
    }
}

pub(super) fn queue_tone(status: &MessageStatus) -> &'static str {
    match status {
        MessageStatus::Pending => "amber",
        MessageStatus::Processing => "cyan",
        MessageStatus::Completed => "sage",
        MessageStatus::Failed => "rose",
        MessageStatus::Dead => "rose",
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_timestamp, source_badge};

    #[test]
    fn parse_timestamp_accepts_minute_and_second_precision() {
        assert!(parse_timestamp("2026-03-10 10:15:42").is_some());
        assert!(parse_timestamp("2026-03-10 10:15").is_some());
        assert!(parse_timestamp("2026/03/10 10:15").is_none());
    }

    #[test]
    fn source_badge_keeps_short_labels() {
        assert_eq!(source_badge("Bundled default"), "Bundled default");
    }

    #[test]
    fn source_badge_uses_leaf_for_paths() {
        assert_eq!(
            source_badge("/Users/dh/.opengoose/profiles/architect.yaml"),
            "architect.yaml"
        );
    }

    #[test]
    fn source_badge_preserves_saved_in_prefix() {
        assert_eq!(
            source_badge("Saved in /Users/dh/.opengoose/teams"),
            "Saved in teams"
        );
    }
}
