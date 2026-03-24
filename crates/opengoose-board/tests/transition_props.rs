use opengoose_board::work_item::Status;
use proptest::prelude::*;

fn arb_status() -> impl Strategy<Value = Status> {
    prop_oneof![
        Just(Status::Open),
        Just(Status::Claimed),
        Just(Status::Done),
        Just(Status::Stuck),
        Just(Status::Abandoned),
    ]
}

proptest! {
    #[test]
    fn validate_transition_never_panics(from in arb_status(), to in arb_status()) {
        let _ = from.validate_transition(to);
    }

    #[test]
    fn done_is_terminal(to in arb_status()) {
        prop_assert!(Status::Done.validate_transition(to).is_err());
    }

    #[test]
    fn abandoned_is_terminal(to in arb_status()) {
        prop_assert!(Status::Abandoned.validate_transition(to).is_err());
    }

    #[test]
    fn validate_transition_matches_can_transition_to(from in arb_status(), to in arb_status()) {
        let can = from.can_transition_to(to);
        let valid = from.validate_transition(to).is_ok();
        prop_assert_eq!(can, valid);
    }

    #[test]
    fn no_self_transitions(s in arb_status()) {
        prop_assert!(!s.can_transition_to(s));
    }
}
