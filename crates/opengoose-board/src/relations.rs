// Relations — 작업 항목 간 의존성 그래프
//
// blocks, depends_on 관계만 (parent_of, relates_to는 Phase 5+)
// 이행적 블로킹 계산, 순환 감지

use crate::work_item::{BoardError, Status};
use std::collections::{HashMap, HashSet, VecDeque};

/// 의존성 관계.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationType {
    /// A blocks B: B는 A가 완료될 때까지 시작 불가.
    Blocks,
}

/// 단일 관계 레코드.
#[derive(Debug, Clone)]
pub struct Relation {
    pub from: i64,
    pub to: i64,
    pub relation: RelationType,
}

/// 의존성 그래프. Board에서 직접 관리 (CowStore 밖).
#[derive(Debug, Clone, Default)]
pub struct RelationGraph {
    /// from_id → [(to_id, relation)]
    edges: HashMap<i64, Vec<(i64, RelationType)>>,
    /// to_id → [from_id] (역방향 인덱스: "나를 블로킹하는 것들")
    reverse: HashMap<i64, Vec<i64>>,
}

impl RelationGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// 관계 추가. 순환 감지 수행.
    pub fn add(&mut self, from: i64, to: i64, relation: RelationType) -> Result<(), BoardError> {
        // 순환 감지: from이 to에 의존하면 순환
        if relation == RelationType::Blocks {
            if from == to {
                return Err(BoardError::CyclicDependency(vec![from, to]));
            }
            if self.would_create_cycle(from, to) {
                return Err(BoardError::CyclicDependency(vec![from, to]));
            }
        }

        self.edges.entry(from).or_default().push((to, relation));
        self.reverse.entry(to).or_default().push(from);
        Ok(())
    }

    /// DB에서 복원할 때 사용. 순환 감지를 건너뜀 (이미 검증된 데이터).
    pub fn restore_edge(&mut self, from: i64, to: i64, relation: RelationType) {
        self.edges.entry(from).or_default().push((to, relation));
        self.reverse.entry(to).or_default().push(from);
    }

    /// 관계 제거.
    pub fn remove(&mut self, from: i64, to: i64) {
        if let Some(edges) = self.edges.get_mut(&from) {
            edges.retain(|(t, _)| *t != to);
        }
        if let Some(rev) = self.reverse.get_mut(&to) {
            rev.retain(|f| *f != from);
        }
    }

    /// 특정 항목을 블로킹하는 항목들 (직접). 슬라이스 반환 — 0 할당.
    pub fn blockers_of(&self, item_id: i64) -> &[i64] {
        self.reverse
            .get(&item_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// 특정 항목이 블로킹하는 항목들 (직접).
    pub fn blocked_by(&self, item_id: i64) -> Vec<i64> {
        self.edges
            .get(&item_id)
            .map(|edges| edges.iter().map(|(to, _)| *to).collect())
            .unwrap_or_default()
    }

    /// 이행적 블로킹: item_id를 직간접적으로 블로킹하는 모든 항목.
    pub fn transitive_blockers_of(&self, item_id: i64) -> HashSet<i64> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        for &blocker in self.blockers_of(item_id) {
            queue.push_back(blocker);
        }

        while let Some(current) = queue.pop_front() {
            if visited.insert(current) {
                for &blocker in self.blockers_of(current) {
                    if !visited.contains(&blocker) {
                        queue.push_back(blocker);
                    }
                }
            }
        }

        visited
    }

    /// 열린 (완료되지 않은) 블로커가 있는지 확인.
    /// statuses: id → Status 매핑.
    pub fn is_blocked(&self, item_id: i64, statuses: &HashMap<i64, Status>) -> bool {
        self.blockers_of(item_id)
            .iter()
            .any(|blocker_id| statuses.get(blocker_id).is_none_or(|s| *s != Status::Done))
    }

    /// 의존성이 걸린 모든 아이템 ID (역방향 인덱스의 키 전체).
    pub fn blocked_item_ids(&self) -> impl Iterator<Item = &i64> {
        self.reverse.keys()
    }

    /// 순환 감지: "from blocks to"를 추가하면 순환이 생기는지.
    /// to에서 시작해서 from에 도달할 수 있으면 순환.
    fn would_create_cycle(&self, from: i64, to: i64) -> bool {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(from);

        while let Some(current) = queue.pop_front() {
            if current == to {
                return true;
            }
            if visited.insert(current) {
                for &blocker in self.blockers_of(current) {
                    if !visited.contains(&blocker) {
                        queue.push_back(blocker);
                    }
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_query_blockers() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");

        assert_eq!(g.blockers_of(2), &[1]);
        assert_eq!(g.blocked_by(1), vec![2]);
    }

    #[test]
    fn transitive_blocking() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");
        g.add(2, 3, RelationType::Blocks)
            .expect("adding 2 blocks 3 should succeed");

        let transitive = g.transitive_blockers_of(3);
        assert!(transitive.contains(&1));
        assert!(transitive.contains(&2));
        assert_eq!(transitive.len(), 2);
    }

    #[test]
    fn self_cycle_rejected() {
        let mut g = RelationGraph::new();
        assert!(g.add(1, 1, RelationType::Blocks).is_err());
    }

    #[test]
    fn cycle_detected() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");
        g.add(2, 3, RelationType::Blocks)
            .expect("adding 2 blocks 3 should succeed");
        // 3 blocks 1 would create cycle: 1→2→3→1
        assert!(g.add(3, 1, RelationType::Blocks).is_err());
    }

    #[test]
    fn is_blocked_checks_status() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");

        let mut statuses = HashMap::new();
        statuses.insert(1, Status::Open);
        statuses.insert(2, Status::Open);

        assert!(g.is_blocked(2, &statuses));

        statuses.insert(1, Status::Done);
        assert!(!g.is_blocked(2, &statuses));
    }

    #[test]
    fn remove_relation() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");
        g.remove(1, 2);

        assert!(g.blockers_of(2).is_empty());
    }

    #[test]
    fn blocked_item_ids_returns_dependency_targets() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");
        g.add(1, 3, RelationType::Blocks)
            .expect("adding 1 blocks 3 should succeed");

        let blocked: std::collections::HashSet<i64> = g.blocked_item_ids().copied().collect();
        assert!(blocked.contains(&2));
        assert!(blocked.contains(&3));
    }

    #[test]
    fn blocked_item_ids_empty_for_new_graph() {
        let g = RelationGraph::new();
        assert_eq!(g.blocked_item_ids().count(), 0);
    }

    #[test]
    fn is_blocked_with_blocker_missing_from_statuses() {
        let mut g = RelationGraph::new();
        g.add(99, 2, RelationType::Blocks)
            .expect("adding 99 blocks 2 should succeed");

        // blocker 99 not in statuses map → treated as not Done → still blocked
        let statuses = std::collections::HashMap::new();
        assert!(g.is_blocked(2, &statuses));
    }

    #[test]
    fn transitive_blockers_returns_empty_for_unblocked() {
        let g = RelationGraph::new();
        assert!(g.transitive_blockers_of(1).is_empty());
    }

    #[test]
    fn blocked_by_returns_empty_for_no_edges() {
        let g = RelationGraph::new();
        assert!(g.blocked_by(1).is_empty());
    }

    #[test]
    fn blockers_of_returns_empty_for_no_reverse() {
        let g = RelationGraph::new();
        assert!(g.blockers_of(1).is_empty());
    }

    #[test]
    fn remove_when_from_has_no_edges_is_noop() {
        let mut g = RelationGraph::new();
        // No edges at all — remove should not panic
        g.remove(99, 100);
        assert!(g.blockers_of(100).is_empty());
    }

    #[test]
    fn remove_when_to_has_no_reverse_is_noop() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");
        // Remove an edge that was never added — reverse[3] doesn't exist
        g.remove(1, 3);
        // Original edge still intact
        assert_eq!(g.blockers_of(2), &[1]);
    }

    #[test]
    fn transitive_blockers_same_blocker_pushed_twice() {
        // W(1) blocks Y(2) and W(1) blocks Z(3), Y(2) blocks X(4), Z(3) blocks X(4)
        // When computing transitive_blockers_of(4): queue=[2,3],
        // processing 2 → push 1; processing 3 → push 1 again (not yet visited)
        // → 1 is popped twice, second pop triggers visited.insert=false branch
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");
        g.add(1, 3, RelationType::Blocks)
            .expect("adding 1 blocks 3 should succeed");
        g.add(2, 4, RelationType::Blocks)
            .expect("adding 2 blocks 4 should succeed");
        g.add(3, 4, RelationType::Blocks)
            .expect("adding 3 blocks 4 should succeed");

        let blockers = g.transitive_blockers_of(4);
        assert_eq!(blockers.len(), 3);
        assert!(blockers.contains(&1));
        assert!(blockers.contains(&2));
        assert!(blockers.contains(&3));
    }

    #[test]
    fn would_create_cycle_same_node_pushed_twice_no_cycle() {
        // 4 blocks 2 and 4 blocks 3; 2 blocks 6; 3 blocks 6
        // would_create_cycle(6, 7): blockers_of(6)=[2,3]
        // → push 4 from both 2 and 3 (before 4 is visited)
        // → 4 popped twice; second pop triggers already-visited branch
        let mut g = RelationGraph::new();
        g.add(4, 2, RelationType::Blocks)
            .expect("adding 4 blocks 2 should succeed");
        g.add(4, 3, RelationType::Blocks)
            .expect("adding 4 blocks 3 should succeed");
        g.add(2, 6, RelationType::Blocks)
            .expect("adding 2 blocks 6 should succeed");
        g.add(3, 6, RelationType::Blocks)
            .expect("adding 3 blocks 6 should succeed");

        // Adding 6→7 has no cycle
        g.add(6, 7, RelationType::Blocks)
            .expect("adding 6 blocks 7 should succeed (no cycle)");
        assert!(g.blocked_by(6).contains(&7));
    }

    #[test]
    fn diamond_graph_no_cycle_already_visited_skipped() {
        // Diamond: 1→3, 2→3, 1→2 — testing "would 3→4 create a cycle?"
        // When traversing from 3: blockers=[1,2], then from 2: blocker=[1] (already visited) → skip
        let mut g = RelationGraph::new();
        g.add(1, 3, RelationType::Blocks)
            .expect("adding 1 blocks 3 should succeed");
        g.add(2, 3, RelationType::Blocks)
            .expect("adding 2 blocks 3 should succeed");
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");

        // Adding 3→4 should succeed (no cycle)
        g.add(3, 4, RelationType::Blocks)
            .expect("adding 3 blocks 4 should succeed (no cycle)");
        assert!(g.blocked_by(3).contains(&4));
    }

    #[test]
    fn restore_edge_skips_cycle_detection() {
        let mut g = RelationGraph::new();
        // Normally 1→2→3→1 would be rejected, but restore_edge skips validation
        g.restore_edge(1, 2, RelationType::Blocks);
        g.restore_edge(2, 3, RelationType::Blocks);
        g.restore_edge(3, 1, RelationType::Blocks); // No error — skips cycle check
        assert_eq!(g.blockers_of(1), &[3]);
    }

    #[test]
    fn transitive_blockers_visited_dedup() {
        // 1→3, 2→3, 1→2: transitive blockers of 3 = {1, 2}, no duplicates
        let mut g = RelationGraph::new();
        g.add(1, 3, RelationType::Blocks)
            .expect("adding 1 blocks 3 should succeed");
        g.add(2, 3, RelationType::Blocks)
            .expect("adding 2 blocks 3 should succeed");
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");

        let blockers = g.transitive_blockers_of(3);
        assert_eq!(blockers.len(), 2);
        assert!(blockers.contains(&1));
        assert!(blockers.contains(&2));
    }

    #[test]
    fn direct_two_node_cycle_rejected() {
        // A→B then B→A should be rejected
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");
        let err = g
            .add(2, 1, RelationType::Blocks)
            .expect_err("2 blocks 1 should create a cycle");
        match err {
            BoardError::CyclicDependency(nodes) => {
                assert!(nodes.contains(&2));
                assert!(nodes.contains(&1));
            }
            other => panic!("expected CyclicDependency, got {other:?}"),
        }
    }

    #[test]
    fn self_referential_returns_cyclic_dependency_error() {
        let mut g = RelationGraph::new();
        let err = g
            .add(42, 42, RelationType::Blocks)
            .expect_err("self-referential relation should fail");
        match err {
            BoardError::CyclicDependency(nodes) => {
                assert_eq!(nodes, vec![42, 42]);
            }
            other => panic!("expected CyclicDependency, got {other:?}"),
        }
    }

    #[test]
    fn long_chain_cycle_rejected() {
        // A→B→C→D, then D→A should be rejected
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("adding 1 blocks 2 should succeed");
        g.add(2, 3, RelationType::Blocks)
            .expect("adding 2 blocks 3 should succeed");
        g.add(3, 4, RelationType::Blocks)
            .expect("adding 3 blocks 4 should succeed");

        let err = g
            .add(4, 1, RelationType::Blocks)
            .expect_err("4 blocks 1 should create a cycle: 1→2→3→4→1");
        match err {
            BoardError::CyclicDependency(_) => {}
            other => panic!("expected CyclicDependency, got {other:?}"),
        }
    }

    #[test]
    fn diamond_dependency_is_not_a_cycle() {
        // A→B, A→C, B→D, C→D — diamond shape, no cycle
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks)
            .expect("A blocks B should succeed");
        g.add(1, 3, RelationType::Blocks)
            .expect("A blocks C should succeed");
        g.add(2, 4, RelationType::Blocks)
            .expect("B blocks D should succeed");
        g.add(3, 4, RelationType::Blocks)
            .expect("C blocks D should succeed");

        // Verify structure
        assert_eq!(g.blocked_by(1), vec![2, 3]);
        assert_eq!(g.blockers_of(4).len(), 2);
        assert!(g.blockers_of(4).contains(&2));
        assert!(g.blockers_of(4).contains(&3));

        // Transitive blockers of D include A, B, C
        let blockers = g.transitive_blockers_of(4);
        assert_eq!(blockers.len(), 3);
        assert!(blockers.contains(&1));
        assert!(blockers.contains(&2));
        assert!(blockers.contains(&3));
    }

    #[test]
    fn diamond_allows_further_edges_without_cycle() {
        // Diamond: A→B, A→C, B→D, C→D. Adding D→E should succeed.
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks).unwrap();
        g.add(1, 3, RelationType::Blocks).unwrap();
        g.add(2, 4, RelationType::Blocks).unwrap();
        g.add(3, 4, RelationType::Blocks).unwrap();
        g.add(4, 5, RelationType::Blocks)
            .expect("D blocks E should succeed — no cycle in diamond + tail");
    }

    #[test]
    fn diamond_rejects_back_edge() {
        // Diamond: A→B, A→C, B→D, C→D. Adding D→A creates a cycle.
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks).unwrap();
        g.add(1, 3, RelationType::Blocks).unwrap();
        g.add(2, 4, RelationType::Blocks).unwrap();
        g.add(3, 4, RelationType::Blocks).unwrap();
        g.add(4, 1, RelationType::Blocks)
            .expect_err("D blocks A should create a cycle through diamond");
    }

    #[test]
    fn deep_chain_without_cycle() {
        let mut g = RelationGraph::new();
        let depth = 50;
        for i in 1..depth {
            g.add(i, i + 1, RelationType::Blocks)
                .unwrap_or_else(|e| panic!("adding {i} blocks {} should succeed: {e:?}", i + 1));
        }

        // Last item is blocked transitively by all preceding items
        let blockers = g.transitive_blockers_of(depth);
        assert_eq!(blockers.len(), (depth - 1) as usize);
        for i in 1..depth {
            assert!(blockers.contains(&i), "blocker {i} should be transitive");
        }

        // Adding depth→1 should create a cycle
        g.add(depth, 1, RelationType::Blocks)
            .expect_err("closing the chain should create a cycle");
    }

    #[test]
    fn deep_chain_is_blocked_only_when_direct_blocker_not_done() {
        let mut g = RelationGraph::new();
        // Chain: 1→2→3→4→5
        for i in 1..5 {
            g.add(i, i + 1, RelationType::Blocks).unwrap();
        }

        let mut statuses: HashMap<i64, Status> = HashMap::new();
        for i in 1..=5 {
            statuses.insert(i, Status::Open);
        }

        // Item 5 is blocked by item 4 (direct blocker is Open)
        assert!(g.is_blocked(5, &statuses));

        // Mark item 4 as Done — item 5 is no longer directly blocked
        statuses.insert(4, Status::Done);
        assert!(!g.is_blocked(5, &statuses));
    }

    #[test]
    fn remove_middle_edge_breaks_transitive_chain() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks).unwrap();
        g.add(2, 3, RelationType::Blocks).unwrap();
        g.add(3, 4, RelationType::Blocks).unwrap();

        // Remove 2→3: chain splits into 1→2 and 3→4
        g.remove(2, 3);

        let blockers_of_4 = g.transitive_blockers_of(4);
        assert_eq!(blockers_of_4.len(), 1);
        assert!(blockers_of_4.contains(&3));
        assert!(!blockers_of_4.contains(&1));
        assert!(!blockers_of_4.contains(&2));
    }

    #[test]
    fn multiple_blockers_all_must_be_done() {
        let mut g = RelationGraph::new();
        g.add(1, 3, RelationType::Blocks).unwrap();
        g.add(2, 3, RelationType::Blocks).unwrap();

        let mut statuses = HashMap::new();
        statuses.insert(1, Status::Done);
        statuses.insert(2, Status::Open);
        statuses.insert(3, Status::Open);

        // Still blocked because 2 is Open
        assert!(g.is_blocked(3, &statuses));

        statuses.insert(2, Status::Done);
        assert!(!g.is_blocked(3, &statuses));
    }

    #[test]
    fn blocked_item_ids_after_remove() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks).unwrap();
        g.add(1, 3, RelationType::Blocks).unwrap();

        g.remove(1, 2);

        // Item 2's reverse entry is now empty but key may still exist;
        // item 3 should still be in blocked_item_ids
        let blocked: HashSet<i64> = g.blocked_item_ids().copied().collect();
        assert!(blocked.contains(&3));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy for a small node ID range to encourage cycles and collisions.
    fn arb_node_id() -> impl Strategy<Value = i64> {
        1..=10i64
    }

    /// Strategy for a list of (from, to) edge pairs.
    fn arb_edges() -> impl Strategy<Value = Vec<(i64, i64)>> {
        prop::collection::vec((arb_node_id(), arb_node_id()), 0..30)
    }

    proptest! {
        #[test]
        fn add_never_panics(edges in arb_edges()) {
            let mut g = RelationGraph::new();
            for (from, to) in edges {
                let _ = g.add(from, to, RelationType::Blocks);
            }
        }

        #[test]
        fn self_edge_always_rejected(node in arb_node_id()) {
            let mut g = RelationGraph::new();
            prop_assert!(g.add(node, node, RelationType::Blocks).is_err());
        }

        #[test]
        fn accepted_edges_never_create_cycles(edges in arb_edges()) {
            // Build a graph from arbitrary edges, accepting only valid ones.
            // Then verify no cycle exists by checking that no node can
            // transitively reach itself.
            let mut g = RelationGraph::new();
            let mut accepted = Vec::new();
            for (from, to) in edges {
                if g.add(from, to, RelationType::Blocks).is_ok() {
                    accepted.push((from, to));
                }
            }

            // For every node, its transitive blockers must not contain itself.
            let all_nodes: HashSet<i64> = accepted
                .iter()
                .flat_map(|(f, t)| [*f, *t])
                .collect();

            for node in &all_nodes {
                let blockers = g.transitive_blockers_of(*node);
                prop_assert!(
                    !blockers.contains(node),
                    "node {} found in its own transitive blockers: {:?}",
                    node,
                    blockers,
                );
            }
        }

        #[test]
        fn blockers_of_subset_of_transitive(edges in arb_edges()) {
            let mut g = RelationGraph::new();
            for (from, to) in &edges {
                let _ = g.add(*from, *to, RelationType::Blocks);
            }

            let all_nodes: HashSet<i64> = edges.iter().flat_map(|(f, t)| [*f, *t]).collect();
            for node in &all_nodes {
                let direct: HashSet<i64> = g.blockers_of(*node).iter().copied().collect();
                let transitive = g.transitive_blockers_of(*node);
                prop_assert!(
                    direct.is_subset(&transitive),
                    "direct blockers {:?} not subset of transitive {:?} for node {}",
                    direct,
                    transitive,
                    node,
                );
            }
        }

        #[test]
        fn remove_then_add_back_succeeds(from in arb_node_id(), to in arb_node_id()) {
            prop_assume!(from != to);
            let mut g = RelationGraph::new();
            g.add(from, to, RelationType::Blocks).expect("first add should succeed");
            g.remove(from, to);
            g.add(from, to, RelationType::Blocks)
                .expect("re-add after remove should succeed");
            prop_assert_eq!(g.blockers_of(to), &[from]);
        }
    }
}
