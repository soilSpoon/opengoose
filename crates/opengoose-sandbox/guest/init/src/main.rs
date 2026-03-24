use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::fs::{self, File, OpenOptions};
use std::process::Command;

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

fn main() {
    let _ = fs::create_dir_all("/proc");
    let _ = fs::create_dir_all("/sys");
    let _ = fs::create_dir_all("/dev");
    let _ = fs::create_dir_all("/tmp");

    mount_or_ignore("proc", "/proc", "proc");
    mount_or_ignore("sysfs", "/sys", "sysfs");
    mount_or_ignore("devtmpfs", "/dev", "devtmpfs");

    let serial_path = if std::path::Path::new("/dev/ttyAMA0").exists() {
        "/dev/ttyAMA0"
    } else if std::path::Path::new("/dev/hvc0").exists() {
        "/dev/hvc0"
    } else {
        "/dev/console"
    };

    let serial_in = File::open(serial_path).expect("open serial for reading");
    let mut serial_out = OpenOptions::new()
        .write(true)
        .open(serial_path)
        .expect("open serial for writing");

    writeln!(serial_out, "READY").expect("write READY");

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
                let _ = writeln!(serial_out, "{}", serde_json::to_string(&resp).unwrap());
                continue;
            }
        };

        let resp = match req.cmd.as_str() {
            "exec" => {
                if req.args.is_empty() {
                    Response { status: -1, stdout: String::new(), stderr: "no args".into() }
                } else {
                    match Command::new(&req.args[0]).args(&req.args[1..]).output() {
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

        let _ = writeln!(serial_out, "{}", serde_json::to_string(&resp).unwrap());
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
