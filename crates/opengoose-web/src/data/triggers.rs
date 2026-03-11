use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, Trigger, TriggerStore};
use urlencoding::encode;

use crate::data::views::{MetaRow, TriggerDetailView, TriggerListItem, TriggersPageView};

/// Load the triggers page view-model, optionally selecting a trigger by name.
pub fn load_triggers_page(db: Arc<Database>, selected: Option<String>) -> Result<TriggersPageView> {
    let store = TriggerStore::new(db);
    let triggers = store.list()?;

    let selected_name = if triggers.is_empty() {
        String::new()
    } else {
        selected
            .filter(|target| triggers.iter().any(|t| &t.name == target))
            .unwrap_or_else(|| triggers[0].name.clone())
    };

    let selected_trigger = triggers.iter().find(|t| t.name == selected_name);

    Ok(TriggersPageView {
        mode_label: if triggers.is_empty() {
            "No triggers configured".into()
        } else {
            format!("{} trigger(s)", triggers.len())
        },
        mode_tone: if triggers.is_empty() {
            "neutral"
        } else {
            "success"
        },
        triggers: triggers
            .iter()
            .map(|t| build_trigger_list_item(t, &selected_name))
            .collect(),
        selected: match selected_trigger {
            Some(t) => build_trigger_detail(t),
            None => placeholder_trigger_detail(),
        },
    })
}

fn build_trigger_list_item(trigger: &Trigger, selected_name: &str) -> TriggerListItem {
    TriggerListItem {
        title: trigger.name.clone(),
        subtitle: format_trigger_type(&trigger.trigger_type),
        team_label: trigger.team_name.clone(),
        status_label: if trigger.enabled {
            "enabled".into()
        } else {
            "disabled".into()
        },
        status_tone: if trigger.enabled {
            "success"
        } else {
            "neutral"
        },
        last_fired: trigger
            .last_fired_at
            .clone()
            .unwrap_or_else(|| "never".into()),
        page_url: format!("/triggers?trigger={}", encode(&trigger.name)),
        active: trigger.name == selected_name,
    }
}

fn build_trigger_detail(trigger: &Trigger) -> TriggerDetailView {
    let meta = vec![
        MetaRow {
            label: "Type".into(),
            value: format_trigger_type(&trigger.trigger_type),
        },
        MetaRow {
            label: "Team".into(),
            value: trigger.team_name.clone(),
        },
        MetaRow {
            label: "Fire count".into(),
            value: trigger.fire_count.to_string(),
        },
        MetaRow {
            label: "Last fired".into(),
            value: trigger
                .last_fired_at
                .clone()
                .unwrap_or_else(|| "never".into()),
        },
        MetaRow {
            label: "Created".into(),
            value: trigger.created_at.clone(),
        },
    ];

    TriggerDetailView {
        name: trigger.name.clone(),
        trigger_type: trigger.trigger_type.clone(),
        team_name: trigger.team_name.clone(),
        input: trigger.input.clone(),
        condition_json: trigger.condition_json.clone(),
        enabled: trigger.enabled,
        fire_count: trigger.fire_count,
        last_fired_at: trigger
            .last_fired_at
            .clone()
            .unwrap_or_else(|| "never".into()),
        created_at: trigger.created_at.clone(),
        meta,
        status_label: if trigger.enabled {
            "enabled".into()
        } else {
            "disabled".into()
        },
        status_tone: if trigger.enabled {
            "success"
        } else {
            "neutral"
        },
        notice: None,
        is_placeholder: false,
    }
}

fn placeholder_trigger_detail() -> TriggerDetailView {
    TriggerDetailView {
        name: String::new(),
        trigger_type: String::new(),
        team_name: String::new(),
        input: String::new(),
        condition_json: "{}".into(),
        enabled: false,
        fire_count: 0,
        last_fired_at: "never".into(),
        created_at: String::new(),
        meta: vec![],
        status_label: "none".into(),
        status_tone: "neutral",
        notice: None,
        is_placeholder: true,
    }
}

fn format_trigger_type(t: &str) -> String {
    match t {
        "webhook_received" => "Webhook".into(),
        "file_watch" => "File watch".into(),
        "cron" => "Cron".into(),
        "message_received" => "Message received".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn load_triggers_page_empty_returns_placeholder() {
        let db = test_db();
        let page = load_triggers_page(db, None).unwrap();
        assert!(page.triggers.is_empty());
        assert!(page.selected.is_placeholder);
        assert_eq!(page.mode_tone, "neutral");
    }

    #[test]
    fn load_triggers_page_selects_first_by_default() {
        let db = test_db();
        let store = TriggerStore::new(db.clone());
        store
            .create(
                "alpha",
                "webhook_received",
                r#"{"path":"/a"}"#,
                "team-a",
                "",
            )
            .unwrap();
        store
            .create("beta", "file_watch", "{}", "team-b", "")
            .unwrap();

        let page = load_triggers_page(db, None).unwrap();
        assert_eq!(page.triggers.len(), 2);
        assert!(page.selected.name == "alpha" || page.selected.name == "beta");
        assert!(!page.selected.is_placeholder);
        assert_eq!(page.mode_tone, "success");
    }

    #[test]
    fn load_triggers_page_selects_named_trigger() {
        let db = test_db();
        let store = TriggerStore::new(db.clone());
        store
            .create("alpha", "webhook_received", "{}", "team-a", "")
            .unwrap();
        store
            .create("beta", "file_watch", "{}", "team-b", "")
            .unwrap();

        let page = load_triggers_page(db, Some("beta".into())).unwrap();
        assert_eq!(page.selected.name, "beta");
        assert_eq!(page.selected.trigger_type, "file_watch");
        assert!(
            page.triggers
                .iter()
                .find(|t| t.title == "beta")
                .unwrap()
                .active
        );
        assert!(
            !page
                .triggers
                .iter()
                .find(|t| t.title == "alpha")
                .unwrap()
                .active
        );
    }

    #[test]
    fn load_triggers_page_invalid_selection_falls_back_to_first() {
        let db = test_db();
        let store = TriggerStore::new(db.clone());
        store
            .create("only-trigger", "webhook_received", "{}", "team-a", "")
            .unwrap();

        let page = load_triggers_page(db, Some("no-such-trigger".into())).unwrap();
        assert_eq!(page.selected.name, "only-trigger");
    }

    #[test]
    fn trigger_list_item_enabled_disabled_tone() {
        let db = test_db();
        let store = TriggerStore::new(db.clone());
        store
            .create("enabled-trigger", "webhook_received", "{}", "team-a", "")
            .unwrap();
        store
            .create("disabled-trigger", "webhook_received", "{}", "team-b", "")
            .unwrap();
        store.set_enabled("disabled-trigger", false).unwrap();

        let page = load_triggers_page(db, None).unwrap();
        let enabled_item = page
            .triggers
            .iter()
            .find(|t| t.title == "enabled-trigger")
            .unwrap();
        let disabled_item = page
            .triggers
            .iter()
            .find(|t| t.title == "disabled-trigger")
            .unwrap();
        assert_eq!(enabled_item.status_tone, "success");
        assert_eq!(disabled_item.status_tone, "neutral");
    }

    #[test]
    fn format_trigger_type_labels() {
        assert_eq!(format_trigger_type("webhook_received"), "Webhook");
        assert_eq!(format_trigger_type("file_watch"), "File watch");
        assert_eq!(format_trigger_type("cron"), "Cron");
        assert_eq!(format_trigger_type("message_received"), "Message received");
        assert_eq!(format_trigger_type("unknown_type"), "unknown_type");
    }
}
