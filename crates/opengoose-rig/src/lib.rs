// opengoose-rig — Agent Rig (영속 pull 루프)
//
// Goose Agent::reply()를 감싸는 최소 래퍼.
// 메시지 라우팅, 플랫폼 관리, 데이터 저장은 하지 않는다.

pub mod mcp_tools;
pub mod rig;

// Phase 3+에서 구현
// pub mod executor;
// pub mod worktree;
// pub mod portless;
// pub mod witness;
// pub mod middleware;
