use opengoose_board::merge::{LwwField, Mergeable, merge_tags};
use opengoose_board::work_item::{Priority, RigId, Status, WorkItem};

use chrono::{TimeZone, Utc};
use proptest::prelude::*;

// ── Arbitrary strategies ─────────────────────────────────────

fn arb_timestamp() -> impl Strategy<Value = chrono::DateTime<Utc>> {
    (0i64..365 * 24 * 3600).prop_map(|secs| {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::seconds(secs)
    })
}

fn arb_status() -> impl Strategy<Value = Status> {
    prop_oneof![
        Just(Status::Open),
        Just(Status::Claimed),
        Just(Status::Done),
        Just(Status::Stuck),
        Just(Status::Abandoned),
    ]
}

fn arb_work_item() -> impl Strategy<Value = WorkItem> {
    (
        1i64..1000,
        "[a-z]{1,10}",
        "[a-z]{0,20}",
        arb_status(),
        arb_priority(),
        arb_tags(),
        arb_timestamp(),
    )
        .prop_map(|(id, title, desc, status, priority, tags, ts)| WorkItem {
            id,
            title,
            description: desc,
            created_by: RigId::new("test"),
            created_at: ts,
            status,
            priority,
            tags,
            claimed_by: None,
            updated_at: ts,
        })
}

fn arb_priority() -> impl Strategy<Value = Priority> {
    prop_oneof![Just(Priority::P0), Just(Priority::P1), Just(Priority::P2)]
}

fn arb_tags() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-z]{1,5}", 0..5)
}

fn arb_lww_field() -> impl Strategy<Value = LwwField<String>> {
    ("[a-z]{1,10}", arb_timestamp()).prop_map(|(v, t)| LwwField {
        value: v,
        updated_at: t,
    })
}

// ── LWW commutativity ────────────────────────────────────────

proptest! {
    #[test]
    fn lww_merge_is_commutative(a in arb_lww_field(), b in arb_lww_field()) {
        if a.updated_at != b.updated_at {
            prop_assert_eq!(a.merge(&b).value, b.merge(&a).value);
        }
    }
}

// ── LWW idempotency ─────────────────────────────────────────

proptest! {
    #[test]
    fn lww_merge_is_idempotent(a in arb_lww_field()) {
        prop_assert_eq!(a.merge(&a).value, a.value);
    }
}

// ── Priority max-register properties ─────────────────────────

proptest! {
    #[test]
    fn priority_merge_is_commutative(a in arb_priority(), b in arb_priority()) {
        prop_assert_eq!(a.merge(&b), b.merge(&a));
    }

    #[test]
    fn priority_merge_is_associative(a in arb_priority(), b in arb_priority(), c in arb_priority()) {
        prop_assert_eq!(a.merge(&b).merge(&c), a.merge(&b.merge(&c)));
    }

    #[test]
    fn priority_merge_is_idempotent(a in arb_priority()) {
        prop_assert_eq!(a.merge(&a), a);
    }
}

// ── Tags G-Set properties ───────────────────────────────────

proptest! {
    #[test]
    fn tags_merge_is_commutative(a in arb_tags(), b in arb_tags()) {
        prop_assert_eq!(merge_tags(&a, &b), merge_tags(&b, &a));
    }

    #[test]
    fn tags_merge_is_superset(a in arb_tags(), b in arb_tags()) {
        let merged = merge_tags(&a, &b);
        for tag in &a { prop_assert!(merged.contains(tag)); }
        for tag in &b { prop_assert!(merged.contains(tag)); }
    }
}

// ── Merge associativity (component-level) ────────────────────

proptest! {
    #[test]
    fn merge_is_associative(a in arb_work_item(), b in arb_work_item(), c in arb_work_item()) {
        // Priority: max-register is associative
        let ab_c = a.priority.merge(&b.priority).merge(&c.priority);
        let a_bc = a.priority.merge(&b.priority.merge(&c.priority));
        prop_assert_eq!(ab_c, a_bc);

        // Tags: G-Set union is associative
        let ab_c_tags = merge_tags(&merge_tags(&a.tags, &b.tags), &c.tags);
        let a_bc_tags = merge_tags(&a.tags, &merge_tags(&b.tags, &c.tags));
        prop_assert_eq!(ab_c_tags, a_bc_tags);
    }
}

// ── WorkItem serde roundtrip ─────────────────────────────────

proptest! {
    #[test]
    fn work_item_serde_roundtrip(item in arb_work_item()) {
        let json = serde_json::to_string(&item).expect("serialize");
        let back: WorkItem = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(item.id, back.id);
        prop_assert_eq!(item.status, back.status);
        prop_assert_eq!(item.priority, back.priority);
        prop_assert_eq!(item.tags, back.tags);
    }
}
