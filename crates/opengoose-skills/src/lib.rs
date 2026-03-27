//! 스킬 카탈로그 — 에이전트가 활용하는 재사용 가능한 지식/패턴의 로딩과 관리.

pub mod error;
pub use error::*;

pub mod skill_name;
pub use skill_name::SkillName;

pub mod catalog;
pub mod evolution;
pub mod lifecycle;
pub mod loader;
pub mod manage;
pub mod metadata;
pub mod source;
#[cfg(test)]
pub(crate) mod test_fixtures;
#[cfg(test)]
pub(crate) mod test_utils;
