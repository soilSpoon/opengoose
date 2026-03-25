use crate::boot;
use crate::error::{SandboxError, Result};
use crate::hypervisor::*;
use crate::machine;
use crate::snapshot::{self, VmSnapshot};
use crate::uart::{self, Pl011};
use crate::virtio::VirtioConsole;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use crate::hypervisor::hvf::HvfHypervisor;

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
    virtio: VirtioConsole,
    /// Exit counters for profiling (public for benchmarks)
    pub exit_counts: ExitCounts,
}

#[derive(Default, Debug)]
pub struct ExitCounts {
    pub mmio_read: u64,
    pub mmio_write: u64,
    pub vtimer: u64,
    pub wfi: u64,
    pub sysreg: u64,
    pub hvc: u64,
    pub canceled: u64,
}

// Safety: MicroVm is Send to allow storage in Mutex<Option<MicroVm>> (SandboxPool).
// HVF vCPU is thread-affine, but the pool contract ensures single-thread usage:
// acquire() → use on caller thread → release(). See HvfVcpu safety comment.
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

        // Log boot UART output for debugging (shows module loading, virtio detection)
        let boot_output = booted.uart.take_output();
        let boot_log = String::from_utf8_lossy(&boot_output);
        for line in boot_log.lines() {
            if line.contains("MODULE") || line.contains("VIRTIO") || line.contains("USING") || line.contains("virtio") {
                eprintln!("[boot] {line}");
            }
        }

        // Save snapshot (vCPU + GIC + vtimer state)
        let vcpu_state = booted.vcpu.get_all_regs()?;
        let gic_state = booted.vm.save_gic_state().ok();
        let vtimer_offset = booted.vcpu.get_vtimer_offset().ok();
        let virtio_state = Some(booted.virtio.save_state());
        let snap = VmSnapshot {
            vcpu_state,
            mem_size: booted.mem_size,
            kernel_hash: "default".into(),
            gic_state,
            vtimer_offset,
            virtio_state,
        };
        snap.save(&meta_path)?;
        snapshot::save_memory(booted.mem_ptr, booted.mem_size, &mem_path)?;

        drop(booted);

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

        let mut virtio = VirtioConsole::new();
        if let Some(vs) = &snapshot.virtio_state {
            virtio.restore_state(vs);
        }
        let mut micro = MicroVm {
            vcpu,
            vm,
            uart,
            mem_ptr,
            mem_size,
            vtimer_irq_pending: false,
            vtimer_masked: false,
            uart_irq_pending: false,
            virtio,
            exit_counts: ExitCounts::default(),
        };
        micro.virtio.suppress_kicks(micro.mem_ptr, micro.mem_size);
        micro.drain_canceled();
        Ok(micro)
    }

    /// Reset this VM for reuse: swap CoW memory and restore registers.
    /// Much faster than fork_from (skips vm_create + vcpu_create).
    #[cfg(target_os = "macos")]
    pub fn reset(&mut self, snapshot: &VmSnapshot, mem_path: &Path) -> Result<()> {
        // Unmap from HVF (invalidates Stage-2 TLB)
        self.vm.unmap_memory(machine::RAM_BASE, self.mem_size)?;

        // Remap CoW memory in place via MAP_FIXED (avoids munmap+mmap VMA churn).
        // MAP_FIXED atomically replaces the old mapping, discarding dirty pages.
        // No prefault — lazy page faults during exec are fast enough.
        use std::os::unix::io::AsRawFd;
        let file = std::fs::File::open(mem_path)?;
        let ptr = unsafe {
            libc::mmap(
                self.mem_ptr as *mut libc::c_void,
                self.mem_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_NORESERVE | libc::MAP_FIXED,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(SandboxError::Snapshot("CoW remap failed".into()));
        }
        drop(file);

        // Re-map into HVF (new Stage-2 entries)
        self.vm.map_memory(machine::RAM_BASE, self.mem_ptr, self.mem_size)?;

        // Restore vCPU state
        Self::restore_state(&mut self.vcpu, snapshot)?;

        // Reset interrupt state
        self.uart = Pl011::new();
        self.uart.restore_driver_state();
        self.vtimer_irq_pending = false;
        self.vtimer_masked = false;
        self.uart_irq_pending = false;
        self.vcpu.reset_irq_injection();
        self.virtio = VirtioConsole::new();
        if let Some(vs) = &snapshot.virtio_state {
            self.virtio.restore_state(vs);
        }
        self.virtio.suppress_kicks(self.mem_ptr, self.mem_size);
        self.exit_counts = ExitCounts::default();

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
            VcpuExit::WaitForEvent => { /* Don't advance PC — WFI re-executes */ }
            VcpuExit::HypervisorCall { .. } => {
                self.handle_psci();
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

    /// Collect raw UART output for a duration (for testing).
    pub fn collect_uart_output_raw(&mut self, timeout: Duration) -> Vec<u8> {
        let _wd = boot::spawn_watchdog(self.vcpu.vcpu_id(), timeout);
        let start = Instant::now();
        while start.elapsed() < timeout {
            match self.step_once() {
                Ok(true) => continue,
                Ok(false) => break,
                Err(_) => break,
            }
        }
        self.uart.take_output()
    }

    /// Execute a command in the guest via virtio-console.
    pub fn exec(&mut self, cmd: &str, args: &[&str], timeout: Duration) -> Result<ExecResult> {
        let all_args: Vec<&str> = std::iter::once(cmd).chain(args.iter().copied()).collect();
        let json = serde_json::json!({"cmd": "exec", "args": all_args});
        let input = format!("{}\n", json);

        // Send via virtio RX (primary path, always ready post-snapshot).
        self.virtio.push_input(input.as_bytes());

        let _wd = boot::spawn_watchdog(self.vcpu.vcpu_id(), timeout);
        let start = Instant::now();
        self.run_exec_loop(timeout, start)
    }

    fn run_exec_loop(&mut self, timeout: Duration, start: Instant) -> Result<ExecResult> {
        while start.elapsed() < timeout {
            // Deliver pending RX data to guest via virtio
            self.virtio.deliver_rx(self.mem_ptr, self.mem_size);

            match self.step_once()? {
                true => {
                    // Poll TX ring (kicks suppressed via VRING_USED_F_NO_NOTIFY)
                    self.virtio.poll_tx(self.mem_ptr, self.mem_size);
                    // Check virtio TX output first (fast path)
                    while let Some(line) = self.virtio.read_line() {
                        if let Ok(result) = serde_json::from_str::<ExecResult>(&line) {
                            return Ok(result);
                        }
                    }
                    // Check UART output (fallback)
                    while let Some(line) = self.uart.read_line() {
                        if let Ok(result) = serde_json::from_str::<ExecResult>(&line) {
                            return Ok(result);
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
        let pending = self.vtimer_irq_pending || self.uart_irq_pending || self.virtio.irq_pending();
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
        // Increment exit counters
        match &exit {
            VcpuExit::MmioRead { .. } => self.exit_counts.mmio_read += 1,
            VcpuExit::MmioWrite { .. } => self.exit_counts.mmio_write += 1,
            VcpuExit::VtimerActivated => self.exit_counts.vtimer += 1,
            VcpuExit::WaitForEvent => self.exit_counts.wfi += 1,
            VcpuExit::SystemRegAccess { .. } => self.exit_counts.sysreg += 1,
            VcpuExit::HypervisorCall { .. } => self.exit_counts.hvc += 1,
            VcpuExit::Unknown(0) => self.exit_counts.canceled += 1,
            VcpuExit::Unknown(_) => {}
        }
        match exit {
            VcpuExit::MmioWrite { addr, len: _, srt } => {
                let data = reg_from_index(srt)
                    .and_then(|r| self.vcpu.get_reg(r).ok())
                    .unwrap_or(0);
                if addr >= machine::VIRTIO_MMIO_BASE && addr < machine::VIRTIO_MMIO_BASE + machine::VIRTIO_MMIO_SIZE {
                    let offset = addr - machine::VIRTIO_MMIO_BASE;
                    self.virtio.handle_mmio_write(offset, data);
                    if offset == 0x050 {
                        self.virtio.process_notify(data as u32, self.mem_ptr, self.mem_size);
                    }
                } else if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                    self.uart.handle_mmio_write(addr - uart::PL011_BASE, data);
                }
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::MmioRead { addr, len: _, reg } => {
                let val = if addr >= machine::VIRTIO_MMIO_BASE && addr < machine::VIRTIO_MMIO_BASE + machine::VIRTIO_MMIO_SIZE {
                    self.virtio.handle_mmio_read(addr - machine::VIRTIO_MMIO_BASE)
                } else if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
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
                self.handle_psci();
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
                } else if self.virtio.irq_pending() {
                    self.virtio.handle_mmio_write(0x064, 1); // ACK interrupt
                    34u64 // SPI 2 = virtio (intid 32+2)
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

    /// Handle HVC exit — minimal PSCI. PC is already past HVC (ARM64 convention).
    fn handle_psci(&mut self) {
        let x0 = self.vcpu.get_reg(Reg::X0).unwrap_or(0);
        const PSCI_VERSION: u64 = 0x84000000;
        const PSCI_RET_NOT_SUPPORTED: u64 = (-1i64) as u64;
        let ret = match x0 {
            PSCI_VERSION => 0x00010000, // PSCI 1.0
            _ => PSCI_RET_NOT_SUPPORTED,
        };
        let _ = self.vcpu.set_reg(Reg::X0, ret);
    }

    fn update_uart_irq(&mut self) {
        // No-op: polling guest init doesn't need interrupt injection.
    }

    fn handle_mmio_read(&self, addr: u64) -> u64 {
        machine::handle_gic_redist_read(addr).unwrap_or(0)
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

#[derive(serde::Deserialize)]
pub struct ExecResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

