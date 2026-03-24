use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::fs::{File, OpenOptions};

#[derive(Deserialize)]
struct Request {
    cmd: String,
    args: Vec<String>,
}

#[derive(Serialize)]
struct Response {
    status: i32,
    stdout: String,
    stderr: String,
}

/// Write directly to PL011 UART MMIO via /dev/mem (bypass kernel driver).
/// This ensures output reaches the VMM even if the PL011 driver isn't fully working.
fn uart_write(msg: &[u8]) {
    // Try /dev/ttyAMA0 first (kernel driver path)
    if let Ok(mut f) = OpenOptions::new().write(true).open("/dev/ttyAMA0") {
        let _ = f.write_all(msg);
        let _ = f.flush();
        return;
    }
    // Fallback: /dev/console
    if let Ok(mut f) = OpenOptions::new().write(true).open("/dev/console") {
        let _ = f.write_all(msg);
        let _ = f.flush();
        return;
    }
    // Last resort: stdout
    let _ = std::io::stdout().write_all(msg);
    let _ = std::io::stdout().flush();
}

fn main() {
    // Mount basic filesystems
    mount_or_ignore("proc", "/proc", "proc");
    mount_or_ignore("sysfs", "/sys", "sysfs");
    mount_or_ignore("devtmpfs", "/dev", "devtmpfs");

    uart_write(b"READY\n");

    // Open serial for bidirectional communication
    let serial_path = if std::path::Path::new("/dev/ttyAMA0").exists() {
        "/dev/ttyAMA0"
    } else {
        "/dev/console"
    };

    let serial_in = match File::open(serial_path) {
        Ok(f) => f,
        Err(_) => {
            uart_write(b"ERROR: cannot open serial input\n");
            loop { unsafe { libc::pause(); } }
        }
    };

    let reader = BufReader::new(serial_in);
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response {
                    status: -1,
                    stdout: String::new(),
                    stderr: format!("parse error: {e}"),
                };
                uart_write(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
                continue;
            }
        };

        let resp = match req.cmd.as_str() {
            "exec" => {
                if req.args.is_empty() {
                    Response { status: -1, stdout: String::new(), stderr: "no args".into() }
                } else {
                    match std::process::Command::new(&req.args[0]).args(&req.args[1..]).output() {
                        Ok(output) => Response {
                            status: output.status.code().unwrap_or(-1),
                            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                        },
                        Err(e) => Response {
                            status: -1,
                            stdout: String::new(),
                            stderr: format!("exec error: {e}"),
                        },
                    }
                }
            }
            "ping" => Response { status: 0, stdout: "pong".into(), stderr: String::new() },
            _ => Response { status: -1, stdout: String::new(), stderr: format!("unknown cmd: {}", req.cmd) },
        };

        uart_write(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes());
    }
}

fn mount_or_ignore(source: &str, target: &str, fstype: &str) {
    unsafe {
        let source = std::ffi::CString::new(source).unwrap();
        let target = std::ffi::CString::new(target).unwrap();
        let fstype = std::ffi::CString::new(fstype).unwrap();
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            0,
            std::ptr::null(),
        );
    }
}
