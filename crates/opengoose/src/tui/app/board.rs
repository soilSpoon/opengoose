use opengoose_board::work_item::{Status, WorkItem};

pub struct BoardState {
    pub items: Vec<WorkItem>,
    pub rigs: Vec<super::RigInfo>,
}

impl BoardState {
    pub fn summary(&self) -> (usize, usize, usize) {
        let open = self
            .items
            .iter()
            .filter(|i| i.status == Status::Open)
            .count();
        let claimed = self
            .items
            .iter()
            .filter(|i| i.status == Status::Claimed)
            .count();
        let done = self
            .items
            .iter()
            .filter(|i| i.status == Status::Done)
            .count();
        (open, claimed, done)
    }

    pub fn active_items(&self) -> Vec<&WorkItem> {
        let mut items: Vec<_> = self
            .items
            .iter()
            .filter(|i| i.status == Status::Open || i.status == Status::Claimed)
            .collect();
        items.sort_by(|a, b| b.priority.urgency().cmp(&a.priority.urgency()));
        items
    }

    pub fn recent_done(&self) -> Vec<&WorkItem> {
        self.items
            .iter()
            .filter(|i| i.status == Status::Done)
            .rev()
            .take(3)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::App;
    use chrono::Utc;
    use opengoose_board::work_item::{Priority, RigId, Status, WorkItem};

    fn make_item(
        id: i64,
        title: &str,
        status: Status,
        priority: Priority,
        claimed_by: Option<&str>,
    ) -> WorkItem {
        WorkItem {
            id,
            title: title.into(),
            description: String::new(),
            created_by: RigId::new("test"),
            created_at: Utc::now(),
            status,
            priority,
            tags: Vec::new(),
            claimed_by: claimed_by.map(RigId::new),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn board_summary_counts_open_claimed_done() {
        let mut app = App::new();
        app.board.items = vec![
            make_item(1, "open", Status::Open, Priority::P1, None),
            make_item(2, "claimed", Status::Claimed, Priority::P1, Some("r1")),
            make_item(3, "done", Status::Done, Priority::P1, Some("r1")),
        ];
        let (open, claimed, done) = app.board_summary();
        assert_eq!(open, 1);
        assert_eq!(claimed, 1);
        assert_eq!(done, 1);
    }

    #[test]
    fn active_items_are_sorted_by_priority() {
        let mut app = App::new();
        app.board.items = vec![
            make_item(1, "low", Status::Open, Priority::P2, None),
            make_item(2, "high", Status::Claimed, Priority::P0, None),
            make_item(3, "mid", Status::Open, Priority::P1, None),
        ];
        let active = app.active_items();
        assert_eq!(
            active.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }

    #[test]
    fn recent_done_limits_to_three_latest() {
        let mut app = App::new();
        app.board.items = vec![
            make_item(1, "done-1", Status::Done, Priority::P1, None),
            make_item(2, "done-2", Status::Done, Priority::P1, None),
            make_item(3, "done-3", Status::Done, Priority::P1, None),
            make_item(4, "done-4", Status::Done, Priority::P1, None),
        ];
        let recent = app.recent_done();
        assert_eq!(
            recent.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![4, 3, 2]
        );
    }
}
