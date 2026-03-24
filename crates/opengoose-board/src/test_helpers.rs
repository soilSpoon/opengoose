// 공유 테스트 헬퍼. 크레이트 내 모든 테스트에서 사용.

use crate::board::{AddStampParams, Board};
use crate::work_item::{PostWorkItem, Priority, RigId};

pub async fn new_board() -> Board {
    Board::in_memory().await.unwrap()
}

pub fn post_req(title: &str) -> PostWorkItem {
    PostWorkItem {
        title: title.to_string(),
        description: String::new(),
        created_by: RigId::new("user"),
        priority: Priority::P1,
        tags: vec![],
    }
}

pub fn stamp_params<'a>(
    target_rig: &'a str,
    work_item_id: i64,
    dimension: &'a str,
    score: f32,
    severity: &'a str,
    stamped_by: &'a str,
) -> AddStampParams<'a> {
    AddStampParams {
        target_rig,
        work_item_id,
        dimension,
        score,
        severity,
        stamped_by,
        comment: None,
        active_skill_versions: None,
    }
}
