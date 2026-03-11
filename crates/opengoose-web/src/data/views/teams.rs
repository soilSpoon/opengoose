use super::Notice;

/// Summary row for the team list sidebar.
#[derive(Clone)]
pub struct TeamListItem {
    pub title: String,
    pub subtitle: String,
    pub members: String,
    pub source_label: String,
    pub source_badge: String,
    pub page_url: String,
    pub active: bool,
}

/// Detail/editor panel for a selected team definition.
#[derive(Clone)]
pub struct TeamEditorView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub workflow_label: String,
    pub members_text: String,
    pub original_name: String,
    pub yaml: String,
    pub notice: Option<Notice>,
}

/// View-model for the teams page (list + selected editor).
#[derive(Clone)]
pub struct TeamsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub teams: Vec<TeamListItem>,
    pub selected: TeamEditorView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_editor_view_optional_notice_is_none() {
        let editor = TeamEditorView {
            title: "code-review".into(),
            subtitle: "Multi-agent review".into(),
            source_label: "Live".into(),
            workflow_label: "chain".into(),
            members_text: "reviewer, tester".into(),
            original_name: "code-review".into(),
            yaml: "name: code-review".into(),
            notice: None,
        };
        assert!(editor.notice.is_none());
        assert_eq!(editor.workflow_label, "chain");
    }
}
