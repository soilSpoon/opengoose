use crate::error::{SandboxError, Result};
use crate::hypervisor::*;
use crate::machine;
use crate::uart::{self, Pl011};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use crate::hypervisor::hvf::HvfHypervisor;

// Placeholder URL — replace with our own ARM64 kernel build later
const KERNEL_URL: &str =
    "https://github.com/nicholasgasior/zeroboot/releases/download/v0.1.0/vmlinux";

/// Ensure a kernel Image exists locally. Download if missing.
pub fn ensure_kernel() -> Result<PathBuf> {
    let cache_dir = kernel_cache_dir()?;
    let kernel_path = cache_dir.join("Image");

    if kernel_path.exists() {
        return Ok(kernel_path);
    }

    log::info!("Downloading kernel to {}...", kernel_path.display());

    let status = std::process::Command::new("curl")
        .args(["-fSL", "--create-dirs", "-o"])
        .arg(&kernel_path)
        .arg(KERNEL_URL)
        .status()
        .map_err(|e| SandboxError::Boot(format!("curl not found: {e}")))?;

    if !status.success() {
        return Err(SandboxError::Boot(format!(
            "kernel download failed (exit {}). URL: {KERNEL_URL}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(kernel_path)
}

fn kernel_cache_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| SandboxError::Boot("HOME not set".into()))?;
    let dir = PathBuf::from(home)
        .join(".opengoose")
        .join("kernel")
        .join("aarch64");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A booted VM with UART, ready for snapshot or direct interaction.
pub struct BootedVm<V: Vm> {
    pub vm: V,
    pub vcpu: V::Vcpu,
    pub uart: Pl011,
    pub mem_ptr: *mut u8,
    pub mem_size: usize,
}

// Safety: BootedVm is Send as long as V: Vm (which is Send) and the raw pointer
// is exclusively owned by this struct.
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

    // 4. Create and place DTB after kernel
    let dtb_gpa = machine::dtb_addr(kernel_end);
    let dtb_offset = (dtb_gpa - machine::RAM_BASE) as usize;
    let dtb_bytes = match machine::create_dtb(ram_size as u64) {
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
        std::ptr::copy_nonoverlapping(
            dtb_bytes.as_ptr(),
            mem_ptr.add(dtb_offset),
            dtb_bytes.len(),
        );
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
        | (1 << 9);  // Debug mask
    try_vm!(vcpu.set_reg(Reg::Cpsr, pstate));

    Ok(BootedVm {
        vm,
        vcpu,
        uart: Pl011::new(),
        mem_ptr,
        mem_size: ram_size,
    })
}

/// Load kernel Image into guest memory at RAM_BASE.
/// Returns the guest physical address of the kernel end.
fn load_kernel(kernel_path: &Path, mem_ptr: *mut u8, ram_size: usize) -> Result<u64> {
    let kernel_data = std::fs::read(kernel_path)
        .map_err(|e| SandboxError::Boot(format!("read kernel: {e}")))?;

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

    Ok(machine::RAM_BASE + kernel_data.len() as u64)
}

/// Convert a register index (0-30) to Reg enum safely.
fn reg_from_index(idx: u8) -> Option<Reg> {
    match idx {
        0 => Some(Reg::X0),
        1 => Some(Reg::X1),
        2 => Some(Reg::X2),
        3 => Some(Reg::X3),
        4 => Some(Reg::X4),
        5 => Some(Reg::X5),
        6 => Some(Reg::X6),
        7 => Some(Reg::X7),
        8 => Some(Reg::X8),
        9 => Some(Reg::X9),
        10 => Some(Reg::X10),
        11 => Some(Reg::X11),
        12 => Some(Reg::X12),
        13 => Some(Reg::X13),
        14 => Some(Reg::X14),
        15 => Some(Reg::X15),
        16 => Some(Reg::X16),
        17 => Some(Reg::X17),
        18 => Some(Reg::X18),
        19 => Some(Reg::X19),
        20 => Some(Reg::X20),
        21 => Some(Reg::X21),
        22 => Some(Reg::X22),
        23 => Some(Reg::X23),
        24 => Some(Reg::X24),
        25 => Some(Reg::X25),
        26 => Some(Reg::X26),
        27 => Some(Reg::X27),
        28 => Some(Reg::X28),
        29 => Some(Reg::X29),
        30 => Some(Reg::X30),
        _ => None, // 31 = XZR (zero register)
    }
}

impl<V: Vm> BootedVm<V> {
    /// Run the VM processing UART MMIO exits until timeout.
    /// Returns accumulated UART output.
    pub fn collect_uart_output(&mut self, timeout: Duration) -> String {
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

    /// Run until a specific marker string appears in UART output.
    pub fn run_until_marker(&mut self, marker: &str, timeout: Duration) -> Result<String> {
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
    fn step_once(&mut self) -> Result<bool> {
        match self.vcpu.run()? {
            VcpuExit::MmioWrite { addr, len: _, srt } => {
                let data = if let Some(r) = reg_from_index(srt) {
                    self.vcpu.get_reg(r).unwrap_or(0)
                } else {
                    0 // XZR
                };
                if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                    self.uart.handle_mmio_write(addr - uart::PL011_BASE, data);
                }
                Ok(true)
            }
            VcpuExit::MmioRead { addr, len: _, reg } => {
                if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                    let val = self.uart.handle_mmio_read(addr - uart::PL011_BASE);
                    if let Some(r) = reg_from_index(reg) {
                        let _ = self.vcpu.set_reg(r, val);
                    }
                } else if let Some(r) = reg_from_index(reg) {
                    // Unknown MMIO read — return 0
                    let _ = self.vcpu.set_reg(r, 0);
                }
                Ok(true)
            }
            VcpuExit::VtimerActivated => {
                // Timer fired — continue
                Ok(true)
            }
            VcpuExit::SystemEvent => {
                // WFI/HVC — advance PC by 4 (ARM64 fixed-width) and continue
                if let Ok(pc) = self.vcpu.get_reg(Reg::Pc) {
                    let _ = self.vcpu.set_reg(Reg::Pc, pc + 4);
                }
                Ok(true)
            }
            VcpuExit::Unknown(_) => Ok(false),
        }
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
