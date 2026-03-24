use crate::error::{SandboxError, Result};

/// Build a minimal cpio newc archive containing a single file as /init.
pub fn build_initramfs(init_binary: &[u8]) -> Vec<u8> {
    let mut archive = Vec::new();
    append_cpio_entry(&mut archive, "init", init_binary, 0o100755);
    append_cpio_entry(&mut archive, "TRAILER!!!", &[], 0);
    while archive.len() % 512 != 0 {
        archive.push(0);
    }
    archive
}

fn append_cpio_entry(archive: &mut Vec<u8>, name: &str, data: &[u8], mode: u32) {
    let name_with_nul = format!("{name}\0");
    let namesize = name_with_nul.len();

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
        0,              // ino
        mode,           // mode
        0,              // uid
        0,              // gid
        1,              // nlink
        0,              // mtime
        data.len(),     // filesize
        0, 0,           // devmajor, devminor
        0, 0,           // rdevmajor, rdevminor
        namesize,       // namesize
        0,              // checksum
    );

    archive.extend_from_slice(header.as_bytes());
    archive.extend_from_slice(name_with_nul.as_bytes());
    while archive.len() % 4 != 0 {
        archive.push(0);
    }
    archive.extend_from_slice(data);
    while archive.len() % 4 != 0 {
        archive.push(0);
    }
}

/// Load the pre-built guest init binary from known locations.
pub fn load_guest_init() -> Result<Vec<u8>> {
    let candidates = [
        concat!(env!("CARGO_MANIFEST_DIR"), "/guest/init/target/aarch64-unknown-linux-musl/release/sandbox-guest-init"),
    ];
    for path in &candidates {
        if let Ok(data) = std::fs::read(path) {
            return Ok(data);
        }
    }
    Err(SandboxError::Boot(
        "guest init binary not found. Build it with: \
         cd crates/opengoose-sandbox/guest/init && \
         cargo build --release --target aarch64-unknown-linux-musl".into()
    ))
}
