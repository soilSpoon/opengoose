use super::MetaRow;

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

/// Full detail panel for a selected session, including messages and metadata.
#[derive(Clone)]
pub struct SessionDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub meta: Vec<MetaRow>,
    pub messages: Vec<MessageBubble>,
    pub empty_hint: String,
}

/// View-model for the sessions page (list + selected detail).
#[derive(Clone)]
pub struct SessionsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
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
    fn session_detail_view_empty_messages() {
        let detail = SessionDetailView {
            title: "My Session".into(),
            subtitle: "Discord · guild".into(),
            source_label: "Live".into(),
            meta: vec![MetaRow {
                label: "Key".into(),
                value: "discord:guild:chan".into(),
            }],
            messages: vec![],
            empty_hint: "No messages yet.".into(),
        };
        assert!(detail.messages.is_empty());
        assert_eq!(detail.meta.len(), 1);
    }
}
