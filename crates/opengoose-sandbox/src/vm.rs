use crate::error::{SandboxError, Result};
use crate::hypervisor::*;
use crate::machine;
use crate::snapshot::{self, VmSnapshot};
use crate::uart::{self, Pl011};
use crate::boot;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(target_os = "macos")]
use crate::hypervisor::hvf::HvfHypervisor;

/// A forked VM instance created from a snapshot via CoW memory mapping.
pub struct MicroVm {
    #[cfg(target_os = "macos")]
    vm: <HvfHypervisor as Hypervisor>::Vm,
    #[cfg(target_os = "macos")]
    vcpu: <<HvfHypervisor as Hypervisor>::Vm as Vm>::Vcpu,
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
        let _ = booted.run_until_marker("READY", Duration::from_secs(10));

        // Save snapshot
        let vcpu_state = booted.vcpu.get_all_regs()?;
        let snap = VmSnapshot {
            vcpu_state,
            mem_size: booted.mem_size,
            kernel_hash: "default".into(),
        };
        snap.save(&meta_path)?;
        snapshot::save_memory(booted.mem_ptr, booted.mem_size, &mem_path)?;

        // BootedVm Drop will clean up the original VM
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

        Ok(MicroVm {
            vm,
            vcpu,
            uart: Pl011::new(),
            mem_ptr,
            mem_size,
        })
    }

    /// Execute a command in the guest and return the result.
    pub fn exec(&mut self, cmd: &str, args: &[&str], timeout: Duration) -> Result<ExecResult> {
        let all_args: Vec<&str> = std::iter::once(cmd).chain(args.iter().copied()).collect();
        let json = serde_json::json!({"cmd": "exec", "args": all_args});
        let input = format!("{}\n", json);

        self.uart.push_input(input.as_bytes());

        let start = std::time::Instant::now();
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
                }
                Ok(true)
            }
            VcpuExit::MmioRead { addr, len: _, reg } => {
                let val = if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                    self.uart.handle_mmio_read(addr - uart::PL011_BASE)
                } else {
                    0
                };
                if let Some(r) = reg_from_index(reg) {
                    let _ = self.vcpu.set_reg(r, val);
                }
                Ok(true)
            }
            VcpuExit::VtimerActivated => Ok(true),
            VcpuExit::SystemEvent => {
                if let Ok(pc) = self.vcpu.get_reg(Reg::Pc) {
                    let _ = self.vcpu.set_reg(Reg::Pc, pc + 4);
                }
                Ok(true)
            }
            VcpuExit::Unknown(_) => Ok(false),
        }
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

/// Safe register index to Reg enum conversion.
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
