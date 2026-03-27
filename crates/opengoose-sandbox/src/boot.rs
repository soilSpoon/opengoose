use crate::error::{Result, SandboxError};
use crate::hypervisor::*;
use crate::machine;
use crate::uart::{self, Pl011};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use crate::hypervisor::hvf::HvfHypervisor;

// Alpine linux-virt kernel for aarch64 — EFI ZBOOT (gzip-compressed PE32+).
// We download the APK, extract vmlinuz-virt, then decompress the gzip payload
// to get the raw ARM64 Image.
const KERNEL_APK_URL: &str =
    "https://dl-cdn.alpinelinux.org/alpine/v3.21/main/aarch64/linux-virt-6.12.77-r0.apk";
const KERNEL_APK_ENTRY: &str = "boot/vmlinuz-virt";

/// Ensure a raw ARM64 kernel Image exists locally.
/// Downloads Alpine linux-virt APK, extracts vmlinuz-virt, decompresses ZBOOT payload.
pub fn ensure_kernel() -> Result<PathBuf> {
    let cache_dir = kernel_cache_dir()?;

    // Prefer custom-built kernel (has VIRTIO_MMIO=y built-in)
    let custom_path = cache_dir.join("Image.custom");
    if custom_path.exists() {
        log::info!("Using custom kernel: {}", custom_path.display());
        return Ok(custom_path);
    }

    // Fallback to Alpine pre-built kernel
    let image_path = cache_dir.join("Image");
    if image_path.exists() {
        return Ok(image_path);
    }

    log::info!("Downloading Alpine linux-virt kernel...");

    // Download APK and extract vmlinuz-virt via curl | tar
    let vmlinuz_path = cache_dir.join("vmlinuz-virt");
    download_vmlinuz(&cache_dir, &vmlinuz_path)?;

    // Decompress ZBOOT → raw ARM64 Image
    log::info!("Decompressing ZBOOT kernel to raw Image...");
    decompress_zboot(&vmlinuz_path, &image_path)?;

    // Clean up compressed vmlinuz
    let _ = std::fs::remove_file(&vmlinuz_path);

    log::info!("Kernel cached at {}", image_path.display());
    Ok(image_path)
}

fn download_vmlinuz(cache_dir: &Path, vmlinuz_path: &Path) -> Result<()> {
    let mut curl = std::process::Command::new("curl")
        .args([
            "-fSL",
            "--connect-timeout",
            "30",
            "--max-time",
            "300",
            KERNEL_APK_URL,
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| SandboxError::Boot(format!("curl not found: {e}")))?;

    let curl_stdout = curl
        .stdout
        .take()
        .ok_or_else(|| SandboxError::Boot("failed to capture curl stdout".into()))?;

    let tar_status = std::process::Command::new("tar")
        .args(["xzf", "-", "-C"])
        .arg(cache_dir)
        .arg(KERNEL_APK_ENTRY)
        .stdin(curl_stdout)
        .status()
        .map_err(|e| SandboxError::Boot(format!("tar failed: {e}")))?;

    let curl_status = curl
        .wait()
        .map_err(|e| SandboxError::Boot(format!("curl wait failed: {e}")))?;

    if !curl_status.success() {
        return Err(SandboxError::Boot(format!(
            "kernel download failed (exit {})",
            curl_status.code().unwrap_or(-1)
        )));
    }
    if !tar_status.success() {
        return Err(SandboxError::Boot(format!(
            "kernel extraction failed (exit {})",
            tar_status.code().unwrap_or(-1)
        )));
    }

    // tar extracts to cache_dir/boot/vmlinuz-virt — move to final path
    let extracted = cache_dir.join("boot").join("vmlinuz-virt");
    std::fs::rename(&extracted, vmlinuz_path)
        .map_err(|e| SandboxError::Boot(format!("rename kernel: {e}")))?;
    let _ = std::fs::remove_dir(cache_dir.join("boot"));
    Ok(())
}

/// Decompress an EFI ZBOOT kernel to raw ARM64 Image.
/// ZBOOT header: offset 4 = "zimg", offset 8 = gzip payload offset (u32 LE),
/// offset 12 = gzip payload size (u32 LE).
fn decompress_zboot(vmlinuz_path: &Path, image_path: &Path) -> Result<()> {
    let data = std::fs::read(vmlinuz_path)
        .map_err(|e| SandboxError::Boot(format!("read vmlinuz: {e}")))?;

    if data.len() < 16 || &data[4..8] != b"zimg" {
        return Err(SandboxError::Boot(
            "not a ZBOOT kernel (missing 'zimg' magic)".into(),
        ));
    }

    let payload_offset = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
    let payload_size = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;

    if payload_offset + payload_size > data.len() {
        return Err(SandboxError::Boot(
            "ZBOOT payload extends beyond file".into(),
        ));
    }

    let payload = &data[payload_offset..payload_offset + payload_size];
    let mut decoder = flate2::read::GzDecoder::new(payload);
    let mut image = Vec::new();
    decoder
        .read_to_end(&mut image)
        .map_err(|e| SandboxError::Boot(format!("gzip decompress: {e}")))?;

    // Verify ARM64 Image magic at offset 0x38
    if image.len() > 0x3c && &image[0x38..0x3c] == b"ARM\x64" {
        log::info!("ARM64 Image: {} bytes", image.len());
    } else {
        log::warn!("Decompressed kernel missing ARM64 magic — may not boot");
    }

    std::fs::write(image_path, &image)
        .map_err(|e| SandboxError::Boot(format!("write Image: {e}")))?;
    Ok(())
}

fn kernel_cache_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| SandboxError::Boot("HOME not set".into()))?;
    let dir = PathBuf::from(home)
        .join(".opengoose")
        .join("kernel")
        .join("aarch64");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A booted VM with UART, ready for snapshot or direct interaction.
/// Field order matters: vcpu must be dropped before vm (HVF constraint).
pub struct BootedVm<V: Vm> {
    pub vcpu: V::Vcpu,
    pub vm: V,
    pub uart: Pl011,
    pub mem_ptr: *mut u8,
    pub mem_size: usize,
    pub virtio: crate::virtio::VirtioConsole,
    pub virtio_fs: Option<crate::virtio_fs::VirtioFs>,
}

// Safety: BootedVm owns mem_ptr exclusively (not aliased). V: Vm is Send.
// Required because raw pointers are !Send, but this pointer is an owned allocation.
unsafe impl<V: Vm> Send for BootedVm<V> {}

#[cfg(target_os = "macos")]
impl BootedVm<<HvfHypervisor as Hypervisor>::Vm> {
    pub fn boot_default() -> Result<Self> {
        let hv = HvfHypervisor;
        boot(&hv, machine::DEFAULT_RAM_SIZE as usize)
    }
}

/// Boot a Linux VM using a kernel Image file.
pub fn boot<H: Hypervisor>(hv: &H, ram_size: usize) -> Result<BootedVm<H::Vm>> {
    // 1. Ensure kernel is available
    let kernel_path = ensure_kernel()?;

    // 2. Allocate guest memory
    let mem_ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            ram_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_ANON | libc::MAP_PRIVATE,
            -1,
            0,
        )
    };
    if mem_ptr == libc::MAP_FAILED {
        return Err(SandboxError::Boot("mmap failed".into()));
    }
    let mem_ptr = mem_ptr as *mut u8;

    // 3. Load kernel into guest memory (raw copy to RAM_BASE offset 0)
    let kernel_end = match load_kernel(&kernel_path, mem_ptr, ram_size) {
        Ok(end) => end,
        Err(e) => {
            unsafe { libc::munmap(mem_ptr as *mut libc::c_void, ram_size) };
            return Err(e);
        }
    };

    // 4. Try to load initramfs (optional — boot continues without it)
    let initrd_info = match crate::initramfs::load_guest_init() {
        Ok(init_bin) => {
            let cpio = crate::initramfs::build_initramfs(&init_bin);
            // Place initramfs after kernel, page-aligned
            let initrd_gpa = machine::dtb_addr(kernel_end); // reuse alignment fn
            let initrd_offset = (initrd_gpa - machine::RAM_BASE) as usize;
            if initrd_offset + cpio.len() > ram_size {
                log::warn!("initramfs doesn't fit in RAM, skipping");
                None
            } else {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        cpio.as_ptr(),
                        mem_ptr.add(initrd_offset),
                        cpio.len(),
                    );
                }
                let initrd_end_gpa = initrd_gpa + cpio.len() as u64;
                log::info!(
                    "Initramfs loaded at {initrd_gpa:#x}-{initrd_end_gpa:#x} ({} bytes)",
                    cpio.len()
                );
                Some((initrd_gpa, initrd_end_gpa))
            }
        }
        Err(e) => {
            log::info!("No guest init binary: {e} — booting without initramfs");
            None
        }
    };

    // 5. Create and place DTB after initramfs (or after kernel if no initramfs)
    let dtb_base = initrd_info.map(|(_, end)| end).unwrap_or(kernel_end);
    let dtb_gpa = machine::dtb_addr(dtb_base);
    let dtb_offset = (dtb_gpa - machine::RAM_BASE) as usize;

    let initrd_for_dtb = initrd_info.map(|(start, end)| machine::InitrdInfo {
        start_gpa: start,
        end_gpa: end,
    });
    let dtb_bytes = match machine::create_dtb_with_initrd(ram_size as u64, initrd_for_dtb.as_ref())
    {
        Ok(b) => b,
        Err(e) => {
            unsafe { libc::munmap(mem_ptr as *mut libc::c_void, ram_size) };
            return Err(e);
        }
    };

    if dtb_offset + dtb_bytes.len() > ram_size {
        unsafe { libc::munmap(mem_ptr as *mut libc::c_void, ram_size) };
        return Err(SandboxError::Boot("DTB does not fit in RAM".into()));
    }

    unsafe {
        std::ptr::copy_nonoverlapping(dtb_bytes.as_ptr(), mem_ptr.add(dtb_offset), dtb_bytes.len());
    }

    // Helper: clean up mem on VM setup errors
    macro_rules! try_vm {
        ($expr:expr) => {
            match $expr {
                Ok(v) => v,
                Err(e) => {
                    unsafe { libc::munmap(mem_ptr as *mut libc::c_void, ram_size) };
                    return Err(e);
                }
            }
        };
    }

    // 5. Create VM
    let mut vm = try_vm!(hv.create_vm());
    try_vm!(vm.map_memory(machine::RAM_BASE, mem_ptr, ram_size));

    // 6. Create GIC
    try_vm!(vm.create_gic(&GicConfig {
        dist_addr: machine::GIC_DIST_ADDR,
        dist_size: machine::GIC_DIST_SIZE,
        redist_addr: machine::GIC_REDIST_ADDR,
        redist_size: machine::GIC_REDIST_SIZE,
    }));

    // 7. Create vCPU and set boot registers
    let mut vcpu = try_vm!(vm.create_vcpu());

    // MPIDR_EL1: affinity 0 (matches GIC default IROUTER=0 for SPI routing)
    try_vm!(vcpu.set_sys_reg(SysReg::MpidrEl1, 0x80000000)); // bit 31 = uniprocessor

    // PC = kernel entry (RAM_BASE)
    try_vm!(vcpu.set_reg(Reg::Pc, machine::RAM_BASE));
    // X0 = DTB address
    try_vm!(vcpu.set_reg(Reg::X0, dtb_gpa));
    // X1, X2, X3 = 0
    try_vm!(vcpu.set_reg(Reg::X1, 0));
    try_vm!(vcpu.set_reg(Reg::X2, 0));
    try_vm!(vcpu.set_reg(Reg::X3, 0));
    // PSTATE = EL1h with DAIF masked
    let pstate: u64 = 0b0101  // EL1h
        | (1 << 6)   // FIQ mask
        | (1 << 7)   // IRQ mask
        | (1 << 8)   // SError mask
        | (1 << 9); // Debug mask
    try_vm!(vcpu.set_reg(Reg::Cpsr, pstate));

    // Create VirtioFs with dummy root so kernel probes the driver at boot.
    // After fork, set_root() swaps in the real worktree.
    let virtio_fs = Some(crate::virtio_fs::VirtioFs::new(std::env::temp_dir()));

    Ok(BootedVm {
        vm,
        vcpu,
        uart: Pl011::new(),
        mem_ptr,
        mem_size: ram_size,
        virtio: crate::virtio::VirtioConsole::new(),
        virtio_fs,
    })
}

/// Load kernel Image into guest memory at RAM_BASE.
/// Returns the guest physical address of the kernel end.
fn load_kernel(kernel_path: &Path, mem_ptr: *mut u8, ram_size: usize) -> Result<u64> {
    let kernel_data =
        std::fs::read(kernel_path).map_err(|e| SandboxError::Boot(format!("read kernel: {e}")))?;

    if kernel_data.len() > ram_size {
        return Err(SandboxError::Boot(format!(
            "kernel ({} bytes) exceeds RAM ({} bytes)",
            kernel_data.len(),
            ram_size
        )));
    }

    // Copy kernel to start of guest memory (RAM_BASE offset 0)
    unsafe {
        std::ptr::copy_nonoverlapping(kernel_data.as_ptr(), mem_ptr, kernel_data.len());
    }

    // ARM64 Image header: offset 16 = image_size (includes BSS).
    // Use image_size (not file size) to avoid placing initramfs/DTB in BSS.
    let image_size = if kernel_data.len() >= 24 {
        u64::from_le_bytes(kernel_data[16..24].try_into().unwrap())
    } else {
        kernel_data.len() as u64
    };
    let kernel_end = machine::RAM_BASE + image_size.max(kernel_data.len() as u64);

    Ok(kernel_end)
}

/// Spawn a watchdog thread that forces a vCPU exit after `timeout`.
/// Returns a guard; dropping it cancels the watchdog.
pub fn spawn_watchdog(vcpu_id: u64, timeout: Duration) -> WatchdogGuard {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if cancel_clone.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !cancel_clone.load(std::sync::atomic::Ordering::Acquire) {
            #[cfg(target_os = "macos")]
            {
                let _ = crate::hypervisor::hvf::force_vcpu_exit(vcpu_id);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = vcpu_id;
        }
    });
    WatchdogGuard { cancel }
}

/// Cancels the watchdog thread when dropped.
pub struct WatchdogGuard {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for WatchdogGuard {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

impl<V: Vm> BootedVm<V> {
    /// Run the VM processing UART MMIO exits until timeout.
    /// Returns accumulated UART output.
    pub fn collect_uart_output(&mut self, timeout: Duration) -> String {
        let _wd = spawn_watchdog(self.vcpu.vcpu_id(), timeout);
        let start = Instant::now();
        while start.elapsed() < timeout {
            match self.step_once() {
                Ok(true) => continue,
                Ok(false) => break,
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&self.uart.take_output()).to_string()
    }

    /// Run until guest init is in a clean polling state.
    /// 1. Wait for "SNAPSHOT" marker on UART
    /// 2. Run for ~100ms more to let kernel settle (return from write(), reach idle)
    pub fn run_until_snapshot_marker(&mut self, timeout: Duration) -> Result<()> {
        let _ = self.run_until_marker("SNAPSHOT", timeout)?;
        Ok(())
    }

    /// Run until a specific marker string appears in UART output.
    pub fn run_until_marker(&mut self, marker: &str, timeout: Duration) -> Result<String> {
        let _wd = spawn_watchdog(self.vcpu.vcpu_id(), timeout);
        let start = Instant::now();
        let mut all_output = String::new();
        while start.elapsed() < timeout {
            match self.step_once() {
                Ok(true) => {
                    while let Some(line) = self.uart.read_line() {
                        all_output.push_str(&line);
                        all_output.push('\n');
                        if line.contains(marker) {
                            return Ok(all_output);
                        }
                    }
                }
                Ok(false) => break,
                Err(e) => return Err(e),
            }
        }
        Err(SandboxError::Timeout(timeout))
    }

    /// Execute one VM step: run vCPU, handle exit. Returns true if should continue.
    /// Update virtio interrupt line via GIC SPI (level-sensitive).
    fn update_virtio_irq(&mut self) {
        let pending = self.virtio.irq_pending();
        let _ = self.vm.set_spi(machine::VIRTIO_IRQ, pending);
    }

    /// Update virtio-fs interrupt line via GIC SPI (level-sensitive).
    fn update_virtio_fs_irq(&mut self) {
        let pending = self
            .virtio_fs
            .as_ref()
            .is_some_and(|vfs| vfs.irq_pending());
        let _ = self.vm.set_spi(machine::VIRTIO_FS_IRQ, pending);
    }

    fn step_once(&mut self) -> Result<bool> {
        // Update GIC SPI lines before running (level-sensitive).
        // Don't use set_irq_pending — let the GIC handle delivery.
        self.update_uart_irq();
        self.update_virtio_irq();
        match self.vcpu.run()? {
            VcpuExit::MmioWrite { addr, len: _, srt } => {
                let data = if let Some(r) = reg_from_index(srt) {
                    self.vcpu.get_reg(r).unwrap_or(0)
                } else {
                    0 // XZR
                };
                if (machine::VIRTIO_MMIO_BASE
                    ..machine::VIRTIO_MMIO_BASE + machine::VIRTIO_MMIO_SIZE)
                    .contains(&addr)
                {
                    let offset = addr - machine::VIRTIO_MMIO_BASE;
                    self.virtio.handle_mmio_write(offset, data);
                    if offset == 0x050 {
                        self.virtio
                            .process_notify(data as u32, self.mem_ptr, self.mem_size);
                        // Deliver ctrl RX responses (multiport handshake)
                        self.virtio.deliver_ctrl_rx(self.mem_ptr, self.mem_size);
                    }
                    self.update_virtio_irq();
                } else if (machine::VIRTIO_FS_MMIO_BASE
                    ..machine::VIRTIO_FS_MMIO_BASE + machine::VIRTIO_FS_MMIO_SIZE)
                    .contains(&addr)
                {
                    if let Some(ref mut vfs) = self.virtio_fs {
                        let offset = addr - machine::VIRTIO_FS_MMIO_BASE;
                        vfs.handle_mmio_write(offset, data);
                        if offset == 0x050 {
                            // QUEUE_NOTIFY
                            vfs.process_notify(data as u32, self.mem_ptr, self.mem_size);
                        }
                    }
                    self.update_virtio_fs_irq();
                } else if (uart::PL011_BASE..uart::PL011_BASE + uart::PL011_SIZE).contains(&addr) {
                    self.uart.handle_mmio_write(addr - uart::PL011_BASE, data);
                    self.update_uart_irq();
                }
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::MmioRead { addr, len: _, reg } => {
                let val = if (machine::VIRTIO_MMIO_BASE
                    ..machine::VIRTIO_MMIO_BASE + machine::VIRTIO_MMIO_SIZE)
                    .contains(&addr)
                {
                    let v = self
                        .virtio
                        .handle_mmio_read(addr - machine::VIRTIO_MMIO_BASE);
                    self.update_virtio_irq();
                    v
                } else if (machine::VIRTIO_FS_MMIO_BASE
                    ..machine::VIRTIO_FS_MMIO_BASE + machine::VIRTIO_FS_MMIO_SIZE)
                    .contains(&addr)
                {
                    if let Some(ref vfs) = self.virtio_fs {
                        vfs.handle_mmio_read(addr - machine::VIRTIO_FS_MMIO_BASE)
                    } else {
                        0
                    }
                } else if (uart::PL011_BASE..uart::PL011_BASE + uart::PL011_SIZE).contains(&addr) {
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
                // HVF auto-masks vtimer after this exit. Unmask for continued timer operation.
                self.vcpu.set_vtimer_mask(false);
                Ok(true)
            }
            VcpuExit::WaitForEvent => {
                // Do NOT advance PC past WFI — let the CPU re-execute it.
                // If an interrupt is pending, WFI completes immediately.
                Ok(true)
            }
            VcpuExit::HypervisorCall { .. } => {
                self.handle_psci()?;
                Ok(true)
            }
            VcpuExit::SystemRegAccess { .. } => {
                // Trapped MSR/MRS — return 0 for reads, ignore writes, advance PC
                self.advance_pc()?;
                Ok(true)
            }
            VcpuExit::Unknown(0) => Ok(true), // CANCELED — retry
            VcpuExit::Unknown(code) => {
                let pc = self.vcpu.get_reg(Reg::Pc).unwrap_or(0);
                log::debug!("Unknown VM exit: code={code:#x} PC={pc:#x}");
                Ok(false)
            }
        }
    }

    /// Update PL011 interrupt line via GIC SPI.
    fn update_uart_irq(&mut self) {
        let intid = uart::PL011_IRQ;
        let pending = self.uart.irq_pending();
        let _ = self.vm.set_spi(intid, pending);
        self.vcpu.set_irq_pending(pending);
    }

    fn handle_mmio_read(&self, addr: u64) -> u64 {
        machine::handle_gic_redist_read(addr).unwrap_or(0)
    }

    /// Handle HVC exit — implements minimal PSCI for kernel boot.
    /// PC is already past the HVC instruction (ARM64 convention).
    fn handle_psci(&mut self) -> Result<()> {
        let x0 = self.vcpu.get_reg(Reg::X0).unwrap_or(0);

        const PSCI_VERSION: u64 = 0x84000000;
        const PSCI_FEATURES: u64 = 0x8400000A;
        const PSCI_SYSTEM_OFF: u64 = 0x84000008;
        const PSCI_SYSTEM_RESET: u64 = 0x84000009;
        const PSCI_CPU_ON_64: u64 = 0xC4000003;
        const PSCI_CPU_SUSPEND_64: u64 = 0xC4000001;
        const PSCI_RET_NOT_SUPPORTED: u64 = (-1i64) as u64;

        match x0 {
            PSCI_VERSION => {
                self.vcpu.set_reg(Reg::X0, 0x00010000)?; // PSCI 1.0
            }
            PSCI_FEATURES | PSCI_CPU_ON_64 | PSCI_CPU_SUSPEND_64 => {
                self.vcpu.set_reg(Reg::X0, PSCI_RET_NOT_SUPPORTED)?;
            }
            PSCI_SYSTEM_OFF | PSCI_SYSTEM_RESET => {
                log::info!("PSCI system off/reset requested");
                self.vcpu.set_reg(Reg::X0, 0)?;
            }
            _ => {
                self.vcpu.set_reg(Reg::X0, PSCI_RET_NOT_SUPPORTED)?;
            }
        }
        // Do NOT advance PC — ARM64 HVC sets ELR_EL2 = HVC+4 already
        Ok(())
    }

    /// Advance PC by 4 bytes (ARM64 fixed-width instructions).
    fn advance_pc(&mut self) -> Result<()> {
        let pc = self.vcpu.get_reg(Reg::Pc)?;
        self.vcpu.set_reg(Reg::Pc, pc + 4)
    }
}

impl<V: Vm> Drop for BootedVm<V> {
    fn drop(&mut self) {
        if !self.mem_ptr.is_null() {
            unsafe {
                libc::munmap(self.mem_ptr as *mut libc::c_void, self.mem_size);
            }
            self.mem_ptr = std::ptr::null_mut();
        }
    }
}
