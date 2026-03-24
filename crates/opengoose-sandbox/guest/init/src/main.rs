use serde::{Deserialize, Serialize};
use std::io::Write;
use std::fs::OpenOptions;

const PL011_BASE: usize = 0x0900_0000;
const UARTDR: usize = 0x000;    // Data Register
const UARTFR: usize = 0x018;    // Flag Register
const UARTFR_RXFE: u32 = 1 << 4; // RX FIFO Empty
const UARTFR_TXFF: u32 = 1 << 5; // TX FIFO Full

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

/// Direct MMIO UART access via mmap of /dev/mem
struct DirectUart {
    base: *mut u8,
}

impl DirectUart {
    fn new() -> Option<Self> {
        let fd = unsafe {
            libc::open(b"/dev/mem\0".as_ptr() as *const _, libc::O_RDWR | libc::O_SYNC)
        };
        if fd < 0 { return None; }
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                0x1000,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                PL011_BASE as libc::off_t,
            )
        };
        unsafe { libc::close(fd); }
        if base == libc::MAP_FAILED { return None; }
        Some(DirectUart { base: base as *mut u8 })
    }

    fn read_reg(&self, offset: usize) -> u32 {
        unsafe { std::ptr::read_volatile((self.base as *const u32).add(offset / 4)) }
    }

    fn write_reg(&self, offset: usize, val: u32) {
        unsafe { std::ptr::write_volatile((self.base as *mut u32).add(offset / 4), val) }
    }

    fn tx_byte(&self, b: u8) {
        // Wait until TX FIFO not full
        while self.read_reg(UARTFR) & UARTFR_TXFF != 0 {}
        self.write_reg(UARTDR, b as u32);
    }

    fn tx_bytes(&self, data: &[u8]) {
        for &b in data {
            self.tx_byte(b);
        }
    }

    fn rx_ready(&self) -> bool {
        self.read_reg(UARTFR) & UARTFR_RXFE == 0
    }

    fn rx_byte(&self) -> u8 {
        (self.read_reg(UARTDR) & 0xFF) as u8
    }
}

/// Fallback: write via /dev/ttyAMA0 (for READY message before polling starts)
fn uart_write_tty(msg: &[u8]) {
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

    // Write READY + SNAPSHOT via kernel driver (interrupt-driven TX still works during boot)
    uart_write_tty(b"READY\n");
    uart_write_tty(b"SNAPSHOT\n");

    // Switch to direct MMIO access for the polling loop.
    // This bypasses the kernel PL011 driver entirely.
    let uart = match DirectUart::new() {
        Some(u) => u,
        None => {
            uart_write_tty(b"ERROR: cannot mmap UART\n");
            loop { unsafe { libc::pause(); } }
        }
    };

    let mut line_buf = Vec::with_capacity(4096);

    // Busy-poll UART for input — no interrupts needed
    loop {
        if uart.rx_ready() {
            let byte = uart.rx_byte();
            if byte == b'\n' {
                process_line(&line_buf, &uart);
                line_buf.clear();
            } else {
                line_buf.push(byte);
            }
        }
        // No sleep/yield — tight poll. Each rx_ready() check causes MMIO exit to VMM.
    }
}

fn process_line(line: &[u8], uart: &DirectUart) {
    let line = match std::str::from_utf8(line) {
        Ok(s) => s.trim(),
        Err(_) => return,
    };
    if line.is_empty() { return; }

    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response {
                status: -1,
                stdout: String::new(),
                stderr: format!("parse error: {e}"),
            };
            let out = format!("{}\n", serde_json::to_string(&resp).unwrap());
            uart.tx_bytes(out.as_bytes());
            return;
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

    let out = format!("{}\n", serde_json::to_string(&resp).unwrap());
    uart.tx_bytes(out.as_bytes());
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
