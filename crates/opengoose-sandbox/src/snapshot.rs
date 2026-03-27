use crate::error::{Result, SandboxError};
use crate::hypervisor::VcpuState;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSnapshot {
    pub vcpu_state: VcpuState,
    pub mem_size: usize,
    pub kernel_hash: String,
    pub gic_state: Option<Vec<u8>>,
    pub vtimer_offset: Option<u64>,
    pub virtio_state: Option<crate::virtio::VirtioState>,
    pub virtio_fs_state: Option<crate::virtio_fs::VirtioFsState>,
}

impl VmSnapshot {
    pub fn save(&self, path: &Path) -> Result<()> {
        let data = bincode::serialize(self)
            .map_err(|e| SandboxError::Snapshot(format!("serialize: {e}")))?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read(path)?;
        bincode::deserialize(&data).map_err(|e| SandboxError::Snapshot(format!("deserialize: {e}")))
    }

    pub fn cache_dir() -> Result<std::path::PathBuf> {
        let home =
            std::env::var("HOME").map_err(|_| SandboxError::Snapshot("HOME not set".into()))?;
        let dir = std::path::PathBuf::from(home)
            .join(".opengoose")
            .join("snapshots")
            .join("aarch64");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Delete cached snapshot files, forcing a rebuild on next boot.
    /// Call this after changing the initramfs (e.g., adding BusyBox).
    pub fn invalidate_cache() -> Result<()> {
        let dir = Self::cache_dir()?;
        let meta = dir.join("snapshot.meta");
        let mem = dir.join("snapshot.mem");
        if meta.exists() {
            std::fs::remove_file(&meta)?;
        }
        if mem.exists() {
            std::fs::remove_file(&mem)?;
        }
        Ok(())
    }
}

pub fn cow_map(mem_path: &Path, mem_size: usize) -> Result<(*mut u8, usize)> {
    use std::os::unix::io::AsRawFd;

    let file = std::fs::File::open(mem_path)?;
    let fd = file.as_raw_fd();

    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            mem_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_NORESERVE,
            fd,
            0,
        )
    };

    if ptr == libc::MAP_FAILED {
        return Err(SandboxError::Snapshot("CoW mmap failed".into()));
    }

    // mmap holds a vnode reference; closing the fd is safe after mmap returns.
    drop(file);

    // Pre-fault critical pages to reduce first-exec latency.
    // Only advise the first ~4MB (kernel + page tables + init) — not the full 256MB.
    let prefault_size = (4 * 1024 * 1024).min(mem_size);
    unsafe {
        libc::madvise(ptr, prefault_size, libc::MADV_WILLNEED);
    }

    Ok((ptr as *mut u8, mem_size))
}

/// # Safety
///
/// `mem_ptr` must be a valid pointer to `mem_size` bytes of readable memory.
pub unsafe fn save_memory(mem_ptr: *const u8, mem_size: usize, path: &Path) -> Result<()> {
    let data = unsafe { std::slice::from_raw_parts(mem_ptr, mem_size) };
    std::fs::write(path, data)?;
    Ok(())
}
