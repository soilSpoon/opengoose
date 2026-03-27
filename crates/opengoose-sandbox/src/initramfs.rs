use crate::error::{Result, SandboxError};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Alpine busybox-static APK for aarch64.
const BUSYBOX_APK_URL: &str =
    "https://dl-cdn.alpinelinux.org/alpine/v3.21/main/aarch64/busybox-static-1.37.0-r14.apk";
const BUSYBOX_APK_ENTRY: &str = "bin/busybox.static";

/// Commands to symlink from /bin/<cmd> → /bin/busybox.
/// Minimal set for cargo build/test + shell scripting.
const BUSYBOX_SYMLINKS: &[&str] = &[
    "sh", "ash", "cat", "cp", "mv", "rm", "ln", "ls", "mkdir", "rmdir", "chmod", "chown",
    "echo", "printf", "test", "[", "true", "false", "sleep", "uname", "id", "whoami", "pwd",
    "head", "tail", "wc", "tr", "cut", "sort", "uniq", "tee", "xargs", "find", "grep", "sed",
    "awk", "diff", "patch", "tar", "gzip", "gunzip", "touch", "date", "basename", "dirname",
    "readlink", "realpath", "which", "expr", "seq", "env", "mount", "umount",
];

/// Build a minimal cpio newc archive with /init, busybox, and basic directory structure.
pub fn build_initramfs(init_binary: &[u8]) -> Vec<u8> {
    let busybox = load_busybox();
    let mut archive = Vec::new();
    let mut ino = 1u32;

    // Directories
    for dir in &[
        "dev", "proc", "sys", "tmp", "bin", "sbin", "usr", "usr/bin", "usr/sbin",
    ] {
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

    // BusyBox: /bin/busybox + symlinks for common commands
    if let Some(ref bb_data) = busybox {
        append_cpio_entry(&mut archive, "bin/busybox", bb_data, 0o100755, ino, 0, 0);
        ino += 1;

        for cmd in BUSYBOX_SYMLINKS {
            append_cpio_symlink(&mut archive, &format!("bin/{cmd}"), "/bin/busybox", ino);
            ino += 1;
        }

        // /usr/bin/env → /bin/busybox (for #!/usr/bin/env shebangs)
        append_cpio_symlink(&mut archive, "usr/bin/env", "/bin/busybox", ino);
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

fn append_cpio_symlink(archive: &mut Vec<u8>, name: &str, target: &str, ino: u32) {
    // Symlink: mode 0o120777, data = target path
    append_cpio_entry(archive, name, target.as_bytes(), 0o120777, ino, 0, 0);
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

/// Load cached busybox binary, downloading if necessary.
/// Returns None if download fails (non-fatal — VM boots without shell tools).
fn load_busybox() -> Option<Vec<u8>> {
    let cache_dir = kernel_cache_dir().ok()?;
    let busybox_path = cache_dir.join("busybox");

    if busybox_path.exists() {
        return std::fs::read(&busybox_path).ok();
    }

    log::info!("Downloading Alpine busybox-static for aarch64...");
    if let Err(e) = download_busybox(&cache_dir, &busybox_path) {
        log::warn!("BusyBox download failed: {e} — VM will boot without shell tools");
        return None;
    }

    log::info!("BusyBox cached at {}", busybox_path.display());
    std::fs::read(&busybox_path).ok()
}

fn download_busybox(cache_dir: &Path, busybox_path: &Path) -> Result<()> {
    let mut curl = Command::new("curl")
        .args([
            "-fSL",
            "--connect-timeout",
            "30",
            "--max-time",
            "300",
            BUSYBOX_APK_URL,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| SandboxError::Boot(format!("curl not found: {e}")))?;

    let curl_stdout = curl
        .stdout
        .take()
        .ok_or_else(|| SandboxError::Boot("failed to capture curl stdout".into()))?;

    let tar_status = Command::new("tar")
        .args(["xzf", "-", "-C"])
        .arg(cache_dir)
        .arg(BUSYBOX_APK_ENTRY)
        .stdin(curl_stdout)
        .status()
        .map_err(|e| SandboxError::Boot(format!("tar failed: {e}")))?;

    let curl_status = curl
        .wait()
        .map_err(|e| SandboxError::Boot(format!("curl wait failed: {e}")))?;

    if !curl_status.success() {
        return Err(SandboxError::Boot(format!(
            "busybox download failed (exit {})",
            curl_status.code().unwrap_or(-1)
        )));
    }
    if !tar_status.success() {
        return Err(SandboxError::Boot(format!(
            "busybox extraction failed (exit {})",
            tar_status.code().unwrap_or(-1)
        )));
    }

    // tar extracts to cache_dir/bin/busybox.static — rename to final path
    let extracted = cache_dir.join("bin").join("busybox.static");
    std::fs::rename(&extracted, busybox_path)
        .map_err(|e| SandboxError::Boot(format!("rename busybox: {e}")))?;
    let _ = std::fs::remove_dir(cache_dir.join("bin"));

    Ok(())
}

fn kernel_cache_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| SandboxError::Boot("HOME not set".into()))?;
    let dir = PathBuf::from(home)
        .join(".opengoose")
        .join("kernel")
        .join("aarch64");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
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
