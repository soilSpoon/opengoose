//! Sandbox — macOS HVF 기반 microVM 샌드박스 (실험적).
//! 에이전트 코드 실행을 격리된 VM에서 수행한다.

pub mod boot;
pub mod client;
pub mod error;
pub mod fuse;
pub mod hypervisor;
pub mod initramfs;
pub mod machine;
pub mod pool;
pub mod snapshot;
pub mod uart;
pub mod virtio;
pub mod virtio_fs;
pub mod vm;
pub mod vring;

pub use error::{Result, SandboxError};
pub use pool::SandboxPool;
pub use vm::ExecResult;
pub use vm::ExitCounts;
pub use vm::MicroVm;

#[cfg(target_os = "macos")]
pub use client::{ApplyResult, SandboxClient, SandboxSession};
