# Sandbox virtio-fs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement virtio-fs in the opengoose-sandbox VMM so the guest can mount a host directory as a read-only filesystem, enabling the sandbox integration pipeline described in the design spec.

**Architecture:** A second virtio-mmio device (device ID 26, virtio-fs) is added alongside the existing virtio-console. The VMM acts as a FUSE server: guest kernel sends FUSE requests via virtqueue, VMM translates them to host syscalls against a configured directory. An inode table maps guest inodes to host paths. Overlay setup happens in guest init.

**Tech Stack:** Rust, HVF (macOS Hypervisor.framework), virtio-mmio v2, FUSE protocol 7.31, Linux virtio-fs driver (built into Alpine linux-virt 6.12)

**Scope:** This plan covers Phase 1 only (virtio-fs device + FUSE server + guest mount). Phase 2 (SandboxClient McpClientTrait) and Phase 3 (Worker integration) will be separate plans.

---

## File Structure

**New files:**
- `crates/opengoose-sandbox/src/virtio_fs.rs` — VirtioFs device: MMIO registers, virtqueue, feature negotiation, FUSE dispatch
- `crates/opengoose-sandbox/src/fuse/mod.rs` — FUSE protocol types (opcodes, header structs), request/response codec
- `crates/opengoose-sandbox/src/fuse/ops.rs` — Individual FUSE operation handlers (LOOKUP, READ, WRITE, etc.)
- `crates/opengoose-sandbox/src/fuse/inode_table.rs` — Inode ↔ host path bidirectional mapping
- `crates/opengoose-sandbox/tests/virtio_fs_test.rs` — Integration test: mount + read + write(overlay) + git diff

**Modified files:**
- `crates/opengoose-sandbox/src/machine.rs` — Add `VIRTIO_FS_MMIO_BASE`, `VIRTIO_FS_IRQ`, second DTB node
- `crates/opengoose-sandbox/src/lib.rs` — Add `pub mod fuse; pub mod virtio_fs;`
- `crates/opengoose-sandbox/src/boot.rs` — Route MMIO reads/writes for second virtio device in `BootedVm::step_once`
- `crates/opengoose-sandbox/src/vm.rs` — Route MMIO for second virtio device in `MicroVm::step_once`, add `mount_virtio_fs(&path)` method, add `VirtioFs` field
- `crates/opengoose-sandbox/src/snapshot.rs` — Add `VirtioFsState` to `VmSnapshot` for tag/root_dir serialization
- `crates/opengoose-sandbox/guest/init/src/main.rs` — Mount virtiofs at `/workspace`, set up overlayfs

---

### Task 1: Machine constants + DTB for second virtio-mmio device

**Files:**
- Modify: `crates/opengoose-sandbox/src/machine.rs`

The second virtio-mmio device needs its own MMIO address range and SPI interrupt line, plus a DTB node so the guest kernel discovers it.

- [ ] **Step 1: Write test for new constants**

```rust
// Add to existing tests in machine.rs
#[test]
fn virtio_fs_mmio_does_not_overlap_console() {
    let console_end = VIRTIO_MMIO_BASE + VIRTIO_MMIO_SIZE;
    assert!(
        VIRTIO_FS_MMIO_BASE >= console_end,
        "virtio-fs MMIO must not overlap virtio-console"
    );
}

#[test]
fn virtio_fs_irq_differs_from_console() {
    assert_ne!(VIRTIO_FS_IRQ, VIRTIO_IRQ);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opengoose-sandbox virtio_fs_mmio_does_not_overlap`
Expected: FAIL — `VIRTIO_FS_MMIO_BASE` not found

- [ ] **Step 3: Add constants**

Add to `crates/opengoose-sandbox/src/machine.rs` after existing virtio constants:

```rust
/// Virtio-mmio fs device (second virtio device)
pub const VIRTIO_FS_MMIO_BASE: u64 = 0x0A00_0200; // right after console's 0x200
pub const VIRTIO_FS_MMIO_SIZE: u64 = 0x200;
pub const VIRTIO_FS_IRQ: u32 = 3; // SPI 3 (console uses SPI 2)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p opengoose-sandbox virtio_fs_mmio`
Expected: PASS

- [ ] **Step 5: Add DTB node for virtio-fs**

In `create_dtb_with_initrd`, add a second virtio_mmio node after the existing one:

```rust
// Virtio-mmio fs device
{
    let virtio_fs = fdt
        .begin_node(&format!("virtio_mmio@{VIRTIO_FS_MMIO_BASE:x}"))
        .map_err(map_err)?;
    fdt.property_string("compatible", "virtio,mmio")
        .map_err(map_err)?;
    fdt.property("reg", &prop64(&[VIRTIO_FS_MMIO_BASE, VIRTIO_FS_MMIO_SIZE]))
        .map_err(map_err)?;
    fdt.property(
        "interrupts",
        &prop32(&[GIC_FDT_IRQ_TYPE_SPI, VIRTIO_FS_IRQ, IRQ_TYPE_LEVEL_HI]),
    )
    .map_err(map_err)?;
    fdt.end_node(virtio_fs).map_err(map_err)?;
}
```

- [ ] **Step 6: Run full test suite**

Run: `cargo test -p opengoose-sandbox`
Expected: All existing tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose-sandbox/src/machine.rs
git commit -m "feat(sandbox): add virtio-fs MMIO constants and DTB node"
```

---

### Task 2: FUSE protocol types and codec

**Files:**
- Create: `crates/opengoose-sandbox/src/fuse/mod.rs`

The FUSE protocol uses a fixed header followed by operation-specific data. All integers are little-endian. The VMM must parse requests from the virtqueue and build responses.

Reference: Linux `include/uapi/linux/fuse.h` — we only need the subset for our ~16 ops.

- [ ] **Step 1: Create fuse module with protocol types**

Create `crates/opengoose-sandbox/src/fuse/mod.rs`:

```rust
//! FUSE protocol types and codec for virtio-fs.
//! Reference: Linux include/uapi/linux/fuse.h (protocol 7.31)

pub mod inode_table;
pub mod ops;

/// FUSE protocol version
pub const FUSE_KERNEL_VERSION: u32 = 7;
pub const FUSE_KERNEL_MINOR_VERSION: u32 = 31;

/// FUSE opcodes (subset needed for cargo build/test)
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Lookup = 1,
    Forget = 2,
    Getattr = 3,
    // Setattr = 4,  // ENOSYS
    Mkdir = 9,
    Unlink = 10,
    Rmdir = 11,
    Rename = 12,
    Open = 14,
    Read = 15,
    Write = 16,
    Statfs = 17,
    Release = 18,
    Fsync = 20,
    Opendir = 27,
    Readdir = 28,
    Releasedir = 29,
    Init = 26,
    Create = 35,
    Readdirplus = 44,
    Destroy = 38,
    Flush = 25,
}

impl Opcode {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::Lookup),
            2 => Some(Self::Forget),
            3 => Some(Self::Getattr),
            9 => Some(Self::Mkdir),
            10 => Some(Self::Unlink),
            11 => Some(Self::Rmdir),
            12 => Some(Self::Rename),
            14 => Some(Self::Open),
            15 => Some(Self::Read),
            16 => Some(Self::Write),
            17 => Some(Self::Statfs),
            18 => Some(Self::Release),
            20 => Some(Self::Fsync),
            25 => Some(Self::Flush),
            26 => Some(Self::Init),
            27 => Some(Self::Opendir),
            28 => Some(Self::Readdir),
            29 => Some(Self::Releasedir),
            35 => Some(Self::Create),
            38 => Some(Self::Destroy),
            44 => Some(Self::Readdirplus),
            _ => None,
        }
    }
}

/// FUSE request header (40 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseInHeader {
    pub len: u32,
    pub opcode: u32,
    pub unique: u64,
    pub nodeid: u64,
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
    pub padding: u32,
}

/// FUSE response header (16 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseOutHeader {
    pub len: u32,
    pub error: i32,
    pub unique: u64,
}

/// FUSE_INIT request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseInitIn {
    pub major: u32,
    pub minor: u32,
    pub max_readahead: u32,
    pub flags: u32,
}

/// FUSE_INIT response body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseInitOut {
    pub major: u32,
    pub minor: u32,
    pub max_readahead: u32,
    pub flags: u32,
    pub max_background: u16,
    pub congestion_threshold: u16,
    pub max_write: u32,
    pub time_gran: u32,
    pub max_pages: u16,
    pub map_alignment: u16,
    pub unused: [u32; 8],
}

/// FUSE file attributes (matches struct fuse_attr)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FuseAttr {
    pub ino: u64,
    pub size: u64,
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub atimensec: u32,
    pub mtimensec: u32,
    pub ctimensec: u32,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u32,
    pub blksize: u32,
    pub flags: u32,
}

/// FUSE_LOOKUP response / FUSE_CREATE response entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseEntryOut {
    pub nodeid: u64,
    pub generation: u64,
    pub entry_valid: u64,
    pub attr_valid: u64,
    pub entry_valid_nsec: u32,
    pub attr_valid_nsec: u32,
    pub attr: FuseAttr,
}

/// FUSE_GETATTR response
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseAttrOut {
    pub attr_valid: u64,
    pub attr_valid_nsec: u32,
    pub dummy: u32,
    pub attr: FuseAttr,
}

/// FUSE_OPEN / FUSE_OPENDIR request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseOpenIn {
    pub flags: u32,
    pub open_flags: u32,
}

/// FUSE_OPEN / FUSE_OPENDIR response
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseOpenOut {
    pub fh: u64,
    pub open_flags: u32,
    pub padding: u32,
}

/// FUSE_READ / FUSE_READDIR request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseReadIn {
    pub fh: u64,
    pub offset: u64,
    pub size: u32,
    pub read_flags: u32,
    pub lock_owner: u64,
    pub flags: u32,
    pub padding: u32,
}

/// FUSE_WRITE request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseWriteIn {
    pub fh: u64,
    pub offset: u64,
    pub size: u32,
    pub write_flags: u32,
    pub lock_owner: u64,
    pub flags: u32,
    pub padding: u32,
}

/// FUSE_WRITE response
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseWriteOut {
    pub size: u32,
    pub padding: u32,
}

/// FUSE_CREATE request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseCreateIn {
    pub flags: u32,
    pub mode: u32,
    pub umask: u32,
    pub open_flags: u32,
}

/// FUSE_MKDIR request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseMkdirIn {
    pub mode: u32,
    pub umask: u32,
}

/// FUSE_RENAME request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseRenameIn {
    pub newdir: u64,
}

/// FUSE_STATFS response
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseStatfsOut {
    pub st: FuseKstatfs,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseKstatfs {
    pub blocks: u64,
    pub bfree: u64,
    pub bavail: u64,
    pub files: u64,
    pub ffree: u64,
    pub bsize: u32,
    pub namelen: u32,
    pub frsize: u32,
    pub padding: u32,
    pub spare: [u32; 6],
}

/// FUSE_FLUSH request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseFlushIn {
    pub fh: u64,
    pub unused: u32,
    pub padding: u32,
    pub lock_owner: u64,
}

/// FUSE_RELEASE / FUSE_RELEASEDIR request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseReleaseIn {
    pub fh: u64,
    pub flags: u32,
    pub release_flags: u32,
    pub lock_owner: u64,
}

/// FUSE readdir entry (variable length, followed by name)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseDirent {
    pub ino: u64,
    pub off: u64,
    pub namelen: u32,
    pub typ: u32,
    // followed by name[namelen], padded to 8-byte boundary
}

/// FUSE_GETATTR request body
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FuseGetattrIn {
    pub getattr_flags: u32,
    pub dummy: u32,
    pub fh: u64,
}

pub const FUSE_IN_HEADER_SIZE: usize = std::mem::size_of::<FuseInHeader>();
pub const FUSE_OUT_HEADER_SIZE: usize = std::mem::size_of::<FuseOutHeader>();

/// Parse a FuseInHeader from raw bytes.
pub fn parse_in_header(data: &[u8]) -> Option<FuseInHeader> {
    if data.len() < FUSE_IN_HEADER_SIZE {
        return None;
    }
    Some(unsafe { std::ptr::read_unaligned(data.as_ptr() as *const FuseInHeader) })
}

/// Parse a struct T from bytes at the given offset.
///
/// # Safety
/// T must be a plain-old-data type with no padding requirements beyond alignment.
pub fn parse_body<T: Copy>(data: &[u8], offset: usize) -> Option<T> {
    let size = std::mem::size_of::<T>();
    if data.len() < offset + size {
        return None;
    }
    Some(unsafe { std::ptr::read_unaligned(data.as_ptr().add(offset) as *const T) })
}

/// Extract a null-terminated name string from bytes starting at offset.
pub fn parse_name(data: &[u8], offset: usize) -> Option<String> {
    let remaining = data.get(offset..)?;
    let end = remaining.iter().position(|&b| b == 0).unwrap_or(remaining.len());
    String::from_utf8(remaining[..end].to_vec()).ok()
}

/// Build a FUSE response: header + body bytes.
pub fn build_response(unique: u64, error: i32, body: &[u8]) -> Vec<u8> {
    let len = (FUSE_OUT_HEADER_SIZE + body.len()) as u32;
    let header = FuseOutHeader {
        len,
        error,
        unique,
    };
    let mut buf = Vec::with_capacity(len as usize);
    buf.extend_from_slice(unsafe {
        std::slice::from_raw_parts(
            &header as *const FuseOutHeader as *const u8,
            FUSE_OUT_HEADER_SIZE,
        )
    });
    buf.extend_from_slice(body);
    buf
}

/// Build an error-only FUSE response (negative errno).
pub fn build_error_response(unique: u64, errno: i32) -> Vec<u8> {
    build_response(unique, -errno, &[])
}

/// Serialize a struct T to bytes.
pub fn to_bytes<T: Copy>(val: &T) -> Vec<u8> {
    let size = std::mem::size_of::<T>();
    let mut buf = vec![0u8; size];
    unsafe {
        std::ptr::copy_nonoverlapping(val as *const T as *const u8, buf.as_mut_ptr(), size);
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuse_in_header_size_is_40() {
        assert_eq!(FUSE_IN_HEADER_SIZE, 40);
    }

    #[test]
    fn fuse_out_header_size_is_16() {
        assert_eq!(FUSE_OUT_HEADER_SIZE, 16);
    }

    #[test]
    fn opcode_roundtrip() {
        assert_eq!(Opcode::from_u32(1), Some(Opcode::Lookup));
        assert_eq!(Opcode::from_u32(26), Some(Opcode::Init));
        assert_eq!(Opcode::from_u32(999), None);
    }

    #[test]
    fn build_error_response_format() {
        let resp = build_error_response(42, libc::ENOENT);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.len, FUSE_OUT_HEADER_SIZE as u32);
        assert_eq!(header.error, -(libc::ENOENT as i32));
        assert_eq!(header.unique, 42);
    }

    #[test]
    fn parse_name_with_nul() {
        let data = b"hello\0world";
        assert_eq!(parse_name(data, 0), Some("hello".to_string()));
    }

    #[test]
    fn parse_name_without_nul() {
        let data = b"hello";
        assert_eq!(parse_name(data, 0), Some("hello".to_string()));
    }
}
```

- [ ] **Step 2: Register module in lib.rs**

Add to `crates/opengoose-sandbox/src/lib.rs`:

```rust
pub mod fuse;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p opengoose-sandbox fuse`
Expected: All FUSE type tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-sandbox/src/fuse/mod.rs crates/opengoose-sandbox/src/lib.rs
git commit -m "feat(sandbox): add FUSE protocol types and codec"
```

---

### Task 3: Inode table

**Files:**
- Create: `crates/opengoose-sandbox/src/fuse/inode_table.rs`

The inode table maps guest inode numbers to host filesystem paths. Inode 1 is always the root (mounted directory). New inodes are allocated on LOOKUP/CREATE/MKDIR.

- [ ] **Step 1: Write inode table tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn root_inode_is_one() {
        let table = InodeTable::new(PathBuf::from("/tmp/test"));
        let entry = table.get(FUSE_ROOT_ID).unwrap();
        assert_eq!(entry.path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn lookup_creates_child_inode() {
        let table = InodeTable::new(PathBuf::from("/tmp/test"));
        let child_ino = table.lookup(FUSE_ROOT_ID, "src");
        assert!(child_ino.is_some());
        let ino = child_ino.unwrap();
        assert_ne!(ino, FUSE_ROOT_ID);
        let entry = table.get(ino).unwrap();
        assert_eq!(entry.path, PathBuf::from("/tmp/test/src"));
    }

    #[test]
    fn lookup_same_name_returns_same_inode() {
        let table = InodeTable::new(PathBuf::from("/tmp/test"));
        let ino1 = table.lookup(FUSE_ROOT_ID, "file.txt").unwrap();
        let ino2 = table.lookup(FUSE_ROOT_ID, "file.txt").unwrap();
        assert_eq!(ino1, ino2);
    }

    #[test]
    fn lookup_nonexistent_parent_returns_none() {
        let table = InodeTable::new(PathBuf::from("/tmp/test"));
        assert!(table.lookup(999, "anything").is_none());
    }

    #[test]
    fn forget_decrements_refcount() {
        let table = InodeTable::new(PathBuf::from("/tmp/test"));
        let ino = table.lookup(FUSE_ROOT_ID, "file.txt").unwrap();
        assert!(table.get(ino).is_some());
        table.forget(ino, 1);
        // After forget, entry may be removed (refcount=0)
        // but root should survive
        assert!(table.get(FUSE_ROOT_ID).is_some());
    }
}
```

- [ ] **Step 2: Implement inode table**

Create `crates/opengoose-sandbox/src/fuse/inode_table.rs`:

```rust
//! Inode ↔ host path bidirectional mapping.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const FUSE_ROOT_ID: u64 = 1;

#[derive(Debug, Clone)]
pub struct InodeEntry {
    pub path: PathBuf,
    pub refcount: u64,
}

pub struct InodeTable {
    inner: Mutex<InodeTableInner>,
}

struct InodeTableInner {
    next_ino: u64,
    entries: HashMap<u64, InodeEntry>,
    path_to_ino: HashMap<PathBuf, u64>,
}

impl InodeTable {
    pub fn new(root_path: PathBuf) -> Self {
        let mut entries = HashMap::new();
        let mut path_to_ino = HashMap::new();
        entries.insert(
            FUSE_ROOT_ID,
            InodeEntry {
                path: root_path.clone(),
                refcount: u64::MAX, // root never expires
            },
        );
        path_to_ino.insert(root_path, FUSE_ROOT_ID);
        InodeTable {
            inner: Mutex::new(InodeTableInner {
                next_ino: 2,
                entries,
                path_to_ino,
            }),
        }
    }

    pub fn get(&self, ino: u64) -> Option<InodeEntry> {
        self.inner.lock().ok()?.entries.get(&ino).cloned()
    }

    /// Look up a child name under a parent inode. Returns the child's inode.
    /// Creates a new inode if not seen before.
    pub fn lookup(&self, parent: u64, name: &str) -> Option<u64> {
        let mut inner = self.inner.lock().ok()?;
        let parent_path = inner.entries.get(&parent)?.path.clone();
        let child_path = parent_path.join(name);

        if let Some(&ino) = inner.path_to_ino.get(&child_path) {
            // Increment refcount
            if let Some(entry) = inner.entries.get_mut(&ino) {
                entry.refcount = entry.refcount.saturating_add(1);
            }
            return Some(ino);
        }

        let ino = inner.next_ino;
        inner.next_ino += 1;
        inner.entries.insert(
            ino,
            InodeEntry {
                path: child_path.clone(),
                refcount: 1,
            },
        );
        inner.path_to_ino.insert(child_path, ino);
        Some(ino)
    }

    /// Create a new inode for a path (used by CREATE/MKDIR).
    pub fn insert(&self, parent: u64, name: &str) -> Option<u64> {
        self.lookup(parent, name)
    }

    /// Decrement refcount. Remove entry if refcount reaches 0 (not root).
    pub fn forget(&self, ino: u64, nlookup: u64) {
        if ino == FUSE_ROOT_ID {
            return;
        }
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(entry) = inner.entries.get_mut(&ino) {
                entry.refcount = entry.refcount.saturating_sub(nlookup);
                if entry.refcount == 0 {
                    let path = entry.path.clone();
                    inner.entries.remove(&ino);
                    inner.path_to_ino.remove(&path);
                }
            }
        }
    }

    /// Get host path for an inode.
    pub fn path(&self, ino: u64) -> Option<PathBuf> {
        self.get(ino).map(|e| e.path)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p opengoose-sandbox inode_table`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-sandbox/src/fuse/inode_table.rs
git commit -m "feat(sandbox): add FUSE inode table"
```

---

### Task 4: FUSE operation handlers

**Files:**
- Create: `crates/opengoose-sandbox/src/fuse/ops.rs`

Each FUSE operation reads from the host filesystem and builds a response. Read-only ops use the host path directly. Write ops (CREATE, WRITE, MKDIR, UNLINK, RMDIR, RENAME) return EROFS — the overlay layer in the guest handles writes.

- [ ] **Step 1: Write tests for FUSE ops**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuse::inode_table::{InodeTable, FUSE_ROOT_ID};
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, InodeTable, HandleTable) {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/nested.txt"), "nested").unwrap();
        let table = InodeTable::new(dir.path().to_path_buf());
        let handles = HandleTable::new();
        (dir, table, handles)
    }

    #[test]
    fn handle_init_returns_version() {
        let resp = handle_init(42, FUSE_KERNEL_VERSION, FUSE_KERNEL_MINOR_VERSION);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
        assert_eq!(header.unique, 42);
    }

    #[test]
    fn handle_lookup_existing_file() {
        let (_dir, inodes, _handles) = setup();
        let resp = handle_lookup(42, FUSE_ROOT_ID, "hello.txt", &inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
    }

    #[test]
    fn handle_lookup_missing_file() {
        let (_dir, inodes, _handles) = setup();
        let resp = handle_lookup(42, FUSE_ROOT_ID, "missing.txt", &inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, -(libc::ENOENT as i32));
    }

    #[test]
    fn handle_getattr_root() {
        let (_dir, inodes, _handles) = setup();
        let resp = handle_getattr(42, FUSE_ROOT_ID, &inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
    }

    #[test]
    fn handle_read_file_contents() {
        let (_dir, inodes, handles) = setup();
        let ino = inodes.lookup(FUSE_ROOT_ID, "hello.txt").unwrap();
        let fh = handles.open(ino, &inodes).unwrap();
        let resp = handle_read(42, fh, 0, 1024, &handles, &inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
        let body = &resp[FUSE_OUT_HEADER_SIZE..];
        assert_eq!(body, b"hello world");
    }

    #[test]
    fn handle_readdir_lists_entries() {
        let (_dir, inodes, handles) = setup();
        let fh = handles.open(FUSE_ROOT_ID, &inodes).unwrap();
        let resp = handle_readdir(42, fh, 0, 4096, &handles, &inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
        assert!(resp.len() > FUSE_OUT_HEADER_SIZE); // has directory entries
    }

    #[test]
    fn handle_statfs_returns_data() {
        let (_dir, inodes, _handles) = setup();
        let resp = handle_statfs(42, &inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
    }

    #[test]
    fn write_ops_return_erofs() {
        let (_dir, inodes, _handles) = setup();
        let ino = inodes.lookup(FUSE_ROOT_ID, "hello.txt").unwrap();
        let resp = handle_create(42, FUSE_ROOT_ID, "new.txt", 0, 0o644, &inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, -(libc::EROFS as i32));
    }
}
```

- [ ] **Step 2: Implement HandleTable and FUSE ops**

Create `crates/opengoose-sandbox/src/fuse/ops.rs`:

```rust
//! FUSE operation handlers.
//! Read ops access the host filesystem via InodeTable.
//! Write ops return EROFS (guest overlay handles writes).

use super::*;
use super::inode_table::{InodeTable, FUSE_ROOT_ID};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Mutex;

/// File handle table — maps fh → inode for open files/dirs.
pub struct HandleTable {
    inner: Mutex<HandleTableInner>,
}

struct HandleTableInner {
    next_fh: u64,
    handles: HashMap<u64, u64>, // fh → inode
}

impl HandleTable {
    pub fn new() -> Self {
        HandleTable {
            inner: Mutex::new(HandleTableInner {
                next_fh: 1,
                handles: HashMap::new(),
            }),
        }
    }

    pub fn open(&self, ino: u64, _inodes: &InodeTable) -> Option<u64> {
        let mut inner = self.inner.lock().ok()?;
        let fh = inner.next_fh;
        inner.next_fh += 1;
        inner.handles.insert(fh, ino);
        Some(fh)
    }

    pub fn close(&self, fh: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.handles.remove(&fh);
        }
    }

    pub fn get_ino(&self, fh: u64) -> Option<u64> {
        self.inner.lock().ok()?.handles.get(&fh).copied()
    }
}

fn metadata_to_attr(ino: u64, meta: &fs::Metadata) -> FuseAttr {
    FuseAttr {
        ino,
        size: meta.len(),
        blocks: meta.blocks(),
        atime: meta.atime() as u64,
        mtime: meta.mtime() as u64,
        ctime: meta.ctime() as u64,
        atimensec: meta.atime_nsec() as u32,
        mtimensec: meta.mtime_nsec() as u32,
        ctimensec: meta.ctime_nsec() as u32,
        mode: meta.mode(),
        nlink: meta.nlink() as u32,
        uid: meta.uid(),
        gid: meta.gid(),
        rdev: meta.rdev() as u32,
        blksize: meta.blksize() as u32,
        flags: 0,
    }
}

pub fn handle_init(unique: u64, _major: u32, _minor: u32) -> Vec<u8> {
    let body = FuseInitOut {
        major: FUSE_KERNEL_VERSION,
        minor: FUSE_KERNEL_MINOR_VERSION,
        max_readahead: 128 * 1024,
        flags: 0,
        max_background: 16,
        congestion_threshold: 12,
        max_write: 128 * 1024,
        time_gran: 1,
        max_pages: 32,
        map_alignment: 0,
        unused: [0; 8],
    };
    build_response(unique, 0, &to_bytes(&body))
}

pub fn handle_lookup(unique: u64, parent: u64, name: &str, inodes: &InodeTable) -> Vec<u8> {
    let Some(ino) = inodes.lookup(parent, name) else {
        return build_error_response(unique, libc::ENOENT);
    };
    let Some(path) = inodes.path(ino) else {
        return build_error_response(unique, libc::ENOENT);
    };
    let Ok(meta) = fs::symlink_metadata(&path) else {
        inodes.forget(ino, 1);
        return build_error_response(unique, libc::ENOENT);
    };
    let entry = FuseEntryOut {
        nodeid: ino,
        generation: 0,
        entry_valid: 1,
        attr_valid: 1,
        entry_valid_nsec: 0,
        attr_valid_nsec: 0,
        attr: metadata_to_attr(ino, &meta),
    };
    build_response(unique, 0, &to_bytes(&entry))
}

pub fn handle_getattr(unique: u64, nodeid: u64, inodes: &InodeTable) -> Vec<u8> {
    let Some(path) = inodes.path(nodeid) else {
        return build_error_response(unique, libc::ENOENT);
    };
    let Ok(meta) = fs::symlink_metadata(&path) else {
        return build_error_response(unique, libc::ENOENT);
    };
    let out = FuseAttrOut {
        attr_valid: 1,
        attr_valid_nsec: 0,
        dummy: 0,
        attr: metadata_to_attr(nodeid, &meta),
    };
    build_response(unique, 0, &to_bytes(&out))
}

pub fn handle_open(unique: u64, nodeid: u64, handles: &HandleTable, inodes: &InodeTable) -> Vec<u8> {
    let Some(fh) = handles.open(nodeid, inodes) else {
        return build_error_response(unique, libc::EIO);
    };
    let out = FuseOpenOut {
        fh,
        open_flags: 0,
        padding: 0,
    };
    build_response(unique, 0, &to_bytes(&out))
}

pub fn handle_read(
    unique: u64,
    fh: u64,
    offset: u64,
    size: u32,
    handles: &HandleTable,
    inodes: &InodeTable,
) -> Vec<u8> {
    let Some(ino) = handles.get_ino(fh) else {
        return build_error_response(unique, libc::EBADF);
    };
    let Some(path) = inodes.path(ino) else {
        return build_error_response(unique, libc::ENOENT);
    };
    let Ok(data) = fs::read(&path) else {
        return build_error_response(unique, libc::EIO);
    };
    let start = offset as usize;
    let end = (start + size as usize).min(data.len());
    if start >= data.len() {
        return build_response(unique, 0, &[]);
    }
    build_response(unique, 0, &data[start..end])
}

pub fn handle_release(unique: u64, fh: u64, handles: &HandleTable) -> Vec<u8> {
    handles.close(fh);
    build_response(unique, 0, &[])
}

pub fn handle_opendir(unique: u64, nodeid: u64, handles: &HandleTable, inodes: &InodeTable) -> Vec<u8> {
    handle_open(unique, nodeid, handles, inodes)
}

pub fn handle_readdir(
    unique: u64,
    fh: u64,
    offset: u64,
    size: u32,
    handles: &HandleTable,
    inodes: &InodeTable,
) -> Vec<u8> {
    let Some(ino) = handles.get_ino(fh) else {
        return build_error_response(unique, libc::EBADF);
    };
    let Some(path) = inodes.path(ino) else {
        return build_error_response(unique, libc::ENOENT);
    };
    let Ok(entries) = fs::read_dir(&path) else {
        return build_error_response(unique, libc::EIO);
    };

    let mut buf = Vec::new();
    let mut idx: u64 = 0;
    for entry in entries.flatten() {
        idx += 1;
        if idx <= offset {
            continue;
        }

        let name = entry.file_name();
        let name_bytes = name.as_encoded_bytes();
        let namelen = name_bytes.len() as u32;
        let entry_size = std::mem::size_of::<FuseDirent>() + name_bytes.len();
        let padded_size = (entry_size + 7) & !7; // 8-byte align

        if buf.len() + padded_size > size as usize {
            break;
        }

        let file_type = entry
            .file_type()
            .map(|ft| {
                if ft.is_dir() {
                    4u32 // DT_DIR
                } else if ft.is_symlink() {
                    10u32 // DT_LNK
                } else {
                    8u32 // DT_REG
                }
            })
            .unwrap_or(0);

        let child_ino = inodes.lookup(ino, &name.to_string_lossy()).unwrap_or(0);

        let dirent = FuseDirent {
            ino: child_ino,
            off: idx,
            namelen,
            typ: file_type,
        };
        buf.extend_from_slice(&to_bytes(&dirent));
        buf.extend_from_slice(name_bytes);
        while buf.len() % 8 != 0 {
            buf.push(0);
        }
    }

    build_response(unique, 0, &buf)
}

pub fn handle_releasedir(unique: u64, fh: u64, handles: &HandleTable) -> Vec<u8> {
    handle_release(unique, fh, handles)
}

pub fn handle_statfs(unique: u64, inodes: &InodeTable) -> Vec<u8> {
    let Some(path) = inodes.path(FUSE_ROOT_ID) else {
        return build_error_response(unique, libc::ENOENT);
    };
    let c_path = match std::ffi::CString::new(path.to_string_lossy().as_bytes()) {
        Ok(p) => p,
        Err(_) => return build_error_response(unique, libc::EINVAL),
    };
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statfs(c_path.as_ptr(), &mut stat) };
    if ret != 0 {
        return build_error_response(unique, libc::EIO);
    }
    let out = FuseStatfsOut {
        st: FuseKstatfs {
            blocks: stat.f_blocks,
            bfree: stat.f_bfree,
            bavail: stat.f_bavail,
            files: stat.f_files,
            ffree: stat.f_ffree,
            bsize: stat.f_bsize as u32,
            namelen: 255,
            frsize: stat.f_bsize as u32,
            padding: 0,
            spare: [0; 6],
        },
    };
    build_response(unique, 0, &to_bytes(&out))
}

// Write ops — return EROFS (overlay handles writes in guest)
pub fn handle_create(unique: u64, _parent: u64, _name: &str, _flags: u32, _mode: u32, _inodes: &InodeTable) -> Vec<u8> {
    build_error_response(unique, libc::EROFS)
}

pub fn handle_write(unique: u64) -> Vec<u8> {
    build_error_response(unique, libc::EROFS)
}

pub fn handle_mkdir(unique: u64) -> Vec<u8> {
    build_error_response(unique, libc::EROFS)
}

pub fn handle_unlink(unique: u64) -> Vec<u8> {
    build_error_response(unique, libc::EROFS)
}

pub fn handle_rmdir(unique: u64) -> Vec<u8> {
    build_error_response(unique, libc::EROFS)
}

pub fn handle_rename(unique: u64) -> Vec<u8> {
    build_error_response(unique, libc::EROFS)
}

pub fn handle_flush(unique: u64) -> Vec<u8> {
    build_response(unique, 0, &[])
}

pub fn handle_fsync(unique: u64) -> Vec<u8> {
    build_response(unique, 0, &[])
}

pub fn handle_destroy(unique: u64) -> Vec<u8> {
    build_response(unique, 0, &[])
}

pub fn handle_forget() {
    // FORGET has no response
}
```

- [ ] **Step 3: Fix tests to use correct API signatures, run**

Update the test for `handle_read` to use `handle_read_with_inodes`:

Run: `cargo test -p opengoose-sandbox fuse::ops`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-sandbox/src/fuse/ops.rs
git commit -m "feat(sandbox): add FUSE operation handlers (read-only + EROFS writes)"
```

---

### Task 5: VirtioFs device

**Files:**
- Create: `crates/opengoose-sandbox/src/virtio_fs.rs`
- Modify: `crates/opengoose-sandbox/src/lib.rs`

The VirtioFs device implements the virtio-mmio v2 register interface (same pattern as VirtioConsole) with device ID 26 (virtio-fs). It manages 2 virtqueues: hiprio (queue 0) and request (queue 1). The request queue carries FUSE messages.

- [ ] **Step 1: Implement VirtioFs device**

Create `crates/opengoose-sandbox/src/virtio_fs.rs`. Follow the exact structure of `virtio.rs` (VirtioConsole) but with:
- `DEVICE_ID = 26` (virtio-fs)
- 2 queues: hiprio (0) and request (1)
- Config space: tag string (e.g., "virtiofs") + num_request_queues=1
- Feature bit: `VIRTIO_FS_F_NOTIFICATION = 0` (none needed)
- `process_request_queue()`: read FUSE request from virtqueue, dispatch to `fuse::ops`, write response back

Key struct:

```rust
pub struct VirtioFs {
    status: u32,
    queue_sel: u32,
    device_features_sel: u32,
    interrupt_status: u32,
    queues: [VirtQueue; 2],       // 0=hiprio, 1=request
    tag: [u8; 36],                // filesystem tag, null-padded
    inodes: InodeTable,
    handles: HandleTable,
}
```

The `process_request_queue` method:
1. Read avail ring for new descriptors
2. Gather descriptor chain into a contiguous buffer (FUSE request)
3. Parse `FuseInHeader`
4. Dispatch to appropriate `fuse::ops::handle_*` function
5. Write response into the writable descriptor(s)
6. Update used ring

This follows the same descriptor chain walking pattern as `VirtioConsole::process_tx`.

- [ ] **Step 2: Register module in lib.rs**

Add to `crates/opengoose-sandbox/src/lib.rs`:

```rust
pub mod virtio_fs;
```

- [ ] **Step 3: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtio_fs_mmio_read_magic() {
        let fs = VirtioFs::new(std::path::PathBuf::from("/tmp"));
        assert_eq!(fs.handle_mmio_read(0x000), 0x7472_6976); // "virt"
    }

    #[test]
    fn virtio_fs_device_id_is_26() {
        let fs = VirtioFs::new(std::path::PathBuf::from("/tmp"));
        assert_eq!(fs.handle_mmio_read(0x008), 26);
    }

    #[test]
    fn virtio_fs_config_tag_readable() {
        let fs = VirtioFs::new(std::path::PathBuf::from("/tmp"));
        // Config space starts at 0x100, tag is first 36 bytes
        let first_byte = fs.handle_mmio_read(0x100);
        // "virtiofs" tag
        assert_eq!(first_byte & 0xFF, b'v' as u64);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p opengoose-sandbox virtio_fs`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-sandbox/src/virtio_fs.rs crates/opengoose-sandbox/src/lib.rs
git commit -m "feat(sandbox): add VirtioFs device with FUSE dispatch"
```

---

### Task 6: Boot + MicroVm MMIO routing

**Files:**
- Modify: `crates/opengoose-sandbox/src/boot.rs`
- Modify: `crates/opengoose-sandbox/src/vm.rs`
- Modify: `crates/opengoose-sandbox/src/snapshot.rs`

The boot sequence and forked VMs must route MMIO accesses to the new virtio-fs device.

- [ ] **Step 1: Add VirtioFs field to BootedVm**

In `boot.rs`, add `virtio_fs: Option<VirtioFs>` to `BootedVm`. Initially `None` during boot (virtio-fs is only active in forked VMs). Update `boot()` to initialize it as `None`.

- [ ] **Step 2: Add MMIO routing in BootedVm::step_once**

In `boot.rs`, add MMIO routing for the virtio-fs address range:

```rust
// In step_once MmioWrite:
} else if (machine::VIRTIO_FS_MMIO_BASE
    ..machine::VIRTIO_FS_MMIO_BASE + machine::VIRTIO_FS_MMIO_SIZE)
    .contains(&addr)
{
    if let Some(ref mut vfs) = self.virtio_fs {
        let offset = addr - machine::VIRTIO_FS_MMIO_BASE;
        vfs.handle_mmio_write(offset, data);
        if offset == 0x050 {
            vfs.process_notify(data as u32, self.mem_ptr, self.mem_size);
        }
    }
}
```

Same pattern for MmioRead.

- [ ] **Step 3: Add VirtioFs field to MicroVm**

In `vm.rs`, add `virtio_fs: Option<VirtioFs>` to `MicroVm`. Update `fork_from`, `reset`, `step_once` with MMIO routing, same as BootedVm.

- [ ] **Step 4: Add mount_virtio_fs method to MicroVm**

```rust
impl MicroVm {
    /// Configure virtio-fs to serve the given host directory.
    /// Must be called after fork_from / reset, before exec.
    pub fn mount_virtio_fs(&mut self, host_dir: &Path) {
        self.virtio_fs = Some(VirtioFs::new(host_dir.to_path_buf()));
    }
}
```

- [ ] **Step 5: Update snapshot to include virtio-fs tag info**

In `snapshot.rs`, add optional virtio-fs metadata. This is needed so that `reset()` knows whether to re-initialize virtio-fs.

- [ ] **Step 6: Run full test suite**

Run: `cargo test -p opengoose-sandbox`
Expected: All existing tests pass (virtio-fs is None by default, no behavior change)

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose-sandbox/src/boot.rs crates/opengoose-sandbox/src/vm.rs crates/opengoose-sandbox/src/snapshot.rs
git commit -m "feat(sandbox): route MMIO to VirtioFs device in boot and fork VMs"
```

---

### Task 7: Guest init — mount virtiofs + overlay

**Files:**
- Modify: `crates/opengoose-sandbox/guest/init/src/main.rs`

The guest init must mount the virtio-fs filesystem and set up an overlay so that writes go to tmpfs.

- [ ] **Step 1: Add virtiofs mount to guest init**

Add after the `mount_or_ignore` calls and before `SNAPSHOT`:

```rust
// Mount virtiofs if available
mount_virtiofs();

// Set up overlay if virtiofs mounted successfully
setup_overlay();
```

Implement the functions:

```rust
fn mount_virtiofs() {
    // Create mount point
    let _ = std::fs::create_dir_all("/mnt/host");

    unsafe {
        let source = std::ffi::CString::new("virtiofs").unwrap();
        let target = std::ffi::CString::new("/mnt/host").unwrap();
        let fstype = std::ffi::CString::new("virtiofs").unwrap();
        let opts = std::ffi::CString::new("tag=virtiofs").unwrap();
        let ret = libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            libc::MS_RDONLY,
            opts.as_ptr() as *const libc::c_void,
        );
        if ret == 0 {
            uart_write(b"VIRTIOFS:mounted\n");
        } else {
            uart_write(b"VIRTIOFS:failed\n");
        }
    }
}

fn setup_overlay() {
    // Check if virtiofs is mounted
    if !std::path::Path::new("/mnt/host").exists() {
        return;
    }

    let _ = std::fs::create_dir_all("/workspace");
    let _ = std::fs::create_dir_all("/tmp/upper");
    let _ = std::fs::create_dir_all("/tmp/work");

    unsafe {
        let source = std::ffi::CString::new("overlay").unwrap();
        let target = std::ffi::CString::new("/workspace").unwrap();
        let fstype = std::ffi::CString::new("overlay").unwrap();
        let opts = std::ffi::CString::new(
            "lowerdir=/mnt/host,upperdir=/tmp/upper,workdir=/tmp/work"
        ).unwrap();
        let ret = libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            0,
            opts.as_ptr() as *const libc::c_void,
        );
        if ret == 0 {
            uart_write(b"OVERLAY:mounted\n");
        } else {
            uart_write(b"OVERLAY:failed\n");
        }
    }
}
```

- [ ] **Step 2: Rebuild guest init**

```bash
cd crates/opengoose-sandbox/guest/init
cargo build --release --target aarch64-unknown-linux-musl
```

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose-sandbox/guest/init/src/main.rs
git commit -m "feat(sandbox): guest init mounts virtiofs + overlay"
```

---

### Task 8: Integration test

**Files:**
- Create: `crates/opengoose-sandbox/tests/virtio_fs_test.rs`

This test verifies the full pipeline: fork VM → mount virtio-fs → read host files → write to overlay → verify host unchanged.

- [ ] **Step 1: Write integration test**

```rust
#[cfg(target_os = "macos")]
use opengoose_sandbox::SandboxPool;
#[cfg(target_os = "macos")]
use std::time::Duration;
#[cfg(target_os = "macos")]
use tempfile::tempdir;

/// Test: fork VM, mount virtiofs, read host file content via exec.
#[test]
#[cfg_attr(target_os = "macos", serial_test::serial)]
#[cfg(target_os = "macos")]
fn test_virtiofs_read_host_file() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "hello from host").unwrap();

    let pool = SandboxPool::new();
    let mut vm = pool.acquire().expect("acquire should succeed");
    vm.mount_virtio_fs(dir.path());

    // Wait for virtiofs mount
    let output = vm.collect_uart_output_raw(Duration::from_secs(5));
    let output_str = String::from_utf8_lossy(&output);
    assert!(
        output_str.contains("VIRTIOFS:mounted"),
        "virtiofs should mount, got: {output_str}"
    );

    // Read the file through the overlay
    let result = vm
        .exec("cat", &["/workspace/test.txt"], Duration::from_secs(5))
        .expect("exec should succeed");
    assert_eq!(result.status, 0);
    assert_eq!(result.stdout.trim(), "hello from host");
}

/// Test: writes go to overlay, host file unchanged.
#[test]
#[cfg_attr(target_os = "macos", serial_test::serial)]
#[cfg(target_os = "macos")]
fn test_virtiofs_overlay_isolation() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("original.txt"), "original content").unwrap();

    let pool = SandboxPool::new();
    let mut vm = pool.acquire().expect("acquire should succeed");
    vm.mount_virtio_fs(dir.path());

    let _ = vm.collect_uart_output_raw(Duration::from_secs(5));

    // Modify file in overlay
    let result = vm
        .exec("sh", &["-c", "echo modified > /workspace/original.txt"], Duration::from_secs(5))
        .expect("exec should succeed");
    assert_eq!(result.status, 0);

    // Verify host file is unchanged
    let host_content = std::fs::read_to_string(dir.path().join("original.txt")).unwrap();
    assert_eq!(host_content, "original content");
}
```

- [ ] **Step 2: Run integration test**

Run: `cargo test -p opengoose-sandbox test_virtiofs -- --ignored`
(May need `--ignored` if marked as such, or run directly)

Expected: Both tests pass. If FUSE ops are missing, debug by checking UART output for which op failed.

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose-sandbox/tests/virtio_fs_test.rs
git commit -m "test(sandbox): add virtio-fs integration tests (mount + read + overlay isolation)"
```

---

## Implementation Notes

### Virtqueue descriptor chain processing

The virtio-fs request queue uses chained descriptors. A typical FUSE request chain:
1. Descriptor 0: readable — `FuseInHeader` + operation-specific data
2. Descriptor 1: writable — buffer for `FuseOutHeader` + response data

The VMM must:
1. Read all readable descriptors into a contiguous buffer
2. Parse `FuseInHeader` to get opcode
3. Dispatch to handler
4. Write response into the writable descriptor(s)
5. Update used ring with total bytes written

This is the same virtqueue pattern as `VirtioConsole::process_tx` but bidirectional.

### Alpine kernel virtio-fs support

Alpine linux-virt 6.12 includes `CONFIG_VIRTIO_FS=m` (module). The guest init must load `virtiofs.ko` if it's built as a module, or the kernel may have it built-in. Check during integration testing. If module loading is needed, follow the same pattern as `virtio_mmio.ko` loading in `initramfs.rs`.

### SPI interrupt for virtio-fs

The virtio-fs device uses SPI 3 (intid 35 = 32 + 3). During boot, the GIC handles delivery via `vm.set_spi()`. In forked VMs (software GIC), add virtio-fs to the `ICC_IAR1_EL1` emulation in `vm.rs::handle_sysreg`:

```rust
35u64 // SPI 3 = virtio-fs (intid 32+3)
```

### What "~15 ops" means in practice

If `cargo build` hits an unimplemented op, the guest kernel gets ENOSYS and prints a warning. Use UART output during integration testing to identify missing ops. The most likely additions beyond the initial set: `SETATTR` (cargo may `chmod`), `ACCESS` (permission checks). Add them as ENOSYS → real impl when needed.
