//! Virtio-mmio v2 filesystem device (device ID 26, virtio-fs).
//! Receives FUSE requests from the guest kernel via virtqueue and dispatches
//! them to `fuse::ops` handlers against the host filesystem.
//!
//! Queue layout: 0=hiprio, 1=request.

use crate::fuse::inode_table::InodeTable;
use crate::fuse::ops::HandleTable;
use crate::fuse::{
    self, build_error_response, parse_body, parse_in_header, parse_name, Opcode, FuseCreateIn,
    FuseReadIn, FuseReleaseIn, FUSE_IN_HEADER_SIZE,
};
use crate::machine;
use std::path::PathBuf;

// Virtio MMIO register offsets (same as VirtioConsole)
const MAGIC_VALUE: u64 = 0x000;
const VERSION: u64 = 0x004;
const DEVICE_ID: u64 = 0x008;
const VENDOR_ID: u64 = 0x00C;
const DEVICE_FEATURES: u64 = 0x010;
const DEVICE_FEATURES_SEL: u64 = 0x014;
const DRIVER_FEATURES: u64 = 0x020;
const DRIVER_FEATURES_SEL: u64 = 0x024;
const QUEUE_SEL: u64 = 0x030;
const QUEUE_NUM_MAX: u64 = 0x034;
const QUEUE_NUM: u64 = 0x038;
const QUEUE_READY: u64 = 0x044;
const QUEUE_NOTIFY: u64 = 0x050;
const INTERRUPT_STATUS: u64 = 0x060;
const INTERRUPT_ACK: u64 = 0x064;
const STATUS: u64 = 0x070;
const QUEUE_DESC_LOW: u64 = 0x080;
const QUEUE_DESC_HIGH: u64 = 0x084;
const QUEUE_DRIVER_LOW: u64 = 0x090;
const QUEUE_DRIVER_HIGH: u64 = 0x094;
const QUEUE_DEVICE_LOW: u64 = 0x0A0;
const QUEUE_DEVICE_HIGH: u64 = 0x0A4;
const CONFIG_GENERATION: u64 = 0x0FC;

const VIRTIO_MAGIC: u32 = 0x7472_6976; // "virt"
const VIRTIO_VERSION: u32 = 2;
const VIRTIO_DEVICE_FS: u32 = 26;
const VIRTIO_VENDOR: u32 = 0x554D_4551; // "QEMU"

const MAX_QUEUE_SIZE: u32 = 256;
const NUM_QUEUES: usize = 2; // 0=hiprio, 1=request

// Vring descriptor flags
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VringDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[derive(Default)]
struct VirtQueue {
    ready: bool,
    num: u32,
    desc_addr: u64,
    driver_addr: u64,
    device_addr: u64,
    last_avail_idx: u16,
}

pub struct VirtioFs {
    status: u32,
    queue_sel: u32,
    device_features_sel: u32,
    interrupt_status: u32,
    queues: [VirtQueue; NUM_QUEUES],
    tag: [u8; 36],
    inodes: InodeTable,
    handles: HandleTable,
}

impl VirtioFs {
    pub fn new(root_path: PathBuf) -> Self {
        let mut tag = [0u8; 36];
        let tag_str = b"virtiofs";
        tag[..tag_str.len()].copy_from_slice(tag_str);

        VirtioFs {
            status: 0,
            queue_sel: 0,
            device_features_sel: 0,
            interrupt_status: 0,
            queues: Default::default(),
            tag,
            inodes: InodeTable::new(root_path),
            handles: HandleTable::new(),
        }
    }

    /// Replace the host root directory. Resets inode and handle tables.
    pub fn set_root(&mut self, root_path: PathBuf) {
        self.inodes = InodeTable::new(root_path);
        self.handles = HandleTable::new();
    }

    pub fn irq_pending(&self) -> bool {
        self.interrupt_status != 0
    }

    pub fn handle_mmio_read(&self, offset: u64) -> u64 {
        match offset {
            MAGIC_VALUE => VIRTIO_MAGIC as u64,
            VERSION => VIRTIO_VERSION as u64,
            DEVICE_ID => VIRTIO_DEVICE_FS as u64,
            VENDOR_ID => VIRTIO_VENDOR as u64,
            DEVICE_FEATURES => {
                if self.device_features_sel == 1 {
                    1 // bit 32 = VIRTIO_F_VERSION_1
                } else {
                    0 // no special features at sel=0
                }
            }
            QUEUE_NUM_MAX => MAX_QUEUE_SIZE as u64,
            QUEUE_READY => {
                let idx = self.queue_sel as usize;
                if idx < NUM_QUEUES {
                    self.queues[idx].ready as u64
                } else {
                    0
                }
            }
            INTERRUPT_STATUS => self.interrupt_status as u64,
            STATUS => self.status as u64,
            CONFIG_GENERATION => 0,
            // Config space: 36-byte tag at 0x100..0x124, then u32 num_request_queues at 0x124
            o @ 0x100..=0x123 => {
                let tag_offset = (o - 0x100) as usize;
                if tag_offset < 36 {
                    self.tag[tag_offset] as u64
                } else {
                    0
                }
            }
            0x124 => 1u64, // num_request_queues
            _ => 0,
        }
    }

    pub fn handle_mmio_write(&mut self, offset: u64, val: u64) {
        match offset {
            DEVICE_FEATURES_SEL => self.device_features_sel = val as u32,
            DRIVER_FEATURES_SEL | DRIVER_FEATURES => {}
            QUEUE_SEL => self.queue_sel = val as u32,
            QUEUE_NUM => {
                let idx = self.queue_sel as usize;
                if idx < NUM_QUEUES {
                    self.queues[idx].num = val as u32;
                }
            }
            QUEUE_READY => {
                let idx = self.queue_sel as usize;
                if idx < NUM_QUEUES {
                    self.queues[idx].ready = val != 0;
                }
            }
            QUEUE_DESC_LOW | QUEUE_DESC_HIGH | QUEUE_DRIVER_LOW | QUEUE_DRIVER_HIGH
            | QUEUE_DEVICE_LOW | QUEUE_DEVICE_HIGH => {
                let idx = self.queue_sel as usize;
                if idx < NUM_QUEUES {
                    let q = &mut self.queues[idx];
                    match offset {
                        QUEUE_DESC_LOW => q.desc_addr = (q.desc_addr & !0xFFFF_FFFF) | val,
                        QUEUE_DESC_HIGH => {
                            q.desc_addr = (q.desc_addr & 0xFFFF_FFFF) | (val << 32)
                        }
                        QUEUE_DRIVER_LOW => {
                            q.driver_addr = (q.driver_addr & !0xFFFF_FFFF) | val
                        }
                        QUEUE_DRIVER_HIGH => {
                            q.driver_addr = (q.driver_addr & 0xFFFF_FFFF) | (val << 32)
                        }
                        QUEUE_DEVICE_LOW => {
                            q.device_addr = (q.device_addr & !0xFFFF_FFFF) | val
                        }
                        QUEUE_DEVICE_HIGH => {
                            q.device_addr = (q.device_addr & 0xFFFF_FFFF) | (val << 32)
                        }
                        _ => unreachable!(),
                    }
                }
            }
            QUEUE_NOTIFY => { /* Handled in process_notify */ }
            INTERRUPT_ACK => {
                self.interrupt_status &= !(val as u32);
            }
            STATUS => {
                self.status = val as u32;
                if val == 0 {
                    self.queues = Default::default();
                    self.interrupt_status = 0;
                }
            }
            _ => {}
        }
    }

    pub fn process_notify(&mut self, queue_idx: u32, mem_ptr: *mut u8, mem_size: usize) {
        if queue_idx == 1 {
            self.process_request_queue(mem_ptr, mem_size);
        }
    }

    fn process_request_queue(&mut self, mem_ptr: *mut u8, mem_size: usize) {
        if !self.queues[1].ready || self.queues[1].num == 0 {
            return;
        }

        loop {
            // Snapshot queue fields to avoid holding &mut self.queues[1] across dispatch
            let driver_addr = self.queues[1].driver_addr;
            let device_addr = self.queues[1].device_addr;
            let desc_addr = self.queues[1].desc_addr;
            let num = self.queues[1].num;
            let last_avail = self.queues[1].last_avail_idx;

            let avail_idx = read_u16(mem_ptr, mem_size, driver_addr + 2);
            if last_avail == avail_idx {
                break;
            }

            let ring_idx = (last_avail as u64 % num as u64) * 2 + 4;
            let head_desc_idx = read_u16(mem_ptr, mem_size, driver_addr + ring_idx) as u64;

            // Walk the descriptor chain: collect readable data, find writable descriptor
            let mut readable_buf = Vec::new();
            let mut writable_addr: u64 = 0;
            let mut writable_len: u32 = 0;
            let mut idx = head_desc_idx;

            for _ in 0..num {
                let desc = read_desc(mem_ptr, mem_size, desc_addr, idx);
                if desc.flags & VRING_DESC_F_WRITE == 0 {
                    // Readable descriptor — FUSE request data
                    let data = read_guest_buf(mem_ptr, mem_size, desc.addr, desc.len as usize);
                    readable_buf.extend_from_slice(&data);
                } else {
                    // Writable descriptor — space for FUSE response
                    if writable_len == 0 {
                        writable_addr = desc.addr;
                        writable_len = desc.len;
                    }
                }
                if desc.flags & VRING_DESC_F_NEXT != 0 {
                    idx = desc.next as u64;
                } else {
                    break;
                }
            }

            // Dispatch FUSE request (borrows &self for inodes/handles)
            let response = self.dispatch_fuse(&readable_buf);

            // Write response to writable descriptor(s)
            let bytes_written = if !response.is_empty() && writable_len > 0 {
                let write_len = response.len().min(writable_len as usize);
                write_guest_buf(mem_ptr, mem_size, writable_addr, &response[..write_len]);
                write_len as u32
            } else {
                0
            };

            // Update used ring
            let used_idx = read_u16(mem_ptr, mem_size, device_addr + 2);
            let used_ring_off = (used_idx as u64 % num as u64) * 8 + 4;
            write_u32(
                mem_ptr,
                mem_size,
                device_addr + used_ring_off,
                head_desc_idx as u32,
            );
            write_u32(
                mem_ptr,
                mem_size,
                device_addr + used_ring_off + 4,
                bytes_written,
            );
            write_u16(
                mem_ptr,
                mem_size,
                device_addr + 2,
                used_idx.wrapping_add(1),
            );
            self.queues[1].last_avail_idx = last_avail.wrapping_add(1);
        }

        self.interrupt_status |= 1;
    }

    fn dispatch_fuse(&self, data: &[u8]) -> Vec<u8> {
        let Some(header) = parse_in_header(data) else {
            return build_error_response(0, libc::EIO);
        };
        let unique = header.unique;
        let nodeid = header.nodeid;
        let body_offset = FUSE_IN_HEADER_SIZE;

        match Opcode::from_u32(header.opcode) {
            Some(Opcode::Init) => {
                let init_in: fuse::FuseInitIn =
                    parse_body(data, body_offset).unwrap_or(fuse::FuseInitIn {
                        major: 7,
                        minor: 31,
                        max_readahead: 0,
                        flags: 0,
                    });
                fuse::ops::handle_init(unique, init_in.major, init_in.minor)
            }
            Some(Opcode::Lookup) => {
                let name = parse_name(data, body_offset).unwrap_or_default();
                fuse::ops::handle_lookup(unique, nodeid, &name, &self.inodes)
            }
            Some(Opcode::Getattr) => fuse::ops::handle_getattr(unique, nodeid, &self.inodes),
            Some(Opcode::Open) => {
                fuse::ops::handle_open(unique, nodeid, &self.handles, &self.inodes)
            }
            Some(Opcode::Read) => {
                let read_in: FuseReadIn =
                    parse_body(data, body_offset).unwrap_or(FuseReadIn {
                        fh: 0,
                        offset: 0,
                        size: 0,
                        read_flags: 0,
                        lock_owner: 0,
                        flags: 0,
                        padding: 0,
                    });
                fuse::ops::handle_read(
                    unique,
                    read_in.fh,
                    read_in.offset,
                    read_in.size,
                    &self.handles,
                    &self.inodes,
                )
            }
            Some(Opcode::Write) => fuse::ops::handle_write(unique),
            Some(Opcode::Statfs) => fuse::ops::handle_statfs(unique, &self.inodes),
            Some(Opcode::Release) => {
                let release_in: FuseReleaseIn =
                    parse_body(data, body_offset).unwrap_or(FuseReleaseIn {
                        fh: 0,
                        flags: 0,
                        release_flags: 0,
                        lock_owner: 0,
                    });
                fuse::ops::handle_release(unique, release_in.fh, &self.handles)
            }
            Some(Opcode::Flush) => fuse::ops::handle_flush(unique),
            Some(Opcode::Fsync) => fuse::ops::handle_fsync(unique),
            Some(Opcode::Opendir) => {
                fuse::ops::handle_opendir(unique, nodeid, &self.handles, &self.inodes)
            }
            Some(Opcode::Readdir) | Some(Opcode::Readdirplus) => {
                let read_in: FuseReadIn =
                    parse_body(data, body_offset).unwrap_or(FuseReadIn {
                        fh: 0,
                        offset: 0,
                        size: 0,
                        read_flags: 0,
                        lock_owner: 0,
                        flags: 0,
                        padding: 0,
                    });
                fuse::ops::handle_readdir(
                    unique,
                    read_in.fh,
                    read_in.offset,
                    read_in.size,
                    &self.handles,
                    &self.inodes,
                )
            }
            Some(Opcode::Releasedir) => {
                let release_in: FuseReleaseIn =
                    parse_body(data, body_offset).unwrap_or(FuseReleaseIn {
                        fh: 0,
                        flags: 0,
                        release_flags: 0,
                        lock_owner: 0,
                    });
                fuse::ops::handle_releasedir(unique, release_in.fh, &self.handles)
            }
            Some(Opcode::Create) => {
                let create_in: FuseCreateIn =
                    parse_body(data, body_offset).unwrap_or(FuseCreateIn {
                        flags: 0,
                        mode: 0,
                        umask: 0,
                        open_flags: 0,
                    });
                let name_offset = body_offset + std::mem::size_of::<FuseCreateIn>();
                let name = parse_name(data, name_offset).unwrap_or_default();
                fuse::ops::handle_create(
                    unique,
                    nodeid,
                    &name,
                    create_in.flags,
                    create_in.mode,
                    &self.inodes,
                )
            }
            Some(Opcode::Mkdir) => fuse::ops::handle_mkdir(unique),
            Some(Opcode::Unlink) => fuse::ops::handle_unlink(unique),
            Some(Opcode::Rmdir) => fuse::ops::handle_rmdir(unique),
            Some(Opcode::Rename) => fuse::ops::handle_rename(unique),
            Some(Opcode::Destroy) => fuse::ops::handle_destroy(unique),
            Some(Opcode::Forget) => {
                fuse::ops::handle_forget();
                Vec::new()
            }
            None => build_error_response(unique, libc::ENOSYS),
        }
    }
}

// --- Virtqueue helper functions (copied from virtio.rs) ---

fn gpa_to_offset(gpa: u64, mem_size: usize) -> Option<usize> {
    if gpa < machine::RAM_BASE {
        return None;
    }
    let offset = (gpa - machine::RAM_BASE) as usize;
    if offset >= mem_size {
        None
    } else {
        Some(offset)
    }
}

fn read_desc(mem_ptr: *mut u8, mem_size: usize, desc_base: u64, idx: u64) -> VringDesc {
    let Some(addr) = desc_base.checked_add(idx.saturating_mul(16)) else {
        return VringDesc::default();
    };
    let Some(offset) = gpa_to_offset(addr, mem_size) else {
        return VringDesc::default();
    };
    if offset + 16 > mem_size {
        return VringDesc::default();
    }
    unsafe {
        let ptr = mem_ptr.add(offset);
        VringDesc {
            addr: (ptr as *const u64).read_unaligned(),
            len: (ptr.add(8) as *const u32).read_unaligned(),
            flags: (ptr.add(12) as *const u16).read_unaligned(),
            next: (ptr.add(14) as *const u16).read_unaligned(),
        }
    }
}

fn read_guest_buf(mem_ptr: *mut u8, mem_size: usize, gpa: u64, len: usize) -> Vec<u8> {
    let Some(offset) = gpa_to_offset(gpa, mem_size) else {
        return Vec::new();
    };
    if offset + len > mem_size {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(mem_ptr.add(offset), len).to_vec() }
}

fn write_guest_buf(mem_ptr: *mut u8, mem_size: usize, gpa: u64, data: &[u8]) {
    let Some(offset) = gpa_to_offset(gpa, mem_size) else {
        return;
    };
    if offset + data.len() > mem_size {
        return;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), mem_ptr.add(offset), data.len());
    }
}

fn read_u16(mem_ptr: *mut u8, mem_size: usize, gpa: u64) -> u16 {
    let Some(offset) = gpa_to_offset(gpa, mem_size) else {
        return 0;
    };
    if offset + 2 > mem_size {
        return 0;
    }
    unsafe { (mem_ptr.add(offset) as *const u16).read_unaligned() }
}

fn write_u16(mem_ptr: *mut u8, mem_size: usize, gpa: u64, val: u16) {
    let Some(offset) = gpa_to_offset(gpa, mem_size) else {
        return;
    };
    if offset + 2 > mem_size {
        return;
    }
    unsafe {
        (mem_ptr.add(offset) as *mut u16).write_unaligned(val);
    }
}

fn write_u32(mem_ptr: *mut u8, mem_size: usize, gpa: u64, val: u32) {
    let Some(offset) = gpa_to_offset(gpa, mem_size) else {
        return;
    };
    if offset + 4 > mem_size {
        return;
    }
    unsafe {
        (mem_ptr.add(offset) as *mut u32).write_unaligned(val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtio_fs_mmio_read_magic() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        assert_eq!(fs.handle_mmio_read(0x000), 0x7472_6976); // "virt"
    }

    #[test]
    fn virtio_fs_device_id_is_26() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        assert_eq!(fs.handle_mmio_read(0x008), 26);
    }

    #[test]
    fn virtio_fs_config_tag_readable() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        let byte0 = fs.handle_mmio_read(0x100) & 0xFF;
        assert_eq!(byte0, b'v' as u64); // "virtiofs"
    }

    #[test]
    fn virtio_fs_queue_num_max() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        assert_eq!(fs.handle_mmio_read(0x034), 256);
    }

    #[test]
    fn virtio_fs_version_is_2() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        assert_eq!(fs.handle_mmio_read(0x004), 2);
    }

    #[test]
    fn virtio_fs_vendor_id() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        assert_eq!(fs.handle_mmio_read(0x00C), 0x554D_4551);
    }

    #[test]
    fn virtio_fs_features_sel0_is_zero() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        // device_features_sel defaults to 0
        assert_eq!(fs.handle_mmio_read(0x010), 0);
    }

    #[test]
    fn virtio_fs_features_sel1_has_version1() {
        let mut fs = VirtioFs::new(PathBuf::from("/tmp"));
        fs.handle_mmio_write(DEVICE_FEATURES_SEL, 1);
        assert_eq!(fs.handle_mmio_read(0x010), 1); // VIRTIO_F_VERSION_1
    }

    #[test]
    fn virtio_fs_config_num_request_queues() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        assert_eq!(fs.handle_mmio_read(0x124), 1);
    }

    #[test]
    fn virtio_fs_irq_pending_default_false() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        assert!(!fs.irq_pending());
    }

    #[test]
    fn virtio_fs_status_write_read() {
        let mut fs = VirtioFs::new(PathBuf::from("/tmp"));
        fs.handle_mmio_write(STATUS, 0x0F);
        assert_eq!(fs.handle_mmio_read(STATUS), 0x0F);
    }

    #[test]
    fn virtio_fs_queue_sel_and_ready() {
        let mut fs = VirtioFs::new(PathBuf::from("/tmp"));
        fs.handle_mmio_write(QUEUE_SEL, 1);
        fs.handle_mmio_write(QUEUE_READY, 1);
        assert_eq!(fs.handle_mmio_read(QUEUE_READY), 1);
    }

    #[test]
    fn virtio_fs_status_reset_clears_queues() {
        let mut fs = VirtioFs::new(PathBuf::from("/tmp"));
        fs.handle_mmio_write(QUEUE_SEL, 0);
        fs.handle_mmio_write(QUEUE_READY, 1);
        fs.handle_mmio_write(STATUS, 0); // reset
        assert_eq!(fs.handle_mmio_read(QUEUE_READY), 0);
    }

    #[test]
    fn virtio_fs_dispatch_unknown_opcode_returns_enosys() {
        let fs = VirtioFs::new(PathBuf::from("/tmp"));
        // Build a fake FUSE request with an invalid opcode (999)
        let mut data = vec![0u8; FUSE_IN_HEADER_SIZE];
        // len
        let len = FUSE_IN_HEADER_SIZE as u32;
        data[0..4].copy_from_slice(&len.to_ne_bytes());
        // opcode = 999
        data[4..8].copy_from_slice(&999u32.to_ne_bytes());
        // unique = 42
        data[8..16].copy_from_slice(&42u64.to_ne_bytes());

        let response = fs.dispatch_fuse(&data);
        assert!(!response.is_empty());
        // Check the error field in the out header (offset 4, i32)
        let error = i32::from_ne_bytes([response[4], response[5], response[6], response[7]]);
        assert_eq!(error, -libc::ENOSYS);
    }
}
