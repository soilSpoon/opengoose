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
    /// Pending interrupts for software GIC emulation (QEMU-style).
    /// When vtimer fires or UART RX has data, we set pending IRQ here.
    vtimer_irq_pending: bool,
    vtimer_masked: bool,
    uart_irq_pending: bool,
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

        let mut vcpu = vm.create_vcpu()?;
        Self::restore_state(&mut vcpu, snapshot)?;

        let mut uart = Pl011::new();
        uart.restore_driver_state();

        let mut micro = MicroVm {
            vcpu,
            vm,
            uart,
            mem_ptr,
            mem_size,
            vtimer_irq_pending: false,
            vtimer_masked: false,
            uart_irq_pending: false,
        };
        micro.drain_canceled();
        Ok(micro)
    }

    /// Reset this VM for reuse: swap CoW memory and restore registers.
    /// Much faster than fork_from (skips vm_create + vcpu_create).
    #[cfg(target_os = "macos")]
    pub fn reset(&mut self, snapshot: &VmSnapshot, mem_path: &Path) -> Result<()> {
        // Unmap old memory
        self.vm.unmap_memory(machine::RAM_BASE, self.mem_size)?;
        // munmap old CoW mapping
        unsafe { libc::munmap(self.mem_ptr as *mut libc::c_void, self.mem_size); }

        // Map new CoW memory
        let (mem_ptr, mem_size) = snapshot::cow_map(mem_path, snapshot.mem_size)?;
        self.vm.map_memory(machine::RAM_BASE, mem_ptr, mem_size)?;
        self.mem_ptr = mem_ptr;
        self.mem_size = mem_size;

        // Restore vCPU state
        Self::restore_state(&mut self.vcpu, snapshot)?;

        // Reset interrupt state
        self.uart = Pl011::new();
        self.uart.restore_driver_state();
        self.vtimer_irq_pending = false;
        self.vtimer_masked = false;
        self.uart_irq_pending = false;
        self.vcpu.reset_irq_injection();

        Ok(())
    }

    /// Restore vCPU registers + vtimer + CPSR from snapshot.
    #[cfg(target_os = "macos")]
    fn restore_state(vcpu: &mut <<HvfHypervisor as Hypervisor>::Vm as Vm>::Vcpu, snapshot: &VmSnapshot) -> Result<()> {
        vcpu.set_all_regs(&snapshot.vcpu_state)?;
        if let Some(offset) = snapshot.vtimer_offset {
            let _ = vcpu.set_vtimer_offset(offset);
        }
        vcpu.set_vtimer_mask(false);
        if let Ok(cpsr) = vcpu.get_reg(Reg::Cpsr) {
            let _ = vcpu.set_reg(Reg::Cpsr, cpsr & !(1 << 7));
        }
        Ok(())
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
            VcpuExit::SystemRegAccess { .. } => { let _ = self.advance_pc(); }
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

    /// Get pointer to mailbox page in guest RAM (host-side).
    fn mailbox_ptr(&self) -> *mut u8 {
        unsafe { self.mem_ptr.add(machine::MAILBOX_OFFSET) }
    }

    /// Write request to host→guest mailbox and wake guest via IRQ.
    fn mailbox_send(&mut self, data: &[u8]) -> Result<()> {
        if data.len() > machine::MBOX_H2G_DATA_MAX {
            return Err(SandboxError::Exec("mailbox message too large".into()));
        }
        let mbox = self.mailbox_ptr();
        unsafe {
            // Write length
            let len = data.len() as u32;
            std::ptr::write_volatile(mbox.add(machine::MBOX_H2G_LEN_OFF) as *mut u32, len);
            // Write data
            std::ptr::copy_nonoverlapping(data.as_ptr(), mbox.add(machine::MBOX_H2G_DATA_OFF), data.len());
            // Memory barrier (ARM64 store barrier)
            #[cfg(target_arch = "aarch64")]
            std::arch::asm!("dmb st");
        }
        // Wake guest via IRQ injection
        self.uart_irq_pending = true;
        Ok(())
    }

    /// Read response from guest→host mailbox. Returns None if length is 0.
    fn mailbox_recv(&self) -> Option<Vec<u8>> {
        let mbox = self.mailbox_ptr();
        unsafe {
            // Memory barrier before reading
            #[cfg(target_arch = "aarch64")]
            std::arch::asm!("dmb ld");
            let len = std::ptr::read_volatile(mbox.add(machine::MBOX_G2H_LEN_OFF) as *const u32) as usize;
            if len == 0 || len > machine::MBOX_G2H_DATA_MAX {
                return None;
            }
            let mut buf = vec![0u8; len];
            std::ptr::copy_nonoverlapping(mbox.add(machine::MBOX_G2H_DATA_OFF), buf.as_mut_ptr(), len);
            // Clear length to indicate we've read it
            std::ptr::write_volatile(mbox.add(machine::MBOX_G2H_LEN_OFF) as *mut u32, 0);
            Some(buf)
        }
    }

    /// Execute a command via shared memory mailbox (fast path).
    /// Falls back to UART exec if mailbox is not supported by guest.
    pub fn exec(&mut self, cmd: &str, args: &[&str], timeout: Duration) -> Result<ExecResult> {
        let all_args: Vec<&str> = std::iter::once(cmd).chain(args.iter().copied()).collect();
        let json = serde_json::json!({"cmd": "exec", "args": all_args});
        let input = format!("{}\n", json);

        // Send via UART (always works) + mailbox (fast path if guest supports it)
        self.uart.push_input(input.as_bytes());
        self.uart_irq_pending = true;
        let _ = self.mailbox_send(input.as_bytes());

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
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        result
    }

    fn run_exec_loop_mailbox(&mut self, timeout: Duration, start: Instant) -> Result<ExecResult> {
        while start.elapsed() < timeout {
            match self.step_once()? {
                true => {
                    // Check mailbox for response (doorbell MMIO write from guest)
                    if let Some(data) = self.mailbox_recv() {
                        if let Ok(text) = std::str::from_utf8(&data) {
                            if let Ok(resp) = serde_json::from_str::<ExecResponse>(text) {
                                return Ok(ExecResult {
                                    status: resp.status,
                                    stdout: resp.stdout,
                                    stderr: resp.stderr,
                                });
                            }
                        }
                    }
                    // Also check UART (fallback for boot-time messages)
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

    /// Execute a command in the guest via UART (legacy path).
    pub fn exec_uart(&mut self, cmd: &str, args: &[&str], timeout: Duration) -> Result<ExecResult> {
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

    /// Sync vtimer state (QEMU-style): check if guest acknowledged the timer interrupt.
    fn sync_vtimer(&mut self) {
        if !self.vtimer_masked { return; }
        // Read CNTV_CTL_EL0: bits [2]=ISTATUS, [1]=IMASK, [0]=ENABLE
        let ctl = self.vcpu.get_sys_reg(SysReg::CntvCtlEl0).unwrap_or(0);
        let enable = ctl & 1;
        let imask = (ctl >> 1) & 1;
        let istatus = (ctl >> 2) & 1;
        // Timer IRQ is pending if ENABLE=1, IMASK=0, ISTATUS=1
        let irq_state = enable == 1 && imask == 0 && istatus == 1;
        self.vtimer_irq_pending = irq_state;
        if !irq_state {
            // Guest cleared/masked the timer — re-enable HVF vtimer notifications
            self.vcpu.set_vtimer_mask(false);
            self.vtimer_masked = false;
        }
    }

    /// Check if any IRQ is pending and inject via hv_vcpu_set_pending_interrupt.
    fn inject_interrupts(&mut self) {
        let pending = self.vtimer_irq_pending || self.uart_irq_pending;
        self.vcpu.set_irq_pending(pending);
    }

    fn step_once(&mut self) -> Result<bool> {
        // QEMU-style vtimer sync on every exit
        self.sync_vtimer();
        // Update UART IRQ state
        self.uart_irq_pending = self.uart.irq_pending();
        // Inject pending IRQs before vcpu_run
        self.inject_interrupts();

        let exit = self.vcpu.run()?;
        match exit {
            VcpuExit::MmioWrite { addr, len: _, srt } => {
                if addr == machine::MAILBOX_DOORBELL {
                    // Guest rang the doorbell — response is in shared memory
                    // (mailbox_recv will be checked by the exec loop)
                } else {
                    let data = reg_from_index(srt)
                        .and_then(|r| self.vcpu.get_reg(r).ok())
                        .unwrap_or(0);
                    if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                        self.uart.handle_mmio_write(addr - uart::PL011_BASE, data);
                    }
                }
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::MmioRead { addr, len: _, reg } => {
                let val = if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                    self.uart.handle_mmio_read(addr - uart::PL011_BASE)
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
                // QEMU-style: mark vtimer IRQ pending, mask HVF vtimer
                self.vtimer_irq_pending = true;
                self.vtimer_masked = true;
                // Don't unmask here — sync_vtimer will unmask when guest clears it
                Ok(true)
            }
            VcpuExit::WaitForEvent => {
                // Don't advance PC — let CPU re-execute WFI.
                // Reset injection tracking so next step_once re-injects IRQ if still pending.
                self.vcpu.reset_irq_injection();
                Ok(true)
            }
            VcpuExit::HypervisorCall { .. } => {
                let _ = self.vcpu.set_reg(Reg::X0, (-1i64) as u64);
                Ok(true)
            }
            VcpuExit::SystemRegAccess { syndrome } => {
                self.handle_sysreg(syndrome)?;
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::Unknown(0) => Ok(true), // CANCELED — retry
            VcpuExit::Unknown(_) => Ok(false),
        }
    }

    /// Handle trapped system register access (MSR/MRS).
    /// Emulates GIC CPU interface registers without hv_gic_create.
    fn handle_sysreg(&mut self, syndrome: u64) -> Result<()> {
        // ESR encoding for MSR/MRS (EC=0x18):
        // bit 0: direction (0=write/MSR, 1=read/MRS)
        // bits 9:5: Rt (register index)
        // bits 19:17, 16:14, 13:10, 4:1: Op0, Op2, CRn, CRm, Op1
        let is_read = syndrome & 1 == 1;
        let rt = ((syndrome >> 5) & 0x1f) as u8;
        let op0 = ((syndrome >> 20) & 3) as u8;
        let op1 = ((syndrome >> 14) & 7) as u8;
        let crn = ((syndrome >> 10) & 0xf) as u8;
        let crm = ((syndrome >> 1) & 0xf) as u8;
        let op2 = ((syndrome >> 17) & 7) as u8;

        // Identify the system register by encoding
        match (op0, op1, crn, crm, op2) {
            // ICC_IAR1_EL1: read → return highest-priority pending intid (or 1023=spurious)
            (3, 0, 12, 12, 0) if is_read => {
                let intid = if self.vtimer_irq_pending {
                    self.vtimer_irq_pending = false;
                    27u64 // PPI 27 = virtual timer
                } else if self.uart_irq_pending {
                    self.uart_irq_pending = false;
                    33u64 // SPI 1 = UART (intid 32+1)
                } else {
                    1023u64 // spurious
                };
                if let Some(r) = reg_from_index(rt) {
                    let _ = self.vcpu.set_reg(r, intid);
                }
            }
            // ICC_EOIR1_EL1: write → end of interrupt
            (3, 0, 12, 12, 1) if !is_read => {
                // ACK — nothing to do, we already cleared pending in IAR read
            }
            // ICC_PMR_EL1: priority mask — return 0xFF (allow all)
            (3, 0, 4, 6, 0) if is_read => {
                if let Some(r) = reg_from_index(rt) {
                    let _ = self.vcpu.set_reg(r, 0xFF);
                }
            }
            // ICC_CTLR_EL1: read → return 0 (defaults)
            (3, 0, 12, 12, 4) if is_read => {
                if let Some(r) = reg_from_index(rt) {
                    let _ = self.vcpu.set_reg(r, 0);
                }
            }
            // ICC_IGRPEN1_EL1: interrupt group enable — return 1 (enabled)
            (3, 0, 12, 12, 7) if is_read => {
                if let Some(r) = reg_from_index(rt) {
                    let _ = self.vcpu.set_reg(r, 1);
                }
            }
            // All other system register accesses — ignore (read returns 0)
            _ => {
                if is_read {
                    if let Some(r) = reg_from_index(rt) {
                        let _ = self.vcpu.set_reg(r, 0);
                    }
                }
            }
        }
        Ok(())
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
