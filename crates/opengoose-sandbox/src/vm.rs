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

        // Run until guest init sends SNAPSHOT marker from clean userspace state.
        booted.run_until_snapshot_marker(Duration::from_secs(30))?;

        // Save snapshot (vCPU + GIC + vtimer state)
        let vcpu_state = booted.vcpu.get_all_regs()?;
        let gic_state = booted.vm.save_gic_state().ok();
        let vtimer_offset = booted.vcpu.get_vtimer_offset().ok();
        let snap = VmSnapshot {
            vcpu_state,
            mem_size: booted.mem_size,
            kernel_hash: "default".into(),
            gic_state,
            vtimer_offset,
        };
        snap.save(&meta_path)?;
        snapshot::save_memory(booted.mem_ptr, booted.mem_size, &mem_path)?;

        // Explicitly drop the boot VM and wait for any pending watchdog threads to settle.
        // HVF vCPU IDs can be reused, and a stale hv_vcpus_exit from a watchdog
        // could cancel the next vCPU if it gets the same ID.
        drop(booted);
        std::thread::sleep(Duration::from_millis(500));

        Ok((snap, mem_path))
    }

    /// Fork a new VM from a snapshot using CoW memory mapping.
    #[cfg(target_os = "macos")]
    pub fn fork_from(snapshot: &VmSnapshot, mem_path: &Path) -> Result<Self> {
        let (mem_ptr, mem_size) = snapshot::cow_map(mem_path, snapshot.mem_size)?;

        let hv = HvfHypervisor;
        let mut vm = hv.create_vm()?;
        vm.map_memory(machine::RAM_BASE, mem_ptr, mem_size)?;

        // NO GIC in fork — guest init uses direct MMIO polling, not interrupts.
        // Without GIC, VtimerActivated exits come directly to VMM,
        // and GIC MMIO reads return 0 (unhandled data abort → kernel handles gracefully).

        let mut vcpu = vm.create_vcpu()?;
        vcpu.set_all_regs(&snapshot.vcpu_state)?;

        // Restore virtual timer offset so guest timer works correctly
        if let Some(offset) = snapshot.vtimer_offset {
            let _ = vcpu.set_vtimer_offset(offset);
        }
        // Unmask vtimer (HVF auto-masks on VTIMER_ACTIVATED exit)
        vcpu.set_vtimer_mask(false);

        // Unmask IRQ in CPSR so pending interrupts can be delivered immediately.
        // At snapshot time, the kernel may have had IRQ masked (in a spinlock section).
        // Clearing CPSR.I lets the GIC deliver SPI interrupts right away.
        if let Ok(cpsr) = vcpu.get_reg(Reg::Cpsr) {
            let _ = vcpu.set_reg(Reg::Cpsr, cpsr & !(1 << 7));
        }

        // Start with TX interrupt enabled (kernel driver has it enabled at snapshot time)
        let mut uart = Pl011::new();
        uart.restore_driver_state();

        let mut micro = MicroVm {
            vcpu,
            vm,
            uart,
            mem_ptr,
            mem_size,
        };

        // hv_gic_create + hv_gic_set_state cause CANCELED exits.
        // Drain all pending CANCELEDs before returning.
        micro.drain_canceled();

        Ok(micro)
    }

    /// Run vCPU once and return the exit reason (for testing/debugging).
    #[cfg(target_os = "macos")]
    pub fn vcpu_run(&mut self) -> Result<VcpuExit> {
        self.vcpu.run()
    }

    /// Handle a VM exit (for testing/debugging). Mirrors step_once logic.
    pub fn handle_exit(&mut self, exit: VcpuExit) {
        match exit {
            VcpuExit::MmioWrite { addr, len: _, srt } => {
                let data = reg_from_index(srt)
                    .and_then(|r| self.vcpu.get_reg(r).ok())
                    .unwrap_or(0);
                if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                    self.uart.handle_mmio_write(addr - uart::PL011_BASE, data);
                    self.update_uart_irq();
                }
                let _ = self.advance_pc();
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
                let _ = self.advance_pc();
            }
            VcpuExit::WaitForEvent => { let _ = self.advance_pc(); }
            VcpuExit::HypervisorCall { .. } => {
                let _ = self.vcpu.set_reg(Reg::X0, (-1i64) as u64);
            }
            VcpuExit::SystemRegAccess => { let _ = self.advance_pc(); }
            VcpuExit::VtimerActivated | VcpuExit::Unknown(_) => {}
        }
    }

    /// Access vcpu for register inspection (testing).
    #[cfg(target_os = "macos")]
    pub fn vcpu(&self) -> &<<HvfHypervisor as Hypervisor>::Vm as Vm>::Vcpu {
        &self.vcpu
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
        let mut iters = 0u64;
        while start.elapsed() < timeout {
            iters += 1;
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
        eprintln!("[exec] timeout after {iters} iterations, elapsed={:?}", start.elapsed());
        Err(SandboxError::Timeout(timeout))
    }

    fn step_once(&mut self) -> Result<bool> {
        let exit = self.vcpu.run()?;
        // Debug: log first few exits
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < 30 || matches!(&exit, VcpuExit::Unknown(_)) {
            let desc = match &exit {
                VcpuExit::MmioWrite { addr, .. } => format!("MmioWrite {addr:#x}"),
                VcpuExit::MmioRead { addr, .. } => format!("MmioRead {addr:#x}"),
                VcpuExit::WaitForEvent => "WFI".into(),
                VcpuExit::HypervisorCall { imm } => format!("HVC#{imm}"),
                VcpuExit::SystemRegAccess => "SysReg".into(),
                VcpuExit::VtimerActivated => "VTimer".into(),
                VcpuExit::Unknown(c) => format!("Unknown({c:#x})"),
            };
            eprintln!("[step {n}] {desc} irq={}", self.uart.irq_pending());
        }
        match exit {
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
            VcpuExit::VtimerActivated => {
                // HVF auto-masks the vtimer after this exit. Unmask it so
                // the guest's timer interrupts keep working (sleep, usleep, etc.)
                self.vcpu.set_vtimer_mask(false);
                Ok(true)
            }
            VcpuExit::WaitForEvent => {
                // Do NOT advance PC past WFI — let the CPU re-execute it.
                // If an interrupt is pending, WFI completes immediately.
                // If not, it will trap again and we loop.
                Ok(true)
            }
            VcpuExit::HypervisorCall { .. } => {
                let _ = self.vcpu.set_reg(Reg::X0, (-1i64) as u64);
                Ok(true)
            }
            VcpuExit::SystemRegAccess => {
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::Unknown(0) => {
                // HV_EXIT_REASON_CANCELED — from hv_gic_set_state or stale watchdog.
                // Safe to retry.
                Ok(true)
            }
            VcpuExit::Unknown(_) => Ok(false),
        }
    }

    /// Consume ALL pending CANCELED exits left by hv_gic_create + hv_gic_set_state.
    /// These exits appear immediately on the next hv_vcpu_run after GIC operations.
    /// Drain initial exits after fork (CANCELED from GIC, VTimer that needs unmasking).
    /// Runs until we see a "normal" exit (MMIO, WFI) indicating guest is running properly.
    fn drain_canceled(&mut self) {
        for i in 0..100 {
            match self.vcpu.run() {
                Ok(VcpuExit::Unknown(0)) => continue, // CANCELED — drain
                Ok(VcpuExit::VtimerActivated) => {
                    // Unmask vtimer so it keeps firing
                    self.vcpu.set_vtimer_mask(false);
                    continue; // Keep draining until we see a real guest exit
                }
                Ok(_) => {
                    eprintln!("[drain] settled after {i} pre-exits");
                    return;
                }
                Err(_) => return,
            }
        }
    }

    fn update_uart_irq(&mut self) {
        // No-op: polling guest init doesn't need interrupt injection.
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
