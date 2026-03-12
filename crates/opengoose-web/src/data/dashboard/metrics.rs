use opengoose_persistence::OrchestrationRun;

use crate::data::utils::{format_duration, preview, run_duration_seconds, run_tone};
use crate::data::views::{StatusSegment, TrendBar};

pub(super) struct DurationStats {
    pub(super) average_label: Option<String>,
    pub(super) note: String,
}

pub(super) fn duration_stats(runs: &[OrchestrationRun]) -> DurationStats {
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

pub(super) fn build_status_segments(
    segments: Vec<(&str, i64, &'static str)>,
) -> Vec<StatusSegment> {
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

pub(super) fn build_duration_bars(runs: &[OrchestrationRun]) -> Vec<TrendBar> {
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
