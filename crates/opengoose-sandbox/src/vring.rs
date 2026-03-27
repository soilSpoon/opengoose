//! Shared virtqueue helper functions for virtio devices.
//! Used by both VirtioConsole and VirtioFs.
//!
//! All functions that accept `mem_ptr` perform bounds-checking before access,
//! but the caller must ensure `mem_ptr` is valid for `mem_size` bytes.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::machine;

/// Vring descriptor flags.
pub(crate) const VRING_DESC_F_NEXT: u16 = 1;
pub(crate) const VRING_DESC_F_WRITE: u16 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VringDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

pub(crate) fn gpa_to_offset(gpa: u64, mem_size: usize) -> Option<usize> {
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

pub(crate) fn read_desc(mem_ptr: *mut u8, mem_size: usize, desc_base: u64, idx: u64) -> VringDesc {
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

pub(crate) fn read_guest_buf(mem_ptr: *mut u8, mem_size: usize, gpa: u64, len: usize) -> Vec<u8> {
    let Some(offset) = gpa_to_offset(gpa, mem_size) else {
        return Vec::new();
    };
    if offset + len > mem_size {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(mem_ptr.add(offset), len).to_vec() }
}

pub(crate) fn write_guest_buf(mem_ptr: *mut u8, mem_size: usize, gpa: u64, data: &[u8]) {
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

pub(crate) fn read_u16(mem_ptr: *mut u8, mem_size: usize, gpa: u64) -> u16 {
    let Some(offset) = gpa_to_offset(gpa, mem_size) else {
        return 0;
    };
    if offset + 2 > mem_size {
        return 0;
    }
    unsafe { (mem_ptr.add(offset) as *const u16).read_unaligned() }
}

pub(crate) fn write_u16(mem_ptr: *mut u8, mem_size: usize, gpa: u64, val: u16) {
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

pub(crate) fn write_u32(mem_ptr: *mut u8, mem_size: usize, gpa: u64, val: u32) {
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
