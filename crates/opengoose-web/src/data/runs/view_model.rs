use opengoose_persistence::OrchestrationRun;
use urlencoding::encode;

use super::loader::RunDetailRecord;
use crate::data::utils::{progress_label, run_tone, work_tone};
use crate::data::views::{BroadcastView, MetaRow, RunDetailView, RunListItem, WorkItemView};

pub(in crate::data) fn build_run_list_items(
    runs: &[OrchestrationRun],
    selected_run_id: Option<String>,
    source_label: &str,
) -> Vec<RunListItem> {
    runs.iter()
        .map(|run| RunListItem {
            title: run.team_name.clone(),
            subtitle: format!("{} workflow · {}", run.workflow, source_label),
            updated_at: run.updated_at.clone(),
            progress_label: progress_label(run),
            badge: run.status.as_str().to_uppercase(),
            badge_tone: run_tone(&run.status),
            page_url: format!("/runs?run={}", encode(&run.team_run_id)),
            queue_page_url: format!("/queue?run={}", encode(&run.team_run_id)),
            active: selected_run_id
                .as_ref()
                .map(|selected| selected == &run.team_run_id)
                .unwrap_or(false),
        })
        .collect()
}

pub(in crate::data) fn build_run_detail(
    detail: &RunDetailRecord,
    source_label: &str,
) -> RunDetailView {
    let run = &detail.run;

    RunDetailView {
        title: format!("Run {}", run.team_run_id),
        subtitle: format!("{} / {}", run.team_name, run.workflow),
        source_label: source_label.into(),
        meta: vec![
            MetaRow {
                label: "Status".into(),
                value: run.status.as_str().into(),
            },
            MetaRow {
                label: "Progress".into(),
                value: progress_label(run),
            },
            MetaRow {
                label: "Session".into(),
                value: run.session_key.clone(),
            },
            MetaRow {
                label: "Updated".into(),
                value: run.updated_at.clone(),
            },
        ],
        work_items: detail
            .work_items
            .iter()
            .map(|item| WorkItemView {
                title: item.title.clone(),
                detail: item
                    .assigned_to
                    .clone()
                    .map(|assignee| format!("{assignee} · {}", item.updated_at))
                    .unwrap_or_else(|| item.updated_at.clone()),
                status_label: item.status.as_str().replace('_', " "),
                status_tone: work_tone(&item.status),
                step_label: item
                    .workflow_step
                    .map(|step| format!("Step {step}"))
                    .unwrap_or_else(|| "Root item".into()),
                indent_class: if item.parent_id.is_some() {
                    "is-child"
                } else {
                    "is-root"
                },
            })
            .collect(),
        broadcasts: detail
            .broadcasts
            .iter()
            .map(|message| BroadcastView {
                sender: message.sender.clone(),
                created_at: message.created_at.clone(),
                content: message.content.clone(),
            })
            .collect(),
        input: run.input.clone(),
        result: run
            .result
            .clone()
            .unwrap_or_else(|| "No final result has been recorded yet.".into()),
        empty_hint: "No work items or broadcasts have been captured for this run yet.".into(),
    }
}
