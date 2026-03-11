use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::ApiKeyStore;

use crate::data::{ApiKeyRowView, ApiKeysPageView, MetricCard};

/// Build the dashboard view-model for API key management.
pub fn load_api_keys_page(store: Arc<ApiKeyStore>) -> Result<ApiKeysPageView> {
    let keys = store.list()?;
    let used_count = keys.iter().filter(|key| key.last_used_at.is_some()).count();
    let described_count = keys.iter().filter(|key| key.description.is_some()).count();
    let never_used_count = keys.len().saturating_sub(used_count);

    let summary = if keys.is_empty() {
        "No shared credentials have been issued yet. Generate one here before connecting remote agents or authenticated API clients.".into()
    } else if never_used_count == 0 {
        format!(
            "{} active credential(s). Every key has authenticated at least once, so usage timestamps are available for rotation planning.",
            keys.len()
        )
    } else {
        format!(
            "{} active credential(s). {} have never been used, which makes it easier to spot stale bootstrap secrets before they spread.",
            keys.len(),
            never_used_count
        )
    };

    let rows = keys
        .into_iter()
        .map(|key| {
            let (usage_label, usage_tone, last_used_at) = match key.last_used_at {
                Some(timestamp) => ("Used", "success", timestamp),
                None => ("Never used", "neutral", "Never".into()),
            };

            ApiKeyRowView {
                id: key.id,
                description_label: key.description.unwrap_or_else(|| "No description".into()),
                created_at: key.created_at,
                last_used_at,
                usage_label: usage_label.into(),
                usage_tone,
            }
        })
        .collect();

    Ok(ApiKeysPageView {
        mode_label: if used_count + never_used_count == 0 {
            "No keys issued".into()
        } else if described_count > 0 {
            "Managed credentials".into()
        } else if never_used_count > 0 {
            "Bootstrap credentials".into()
        } else {
            "Active credentials".into()
        },
        mode_tone: if used_count + never_used_count == 0 {
            "neutral"
        } else if described_count > 0 {
            "success"
        } else if never_used_count > 0 {
            "amber"
        } else {
            "success"
        },
        summary,
        metrics: vec![
            MetricCard {
                label: "Active keys".into(),
                value: (used_count + never_used_count).to_string(),
                note: "Stored credentials that can still authenticate requests".into(),
                tone: "cyan",
            },
            MetricCard {
                label: "Used".into(),
                value: used_count.to_string(),
                note: "Keys with at least one successful authentication".into(),
                tone: "sage",
            },
            MetricCard {
                label: "Never used".into(),
                value: never_used_count.to_string(),
                note: "Fresh or stale keys that have no last-used timestamp yet".into(),
                tone: "amber",
            },
            MetricCard {
                label: "Described".into(),
                value: described_count.to_string(),
                note: "Keys carrying operator-provided ownership context".into(),
                tone: "neutral",
            },
        ],
        keys: rows,
        notice: None,
        generated_key: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_persistence::Database;

    #[test]
    fn load_api_keys_page_reports_empty_state() {
        let store = Arc::new(ApiKeyStore::new(Arc::new(
            Database::open_in_memory().expect("db should open"),
        )));

        let page = load_api_keys_page(store).expect("page should load");

        assert_eq!(page.mode_label, "No keys issued");
        assert!(page.keys.is_empty());
    }

    #[test]
    fn load_api_keys_page_marks_used_and_unused_rows() {
        let store = Arc::new(ApiKeyStore::new(Arc::new(
            Database::open_in_memory().expect("db should open"),
        )));
        let seeded = store.generate(Some("remote")).expect("key should generate");
        store.generate(None).expect("unused key should generate");
        store
            .validate(&seeded.plaintext)
            .expect("validation should update last used");

        let page = load_api_keys_page(store).expect("page should load");

        assert_eq!(page.keys.len(), 2);
        assert!(page.keys.iter().any(|key| key.usage_label == "Used"));
        assert!(page.keys.iter().any(|key| key.usage_label == "Never used"));
    }
}
