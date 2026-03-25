use crate::error::{Result, SandboxError};

/// Build a minimal cpio newc archive with /init and basic directory structure.
pub fn build_initramfs(init_binary: &[u8]) -> Vec<u8> {
    let mut archive = Vec::new();
    let mut ino = 1u32;

    // Directories
    for dir in &["dev", "proc", "sys", "tmp"] {
        append_cpio_dir(&mut archive, dir, ino);
        ino += 1;
    }

    // /dev/console — character device, major 5 minor 1
    append_cpio_dev(&mut archive, "dev/console", ino, 5, 1);
    ino += 1;

    // /dev/ttyAMA0 — character device, major 204 minor 64 (PL011)
    append_cpio_dev(&mut archive, "dev/ttyAMA0", ino, 204, 64);
    ino += 1;

    // /dev/null — character device, major 1 minor 3
    append_cpio_dev(&mut archive, "dev/null", ino, 1, 3);
    ino += 1;

    // /dev/mem — character device, major 1 minor 1 (for direct MMIO access)
    append_cpio_dev(&mut archive, "dev/mem", ino, 1, 1);
    ino += 1;

    // /lib/modules/ directory for kernel modules
    append_cpio_dir(&mut archive, "lib", ino);
    ino += 1;
    append_cpio_dir(&mut archive, "lib/modules", ino);
    ino += 1;

    // Include virtio_mmio.ko if available (enables virtio-console fast path)
    if let Some(ko_data) = load_kernel_module("virtio_mmio.ko") {
        append_cpio_entry(
            &mut archive,
            "lib/modules/virtio_mmio.ko",
            &ko_data,
            0o100644,
            ino,
            0,
            0,
        );
        ino += 1;
    }

    // /init — the guest init binary
    append_cpio_entry(&mut archive, "init", init_binary, 0o100755, ino, 0, 0);

    // Trailer
    append_cpio_entry(&mut archive, "TRAILER!!!", &[], 0, 0, 0, 0);
    while archive.len() % 512 != 0 {
        archive.push(0);
    }
    archive
}

fn append_cpio_dir(archive: &mut Vec<u8>, name: &str, ino: u32) {
    append_cpio_entry(archive, name, &[], 0o040755, ino, 0, 0);
}

fn append_cpio_dev(archive: &mut Vec<u8>, name: &str, ino: u32, major: u32, minor: u32) {
    // Character device: mode 0o020666
    append_cpio_entry(archive, name, &[], 0o020666, ino, major, minor);
}

fn append_cpio_entry(
    archive: &mut Vec<u8>,
    name: &str,
    data: &[u8],
    mode: u32,
    ino: u32,
    rdev_major: u32,
    rdev_minor: u32,
) {
    let name_with_nul = format!("{name}\0");
    let namesize = name_with_nul.len();
    let nlink: u32 = if mode & 0o040000 != 0 { 2 } else { 1 }; // dirs get nlink=2

    let header = format!(
        "070701\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}",
        ino,        // ino
        mode,       // mode
        0,          // uid
        0,          // gid
        nlink,      // nlink
        0,          // mtime
        data.len(), // filesize
        0,
        0, // devmajor, devminor
        rdev_major,
        rdev_minor, // rdevmajor, rdevminor
        namesize,   // namesize
        0,          // checksum
    );

    archive.extend_from_slice(header.as_bytes());
    archive.extend_from_slice(name_with_nul.as_bytes());
    while !archive.len().is_multiple_of(4) {
        archive.push(0);
    }
    archive.extend_from_slice(data);
    while !archive.len().is_multiple_of(4) {
        archive.push(0);
    }
}

/// Try to load a kernel module (.ko) from the kernel cache.
fn load_kernel_module(name: &str) -> Option<Vec<u8>> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home)
        .join(".opengoose/kernel/aarch64/modules")
        .join(name);
    std::fs::read(&path).ok()
}

/// Load the pre-built guest init binary from known locations.
pub fn load_guest_init() -> Result<Vec<u8>> {
    let candidates = [concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/guest/init/target/aarch64-unknown-linux-musl/release/sandbox-guest-init"
    )];
    for path in &candidates {
        if let Ok(data) = std::fs::read(path) {
            return Ok(data);
        }
    }
    Err(SandboxError::Boot(
        "guest init binary not found. Build it with: \
         cd crates/opengoose-sandbox/guest/init && \
         cargo build --release --target aarch64-unknown-linux-musl"
            .into(),
    ))
}
