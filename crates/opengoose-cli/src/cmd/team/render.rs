use opengoose_persistence::WorkStatus;

pub(super) fn work_status_icon(status: &WorkStatus) -> &'static str {
    match status {
        WorkStatus::Completed => "✓",
        WorkStatus::InProgress => "▶",
        WorkStatus::Failed => "✗",
        WorkStatus::Pending => "○",
        WorkStatus::Cancelled => "⊘",
    }
}

pub(super) fn preview_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }

    let end = text.floor_char_boundary(max_bytes);
    format!("{}...", &text[..end])
}
