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

fn uart_write(msg: &[u8]) {
    if let Ok(mut f) = OpenOptions::new().write(true).open("/dev/ttyAMA0") {
        let _ = f.write_all(msg);
        let _ = f.flush();
        return;
    }
    if let Ok(mut f) = OpenOptions::new().write(true).open("/dev/console") {
        let _ = f.write_all(msg);
        let _ = f.flush();
    }
}

fn main() {
    mount_or_ignore("proc", "/proc", "proc");
    mount_or_ignore("sysfs", "/sys", "sysfs");
    mount_or_ignore("devtmpfs", "/dev", "devtmpfs");

    uart_write(b"READY\n");

    // Try virtio-console paths first (fast: bulk transfer via shared memory ring)
    let virtio_paths = ["/dev/vport0p0", "/dev/hvc0"];
    for path in &virtio_paths {
        if std::path::Path::new(path).exists() {
            uart_write(format!("VIRTIO:{path}\n").as_bytes());
        }
    }

    uart_write(b"SNAPSHOT\n");

    // Try virtio console
    for path in &virtio_paths {
        if let Ok(serial_in) = File::open(path) {
            if let Ok(serial_out) = OpenOptions::new().write(true).open(path) {
                uart_write(format!("USING:{path}\n").as_bytes());
                run_loop(serial_in, serial_out);
            }
        }
    }

    // Fallback: UART
    let serial_path = if std::path::Path::new("/dev/ttyAMA0").exists() {
        "/dev/ttyAMA0"
    } else {
        "/dev/console"
    };
    uart_write(b"USING:UART\n");
    let serial_in = File::open(serial_path).unwrap();
    let serial_out = OpenOptions::new().write(true).open(serial_path).unwrap();
    run_loop(serial_in, serial_out);
}

fn run_loop(serial_in: File, mut serial_out: File) -> ! {
    let reader = BufReader::new(serial_in);
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if line.trim().is_empty() { continue; }

        let resp = process_request(line.trim());
        let json = format!("{}\n", serde_json::to_string(&resp).unwrap());
        let _ = serial_out.write_all(json.as_bytes());
        let _ = serial_out.flush();
    }
    loop { unsafe { libc::pause(); } }
}

fn process_request(line: &str) -> Response {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return Response { status: -1, stdout: String::new(), stderr: format!("parse error: {e}") };
        }
    };

    match req.cmd.as_str() {
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
