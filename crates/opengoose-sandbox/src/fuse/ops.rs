//! FUSE operation handlers.
//! Read ops access the host filesystem via InodeTable.
//! Write ops return EROFS (guest overlay handles writes).

use super::inode_table::{FUSE_ROOT_ID, InodeTable};
use super::*;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;

/// File handle table — maps fh to inode for open files/dirs.
struct HandleEntry {
    ino: u64,
    file: Option<fs::File>,
}

pub struct HandleTable {
    next_fh: u64,
    handles: HashMap<u64, HandleEntry>,
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::new()
    }
}

impl HandleTable {
    pub fn new() -> Self {
        HandleTable {
            next_fh: 1,
            handles: HashMap::new(),
        }
    }

    pub fn open(&mut self, ino: u64, inodes: &InodeTable) -> Option<u64> {
        let path = inodes.path(ino)?;
        let metadata = fs::symlink_metadata(&path).ok()?;
        let file = if metadata.is_dir() {
            None
        } else {
            Some(fs::File::open(&path).ok()?)
        };

        let fh = self.next_fh;
        self.next_fh = self.next_fh.checked_add(1)?;
        self.handles.insert(fh, HandleEntry { ino, file });
        Some(fh)
    }

    pub fn close(&mut self, fh: u64) {
        self.handles.remove(&fh);
    }

    pub fn get_ino(&self, fh: u64) -> Option<u64> {
        self.handles.get(&fh).map(|entry| entry.ino)
    }

    fn get_file_mut(&mut self, fh: u64) -> Option<&mut fs::File> {
        self.handles.get_mut(&fh)?.file.as_mut()
    }
}

fn metadata_to_attr(ino: u64, meta: &fs::Metadata) -> FuseAttr {
    let clamp_i64_to_u64 = |value: i64| if value >= 0 { value as u64 } else { 0 };
    let clamp_i64_to_u32 = |value: i64| if value >= 0 { value as u32 } else { 0 };
    FuseAttr {
        ino,
        size: meta.len(),
        blocks: meta.blocks(),
        atime: clamp_i64_to_u64(meta.atime()),
        mtime: clamp_i64_to_u64(meta.mtime()),
        ctime: clamp_i64_to_u64(meta.ctime()),
        atimensec: clamp_i64_to_u32(meta.atime_nsec()),
        mtimensec: clamp_i64_to_u32(meta.mtime_nsec()),
        ctimensec: clamp_i64_to_u32(meta.ctime_nsec()),
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

pub fn handle_lookup(unique: u64, parent: u64, name: &str, inodes: &mut InodeTable) -> Vec<u8> {
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

pub fn handle_getattr(unique: u64, nodeid: u64, inodes: &mut InodeTable) -> Vec<u8> {
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

pub fn handle_open(
    unique: u64,
    nodeid: u64,
    handles: &mut HandleTable,
    inodes: &mut InodeTable,
) -> Vec<u8> {
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
    handles: &mut HandleTable,
    inodes: &mut InodeTable,
) -> Vec<u8> {
    let Some(ino) = handles.get_ino(fh) else {
        return build_error_response(unique, libc::EBADF);
    };
    if inodes.path(ino).is_none() {
        return build_error_response(unique, libc::ENOENT);
    }
    let Some(file) = handles.get_file_mut(fh) else {
        return build_error_response(unique, libc::EIO);
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return build_response(unique, 0, &[]);
    }
    let mut buf = vec![0u8; size as usize];
    match file.read(&mut buf) {
        Ok(0) => build_response(unique, 0, &[]),
        Ok(n) => build_response(unique, 0, &buf[..n]),
        Err(_) => build_error_response(unique, libc::EIO),
    }
}

pub fn handle_release(unique: u64, fh: u64, handles: &mut HandleTable) -> Vec<u8> {
    handles.close(fh);
    build_response(unique, 0, &[])
}

pub fn handle_opendir(
    unique: u64,
    nodeid: u64,
    handles: &mut HandleTable,
    inodes: &mut InodeTable,
) -> Vec<u8> {
    handle_open(unique, nodeid, handles, inodes)
}

pub fn handle_readdir(
    unique: u64,
    fh: u64,
    offset: u64,
    size: u32,
    handles: &HandleTable,
    inodes: &mut InodeTable,
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

pub fn handle_releasedir(unique: u64, fh: u64, handles: &mut HandleTable) -> Vec<u8> {
    handle_release(unique, fh, handles)
}

pub fn handle_statfs(unique: u64, inodes: &mut InodeTable) -> Vec<u8> {
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

pub fn handle_create(
    unique: u64,
    _parent: u64,
    _name: &str,
    _flags: u32,
    _mode: u32,
    _inodes: &mut InodeTable,
) -> Vec<u8> {
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

// FORGET is handled directly in VirtioFs::dispatch_fuse and process_hiprio_queue
// (extracts nodeid + nlookup and calls inodes.forget)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuse::inode_table::{FUSE_ROOT_ID, InodeTable};
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
        let (_dir, mut inodes, _handles) = setup();
        let resp = handle_lookup(42, FUSE_ROOT_ID, "hello.txt", &mut inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
    }

    #[test]
    fn handle_lookup_missing_file() {
        let (_dir, mut inodes, _handles) = setup();
        let resp = handle_lookup(42, FUSE_ROOT_ID, "missing.txt", &mut inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, -libc::ENOENT);
    }

    #[test]
    fn handle_getattr_root() {
        let (_dir, mut inodes, _handles) = setup();
        let resp = handle_getattr(42, FUSE_ROOT_ID, &mut inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
    }

    #[test]
    fn handle_read_file_contents() {
        let (_dir, mut inodes, mut handles) = setup();
        let ino = inodes.lookup(FUSE_ROOT_ID, "hello.txt").unwrap();
        let fh = handles.open(ino, &inodes).unwrap();
        let resp = handle_read(42, fh, 0, 1024, &mut handles, &mut inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
        let body = &resp[FUSE_OUT_HEADER_SIZE..];
        assert_eq!(body, b"hello world");
    }

    #[test]
    fn handle_readdir_lists_entries() {
        let (_dir, mut inodes, mut handles) = setup();
        let fh = handles.open(FUSE_ROOT_ID, &inodes).unwrap();
        let resp = handle_readdir(42, fh, 0, 4096, &handles, &mut inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
        assert!(resp.len() > FUSE_OUT_HEADER_SIZE); // has directory entries
    }

    #[test]
    fn handle_statfs_returns_data() {
        let (_dir, mut inodes, _handles) = setup();
        let resp = handle_statfs(42, &mut inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, 0);
    }

    #[test]
    fn write_ops_return_erofs() {
        let (_dir, mut inodes, _handles) = setup();
        let _ino = inodes.lookup(FUSE_ROOT_ID, "hello.txt").unwrap();
        let resp = handle_create(42, FUSE_ROOT_ID, "new.txt", 0, 0o644, &mut inodes);
        let header: FuseOutHeader = unsafe { std::ptr::read_unaligned(resp.as_ptr() as *const _) };
        assert_eq!(header.error, -libc::EROFS);
    }
}
