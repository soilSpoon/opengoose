use std::io::Write;

fn main() {
    // Write READY to /dev/console (fd 1 is already connected to console by kernel)
    let _ = std::io::stdout().write_all(b"READY\n");
    let _ = std::io::stdout().flush();

    // Also try writing to /dev/console directly
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/console") {
        let _ = f.write_all(b"READY\n");
    }

    // Sleep forever (PID 1 must not exit)
    loop {
        unsafe { libc::pause(); }
    }
}
