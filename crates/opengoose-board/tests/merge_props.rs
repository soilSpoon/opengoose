use opengoose_board::merge::{merge_tags, LwwField, Mergeable};
use opengoose_board::work_item::Priority;

use chrono::{TimeZone, Utc};
use proptest::prelude::*;

// ── Arbitrary strategies ─────────────────────────────────────

fn arb_timestamp() -> impl Strategy<Value = chrono::DateTime<Utc>> {
    (0i64..365 * 24 * 3600).prop_map(|secs| {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::seconds(secs)
    })
}

fn arb_priority() -> impl Strategy<Value = Priority> {
    prop_oneof![Just(Priority::P0), Just(Priority::P1), Just(Priority::P2)]
}

fn arb_tags() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-z]{1,5}", 0..5)
}

fn arb_lww_field() -> impl Strategy<Value = LwwField<String>> {
    ("[a-z]{1,10}", arb_timestamp())
        .prop_map(|(v, t)| LwwField {
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
