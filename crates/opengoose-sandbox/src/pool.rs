use crate::error::{SandboxError, Result};
use crate::snapshot::VmSnapshot;
use crate::vm::MicroVm;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Manages snapshot lifecycle and VM acquisition.
/// First `acquire()` auto-creates and caches the snapshot.
/// HVF constraint: only one VM at a time (sequential reuse).
pub struct SandboxPool {
    snapshot: OnceLock<(VmSnapshot, PathBuf)>,
    lock: Mutex<()>,
}

impl SandboxPool {
    pub fn new() -> Self {
        SandboxPool {
            snapshot: OnceLock::new(),
            lock: Mutex::new(()),
        }
    }

    /// Acquire a forked MicroVm. Creates snapshot on first call.
    /// Only one VM can exist at a time (HVF constraint).
    /// The returned MicroVm must be dropped before calling acquire() again.
    #[cfg(target_os = "macos")]
    pub fn acquire(&self) -> Result<MicroVm> {
        let _guard = self.lock.lock()
            .map_err(|_| SandboxError::Hypervisor("pool lock poisoned".into(), -1))?;
        let (snapshot, mem_path) = match self.snapshot.get() {
            Some(s) => s,
            None => {
                let snap_data = MicroVm::ensure_snapshot()?;
                self.snapshot.get_or_init(|| snap_data)
            }
        };
        MicroVm::fork_from(snapshot, mem_path)
    }
}
