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
    pub fn add(
        &mut self,
        from: i64,
        to: i64,
        relation: RelationType,
    ) -> Result<(), BoardError> {
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
        self.reverse.get(&item_id).map(|v| v.as_slice()).unwrap_or(&[])
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
        self.blockers_of(item_id).iter().any(|blocker_id| {
            statuses
                .get(blocker_id)
                .is_none_or(|s| *s != Status::Done)
        })
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
        g.add(1, 2, RelationType::Blocks).unwrap(); // 1 blocks 2

        assert_eq!(g.blockers_of(2), &[1]);
        assert_eq!(g.blocked_by(1), vec![2]);
    }

    #[test]
    fn transitive_blocking() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks).unwrap(); // 1 blocks 2
        g.add(2, 3, RelationType::Blocks).unwrap(); // 2 blocks 3

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
        g.add(1, 2, RelationType::Blocks).unwrap();
        g.add(2, 3, RelationType::Blocks).unwrap();
        // 3 blocks 1 would create cycle: 1→2→3→1
        assert!(g.add(3, 1, RelationType::Blocks).is_err());
    }

    #[test]
    fn is_blocked_checks_status() {
        let mut g = RelationGraph::new();
        g.add(1, 2, RelationType::Blocks).unwrap();

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
        g.add(1, 2, RelationType::Blocks).unwrap();
        g.remove(1, 2);

        assert!(g.blockers_of(2).is_empty());
    }
}
