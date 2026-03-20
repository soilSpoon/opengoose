// lifecycle.rs — 3-stage decay for learned skills
//
// Active:   0-30 days since last_included_at (or generated_at)
// Dormant:  31-120 days
// Archived: 121+ days

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum Lifecycle {
    Active,
    Dormant,
    Archived,
}

pub fn determine_lifecycle(generated_at: &str, last_included_at: Option<&str>) -> Lifecycle {
    let last = last_included_at
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| {
            DateTime::parse_from_rfc3339(generated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now())
        });

    let days = (Utc::now() - last).num_days();
    if days <= 30 {
        Lifecycle::Active
    } else if days <= 120 {
        Lifecycle::Dormant
    } else {
        Lifecycle::Archived
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_active_when_recent() {
        let now = Utc::now().to_rfc3339();
        assert_eq!(determine_lifecycle(&now, Some(&now)), Lifecycle::Active);
    }

    #[test]
    fn lifecycle_dormant_after_30_days() {
        let old = (Utc::now() - chrono::Duration::days(35)).to_rfc3339();
        assert_eq!(determine_lifecycle(&old, Some(&old)), Lifecycle::Dormant);
    }

    #[test]
    fn lifecycle_archived_after_120_days() {
        let old = (Utc::now() - chrono::Duration::days(150)).to_rfc3339();
        assert_eq!(determine_lifecycle(&old, Some(&old)), Lifecycle::Archived);
    }

    #[test]
    fn lifecycle_uses_generated_at_when_no_last_included() {
        let now = Utc::now().to_rfc3339();
        assert_eq!(determine_lifecycle(&now, None), Lifecycle::Active);
    }

    #[test]
    fn lifecycle_boundary_30_days_is_active() {
        let edge = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        assert_eq!(determine_lifecycle(&edge, Some(&edge)), Lifecycle::Active);
    }

    #[test]
    fn lifecycle_boundary_120_days_is_dormant() {
        let edge = (Utc::now() - chrono::Duration::days(120)).to_rfc3339();
        assert_eq!(determine_lifecycle(&edge, Some(&edge)), Lifecycle::Dormant);
    }

    #[test]
    fn lifecycle_boundary_121_days_is_archived() {
        let edge = (Utc::now() - chrono::Duration::days(121)).to_rfc3339();
        assert_eq!(determine_lifecycle(&edge, Some(&edge)), Lifecycle::Archived);
    }
}
