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
    Getxattr = 22,
    Listxattr = 23,
    Flush = 25,
    Init = 26,
    Opendir = 27,
    Readdir = 28,
    Releasedir = 29,
    Access = 34,
    Create = 35,
    Destroy = 38,
    Readdirplus = 44,
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
            22 => Some(Self::Getxattr),
            23 => Some(Self::Listxattr),
            25 => Some(Self::Flush),
            26 => Some(Self::Init),
            27 => Some(Self::Opendir),
            28 => Some(Self::Readdir),
            29 => Some(Self::Releasedir),
            34 => Some(Self::Access),
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
/// This uses `read_unaligned`, so alignment is not required, but callers must
/// ensure `T` is a POD-like `Copy` type that is valid for any bit pattern:
/// no references, no niche/invalid representations, and no bytes that would
/// make the resulting value undefined to materialize.
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
    let end = remaining
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(remaining.len());
    String::from_utf8(remaining[..end].to_vec()).ok()
}

/// Build a FUSE response: header + body bytes.
pub fn build_response(unique: u64, error: i32, body: &[u8]) -> Vec<u8> {
    let len = (FUSE_OUT_HEADER_SIZE + body.len()) as u32;
    let header = FuseOutHeader { len, error, unique };
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
        assert_eq!(header.error, -libc::ENOENT);
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
