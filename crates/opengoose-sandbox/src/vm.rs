use crate::error::{SandboxError, Result};
use crate::hypervisor::*;
use crate::machine;
use crate::snapshot::{self, VmSnapshot};
use crate::uart::{self, Pl011};
use crate::boot;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use crate::hypervisor::hvf::{self, HvfHypervisor};

/// A forked VM instance created from a snapshot via CoW memory mapping.
/// Field order matters: vcpu must be dropped before vm (HVF constraint).
pub struct MicroVm {
    #[cfg(target_os = "macos")]
    vcpu: <<HvfHypervisor as Hypervisor>::Vm as Vm>::Vcpu,
    #[cfg(target_os = "macos")]
    vm: <HvfHypervisor as Hypervisor>::Vm,
    uart: Pl011,
    mem_ptr: *mut u8,
    mem_size: usize,
}

unsafe impl Send for MicroVm {}

impl MicroVm {
    /// Ensure a snapshot exists (create if needed). Returns (snapshot, mem_path).
    #[cfg(target_os = "macos")]
    pub fn ensure_snapshot() -> Result<(VmSnapshot, PathBuf)> {
        let cache_dir = VmSnapshot::cache_dir()?;
        let meta_path = cache_dir.join("snapshot.meta");
        let mem_path = cache_dir.join("snapshot.mem");

        if meta_path.exists() && mem_path.exists() {
            let snap = VmSnapshot::load(&meta_path)?;
            return Ok((snap, mem_path));
        }

        // Boot a fresh VM
        let hv = HvfHypervisor;
        let mut booted = boot::boot(&hv, machine::DEFAULT_RAM_SIZE as usize)?;

        // Run until guest init prints "READY" or timeout
        let _ = booted.run_until_marker("READY", Duration::from_secs(30));

        // Save snapshot
        let vcpu_state = booted.vcpu.get_all_regs()?;
        let snap = VmSnapshot {
            vcpu_state,
            mem_size: booted.mem_size,
            kernel_hash: "default".into(),
        };
        snap.save(&meta_path)?;
        snapshot::save_memory(booted.mem_ptr, booted.mem_size, &mem_path)?;

        Ok((snap, mem_path))
    }

    /// Fork a new VM from a snapshot using CoW memory mapping.
    #[cfg(target_os = "macos")]
    pub fn fork_from(snapshot: &VmSnapshot, mem_path: &Path) -> Result<Self> {
        let (mem_ptr, mem_size) = snapshot::cow_map(mem_path, snapshot.mem_size)?;

        let hv = HvfHypervisor;
        let mut vm = hv.create_vm()?;
        vm.map_memory(machine::RAM_BASE, mem_ptr, mem_size)?;

        vm.create_gic(&GicConfig {
            dist_addr: machine::GIC_DIST_ADDR,
            dist_size: machine::GIC_DIST_SIZE,
            redist_addr: machine::GIC_REDIST_ADDR,
            redist_size: machine::GIC_REDIST_SIZE,
        })?;

        let mut vcpu = vm.create_vcpu()?;
        vcpu.set_all_regs(&snapshot.vcpu_state)?;

        // Start with TX interrupt enabled (kernel driver has it enabled at snapshot time)
        let mut uart = Pl011::new();
        uart.restore_driver_state();

        Ok(MicroVm {
            vm,
            vcpu,
            uart,
            mem_ptr,
            mem_size,
        })
    }

    /// Push data to the UART input buffer (for testing).
    pub fn uart_push_input(&mut self, data: &[u8]) {
        self.uart.push_input(data);
        self.update_uart_irq();
    }

    /// Collect raw UART output for a duration (for testing).
    pub fn collect_uart_output_raw(&mut self, timeout: Duration) -> Vec<u8> {
        let vcpu_id = self.vcpu.vcpu_id();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < timeout {
                if cancel_clone.load(std::sync::atomic::Ordering::Relaxed) { return; }
                std::thread::sleep(Duration::from_millis(50));
            }
            if !cancel_clone.load(std::sync::atomic::Ordering::Relaxed) {
                #[cfg(target_os = "macos")]
                { let _ = hvf::force_vcpu_exit(vcpu_id); }
            }
        });

        let start = Instant::now();
        while start.elapsed() < timeout {
            match self.step_once() {
                Ok(true) => continue,
                Ok(false) => break,
                Err(_) => break,
            }
        }
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        self.uart.take_output()
    }

    /// Execute a command in the guest and return the result.
    pub fn exec(&mut self, cmd: &str, args: &[&str], timeout: Duration) -> Result<ExecResult> {
        let all_args: Vec<&str> = std::iter::once(cmd).chain(args.iter().copied()).collect();
        let json = serde_json::json!({"cmd": "exec", "args": all_args});
        let input = format!("{}\n", json);

        self.uart.push_input(input.as_bytes());
        // Inject RX interrupt so the guest wakes up to read the input
        self.update_uart_irq();

        // Watchdog to force exit after timeout
        let vcpu_id = self.vcpu.vcpu_id();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < timeout {
                if cancel_clone.load(std::sync::atomic::Ordering::Relaxed) { return; }
                std::thread::sleep(Duration::from_millis(50));
            }
            if !cancel_clone.load(std::sync::atomic::Ordering::Relaxed) {
                #[cfg(target_os = "macos")]
                { let _ = hvf::force_vcpu_exit(vcpu_id); }
            }
        });

        let start = Instant::now();
        let result = self.run_exec_loop(timeout, start);

        // Cancel watchdog
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);

        result
    }

    fn run_exec_loop(&mut self, timeout: Duration, start: Instant) -> Result<ExecResult> {
        while start.elapsed() < timeout {
            match self.step_once()? {
                true => {
                    while let Some(line) = self.uart.read_line() {
                        if let Ok(resp) = serde_json::from_str::<ExecResponse>(&line) {
                            return Ok(ExecResult {
                                status: resp.status,
                                stdout: resp.stdout,
                                stderr: resp.stderr,
                            });
                        }
                    }
                }
                false => break,
            }
        }
        Err(SandboxError::Timeout(timeout))
    }

    fn step_once(&mut self) -> Result<bool> {
        match self.vcpu.run()? {
            VcpuExit::MmioWrite { addr, len: _, srt } => {
                let data = reg_from_index(srt)
                    .and_then(|r| self.vcpu.get_reg(r).ok())
                    .unwrap_or(0);
                if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                    self.uart.handle_mmio_write(addr - uart::PL011_BASE, data);
                    self.update_uart_irq();
                }
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::MmioRead { addr, len: _, reg } => {
                let val = if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                    let v = self.uart.handle_mmio_read(addr - uart::PL011_BASE);
                    self.update_uart_irq();
                    v
                } else {
                    self.handle_mmio_read(addr)
                };
                if let Some(r) = reg_from_index(reg) {
                    let _ = self.vcpu.set_reg(r, val);
                }
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::VtimerActivated => Ok(true),
            VcpuExit::WaitForEvent => {
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::HypervisorCall => {
                let _ = self.vcpu.set_reg(Reg::X0, (-1i64) as u64);
                Ok(true)
            }
            VcpuExit::SystemRegAccess => {
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::Unknown(_) => Ok(false),
        }
    }

    fn update_uart_irq(&mut self) {
        let intid = uart::PL011_IRQ;
        let pending = self.uart.irq_pending();
        let _ = self.vm.set_spi(intid, pending);
    }

    fn handle_mmio_read(&self, addr: u64) -> u64 {
        let redist_base = machine::GIC_REDIST_ADDR;
        let redist_end = redist_base + machine::GIC_REDIST_SIZE;
        if addr >= redist_base && addr < redist_end {
            let offset = addr - redist_base;
            match offset {
                0x0000 => return 0,
                0x0004 => return 0x0100_043B,
                0x0008 => return 1 << 4,
                0x000C => return 0,
                0x0010 => return 0,
                0x0014 => return 0,
                0xFFE8 => return 0x3B,
                0x10080 => return 0,
                0x10100 => return 0,
                0x10180 => return 0,
                0x10C00 | 0x10C04 => return 0,
                _ => {}
            }
            if offset >= 0x10400 && offset < 0x10420 {
                return 0;
            }
        }
        0
    }

    fn advance_pc(&mut self) -> Result<()> {
        let pc = self.vcpu.get_reg(Reg::Pc)?;
        self.vcpu.set_reg(Reg::Pc, pc + 4)
    }
}

impl Drop for MicroVm {
    fn drop(&mut self) {
        if !self.mem_ptr.is_null() {
            unsafe { libc::munmap(self.mem_ptr as *mut std::ffi::c_void, self.mem_size); }
            self.mem_ptr = std::ptr::null_mut();
        }
    }
}

pub struct ExecResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(serde::Deserialize)]
struct ExecResponse {
    status: i32,
    stdout: String,
    stderr: String,
}

fn reg_from_index(idx: u8) -> Option<Reg> {
    match idx {
        0 => Some(Reg::X0), 1 => Some(Reg::X1), 2 => Some(Reg::X2), 3 => Some(Reg::X3),
        4 => Some(Reg::X4), 5 => Some(Reg::X5), 6 => Some(Reg::X6), 7 => Some(Reg::X7),
        8 => Some(Reg::X8), 9 => Some(Reg::X9), 10 => Some(Reg::X10), 11 => Some(Reg::X11),
        12 => Some(Reg::X12), 13 => Some(Reg::X13), 14 => Some(Reg::X14), 15 => Some(Reg::X15),
        16 => Some(Reg::X16), 17 => Some(Reg::X17), 18 => Some(Reg::X18), 19 => Some(Reg::X19),
        20 => Some(Reg::X20), 21 => Some(Reg::X21), 22 => Some(Reg::X22), 23 => Some(Reg::X23),
        24 => Some(Reg::X24), 25 => Some(Reg::X25), 26 => Some(Reg::X26), 27 => Some(Reg::X27),
        28 => Some(Reg::X28), 29 => Some(Reg::X29), 30 => Some(Reg::X30),
        _ => None,
    }
}
