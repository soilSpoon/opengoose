use std::fmt::Write as _;

use super::types::{HistoryMessage, SessionExport};

/// Render a single session export as Markdown.
pub fn render_session_export_markdown(export: &SessionExport) -> String {
    let mut markdown = String::new();
    writeln!(&mut markdown, "# OpenGoose Session Export").expect("write to string");
    writeln!(&mut markdown).expect("write to string");
    write_session_details(&mut markdown, export, "##");

    markdown
}

/// Render multiple session exports as Markdown.
pub fn render_batch_session_exports_markdown(
    exports: &[SessionExport],
    since: Option<&str>,
    until: Option<&str>,
) -> String {
    let mut markdown = String::new();
    writeln!(&mut markdown, "# OpenGoose Session Batch Export").expect("write to string");
    writeln!(&mut markdown).expect("write to string");
    writeln!(&mut markdown, "- Sessions: {}", exports.len()).expect("write to string");
    if let Some(since) = since {
        writeln!(&mut markdown, "- Since: {since}").expect("write to string");
    }
    if let Some(until) = until {
        writeln!(&mut markdown, "- Until: {until}").expect("write to string");
    }
    writeln!(&mut markdown).expect("write to string");

    if exports.is_empty() {
        writeln!(&mut markdown, "_No sessions matched the requested range._")
            .expect("write to string");
        return markdown;
    }

    for export in exports {
        writeln!(&mut markdown, "## Session `{}`", export.session_key).expect("write to string");
        writeln!(&mut markdown).expect("write to string");
        write_session_details(&mut markdown, export, "###");
    }

    markdown
}

fn write_session_details(markdown: &mut String, export: &SessionExport, message_heading: &str) {
    writeln!(markdown, "- Session key: `{}`", export.session_key).expect("write to string");
    writeln!(
        markdown,
        "- Active team: {}",
        export.active_team.as_deref().unwrap_or("-")
    )
    .expect("write to string");
    writeln!(markdown, "- Created at: {}", export.created_at).expect("write to string");
    writeln!(markdown, "- Updated at: {}", export.updated_at).expect("write to string");
    writeln!(markdown, "- Message count: {}", export.message_count).expect("write to string");
    writeln!(markdown).expect("write to string");

    if export.messages.is_empty() {
        writeln!(markdown, "_No messages stored for this session._").expect("write to string");
        return;
    }

    for (index, message) in export.messages.iter().enumerate() {
        writeln!(
            markdown,
            "{message_heading} {}. {}",
            index + 1,
            message_heading_text(message)
        )
        .expect("write to string");
        writeln!(markdown).expect("write to string");
        writeln!(markdown, "```text").expect("write to string");
        writeln!(markdown, "{}", message.content).expect("write to string");
        writeln!(markdown, "```").expect("write to string");
        writeln!(markdown).expect("write to string");
    }
}

fn message_heading_text(message: &HistoryMessage) -> String {
    match message.author.as_deref() {
        Some(author) => format!("{} · {} · {}", message.role, author, message.created_at),
        None => format!("{} · {}", message.role, message.created_at),
    }
}
