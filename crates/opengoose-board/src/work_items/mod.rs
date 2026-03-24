// Work item CRUD operations for Board.
//
// Split into three submodules:
// - transitions: state-changing operations (post, claim, submit, unclaim, mark_stuck, retry, abandon)
// - queries: read-only operations (get, list, ready, claimed_by, completed_by_rig)
// - helpers: internal utilities (transition, sync_item, get_or_err, find_model, blocked_item_ids, compact)

mod helpers;
mod queries;
mod transitions;
