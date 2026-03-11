use super::{MetaRow, Notice, SelectOption};

/// Summary row for the session list sidebar.
#[derive(Clone)]
pub struct SessionListItem {
    pub title: String,
    pub subtitle: String,
    pub preview: String,
    pub updated_at: String,
    pub badge: String,
    pub badge_tone: &'static str,
    pub page_url: String,
    pub active: bool,
}

/// A single chat message bubble in the session detail view.
#[derive(Clone)]
pub struct MessageBubble {
    pub role_label: String,
    pub author_label: String,
    pub timestamp: String,
    pub content: String,
    pub tone: &'static str,
    pub alignment: &'static str,
}

/// One operator action for exporting a session transcript.
#[derive(Clone)]
pub struct SessionExportAction {
    pub label: String,
    pub href: String,
}

/// Lightweight GET form state for batch session export.
#[derive(Clone)]
pub struct BatchExportFormView {
    pub action_url: String,
    pub since: String,
    pub until: String,
    pub limit: usize,
    pub format_options: Vec<SelectOption>,
    pub hint: String,
}

/// Full detail panel for a selected session, including messages and metadata.
#[derive(Clone)]
pub struct SessionDetailView {
    pub session_key: String,
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub export_actions: Vec<SessionExportAction>,
    pub meta: Vec<MetaRow>,
    pub notice: Option<Notice>,
    pub selected_model: String,
    pub model_options: Vec<SelectOption>,
    pub messages: Vec<MessageBubble>,
    pub empty_hint: String,
}

/// View-model for the sessions page (list + selected detail).
#[derive(Clone)]
pub struct SessionsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub live_stream_url: String,
    pub batch_export: BatchExportFormView,
    pub sessions: Vec<SessionListItem>,
    pub selected: SessionDetailView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_list_item_active_flag() {
        let item = SessionListItem {
            title: "Session A".into(),
            subtitle: "Discord".into(),
            preview: "hello world".into(),
            updated_at: "10:00".into(),
            badge: "DISCORD".into(),
            badge_tone: "cyan",
            page_url: "/sessions?session=abc".into(),
            active: true,
        };
        assert!(item.active);
        assert_eq!(item.badge_tone, "cyan");
    }

    #[test]
    fn session_list_item_inactive_flag() {
        let item = SessionListItem {
            title: "Session B".into(),
            subtitle: "Telegram".into(),
            preview: "hi".into(),
            updated_at: "11:00".into(),
            badge: "TELEGRAM".into(),
            badge_tone: "sage",
            page_url: "/sessions?session=xyz".into(),
            active: false,
        };
        assert!(!item.active);
    }

    #[test]
    fn message_bubble_assistant_tone_and_alignment() {
        let bubble = MessageBubble {
            role_label: "Assistant".into(),
            author_label: "goose".into(),
            timestamp: "10:01".into(),
            content: "Sure, I can help.".into(),
            tone: "accent",
            alignment: "right",
        };
        assert_eq!(bubble.tone, "accent");
        assert_eq!(bubble.alignment, "right");
    }

    #[test]
    fn message_bubble_user_tone_and_alignment() {
        let bubble = MessageBubble {
            role_label: "User".into(),
            author_label: "alice".into(),
            timestamp: "10:00".into(),
            content: "What can you do?".into(),
            tone: "plain",
            alignment: "left",
        };
        assert_eq!(bubble.tone, "plain");
        assert_eq!(bubble.alignment, "left");
    }

    #[test]
    fn session_export_action_preserves_href() {
        let action = SessionExportAction {
            label: "Export JSON".into(),
            href: "/api/sessions/demo/export?format=json".into(),
        };
        assert!(action.href.contains("format=json"));
    }

    #[test]
    fn batch_export_form_tracks_selected_format() {
        let form = BatchExportFormView {
            action_url: "/api/sessions/export".into(),
            since: "7d".into(),
            until: String::new(),
            limit: 100,
            format_options: vec![
                SelectOption {
                    value: "json".into(),
                    label: "JSON".into(),
                    selected: false,
                },
                SelectOption {
                    value: "md".into(),
                    label: "Markdown".into(),
                    selected: true,
                },
            ],
            hint: "Provide at least one time boundary.".into(),
        };
        assert_eq!(form.limit, 100);
        assert_eq!(
            form.format_options
                .iter()
                .filter(|item| item.selected)
                .count(),
            1
        );
    }

    #[test]
    fn session_detail_view_empty_messages() {
        let detail = SessionDetailView {
            session_key: "discord:guild:chan".into(),
            title: "My Session".into(),
            subtitle: "Discord · guild".into(),
            source_label: "Live".into(),
            export_actions: vec![SessionExportAction {
                label: "Export JSON".into(),
                href: "/api/sessions/demo/export?format=json".into(),
            }],
            meta: vec![MetaRow {
                label: "Key".into(),
                value: "discord:guild:chan".into(),
            }],
            notice: None,
            selected_model: String::new(),
            model_options: vec![],
            messages: vec![],
            empty_hint: "No messages yet.".into(),
        };
        assert!(detail.messages.is_empty());
        assert_eq!(detail.meta.len(), 1);
        assert_eq!(detail.export_actions.len(), 1);
    }
}
