#[cfg(target_os = "macos")]
use crate::error::{Result, SandboxError};
#[cfg(target_os = "macos")]
use crate::snapshot::VmSnapshot;
#[cfg(target_os = "macos")]
use crate::vm::MicroVm;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};

/// Manages snapshot lifecycle and VM acquisition.
/// First `acquire()` auto-creates and caches the snapshot.
/// Subsequent calls reuse the VM/vCPU (only swap CoW memory + restore regs).
pub struct SandboxPool {
    #[cfg(target_os = "macos")]
    snapshot: OnceLock<(VmSnapshot, PathBuf)>,
    /// Cached VM for reuse. None = first acquire needs fork_from.
    #[cfg(target_os = "macos")]
    cached_vm: Mutex<Option<MicroVm>>,
}

impl Default for SandboxPool {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxPool {
    pub fn new() -> Self {
        SandboxPool {
            #[cfg(target_os = "macos")]
            snapshot: OnceLock::new(),
            #[cfg(target_os = "macos")]
            cached_vm: Mutex::new(None),
        }
    }

    /// Acquire a forked MicroVm. Creates snapshot on first call.
    /// Second+ calls reuse the VM/vCPU via reset (much faster).
    #[cfg(target_os = "macos")]
    pub fn acquire(&self) -> Result<MicroVm> {
        let (snapshot, mem_path) = match self.snapshot.get() {
            Some(s) => s,
            None => {
                let snap_data = MicroVm::ensure_snapshot()?;
                self.snapshot.get_or_init(|| snap_data)
            }
        };

        let mut guard = self
            .cached_vm
            .lock()
            .map_err(|_| SandboxError::Hypervisor("pool lock poisoned".into(), -1))?;

        match guard.take() {
            Some(mut vm) => {
                // Reuse existing VM — just swap memory and restore registers
                vm.reset(snapshot, mem_path)?;
                Ok(vm)
            }
            None => {
                // First acquire — create VM from scratch
                MicroVm::fork_from(snapshot, mem_path)
            }
        }
    }

    /// Return a VM to the pool for reuse.
    #[cfg(target_os = "macos")]
    pub fn release(&self, vm: MicroVm) {
        if let Ok(mut guard) = self.cached_vm.lock() {
            *guard = Some(vm);
        }
    }
}
