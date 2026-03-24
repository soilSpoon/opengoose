# opengoose-sandbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust crate providing sub-millisecond microVM sandboxing via CoW snapshot fork on Apple Hypervisor.framework (macOS ARM64), extensible to Linux KVM.

**Architecture:** Minimal VMM with Hypervisor trait abstracting HVF/KVM. Boots a Linux guest using libkrunfw kernel, PL011 UART for host↔guest communication, GICv3 for interrupts. First use auto-generates a snapshot; subsequent forks reuse it via `mmap(MAP_PRIVATE)` for CoW memory sharing.

**Tech Stack:** Rust (edition 2024), Apple Hypervisor.framework FFI, libkrunfw (kernel), vm-fdt (DTB), vm-memory (guest memory), serde+bincode (snapshots)

**Spec:** `docs/superpowers/specs/2026-03-24-opengoose-sandbox-design.md`

---

## Background & Key Findings

### libkrunfw — kernel provider
- Shared library (`.dylib` on macOS) exporting `krunfw_get_kernel(load_addr, entry_addr, size) -> *char`
- Returns raw kernel bytes already in memory (from `.data` section of the dylib)
- ARM64: `Image` format, load_addr = entry_addr = `0x8000_0000` (but we remap to our RAM base)
- **No need for `linux-loader` crate** — bytes are pre-parsed, just memcpy into guest memory
- Install: `brew tap slp/krun && brew install libkrunfw`

### HVF constraints
- **One VM per process** — `hv_vm_create()` can only be called once
- **vCPU thread affinity** — all vCPU ops must happen on the thread that called `hv_vcpu_create()`
- **GIC APIs require macOS 15.0+** (Sequoia)
- `hv_vcpu_exit_t` pointer is returned by `hv_vcpu_create()`, reused across `hv_vcpu_run()` calls

### ARM64 boot protocol
- PC = kernel entry, X0 = DTB address, X1/X2/X3 = 0
- PSTATE = EL1h with DAIF masked (interrupts disabled initially)
- Kernel at RAM_BASE + text_offset (usually 0 for kernels ≥ 5.7)
- DTB after kernel, page-aligned

### Memory map (ARM64 virt machine)
```
0x0800_0000  GICv3 Distributor (64 KiB)
0x080A_0000  GICv3 Redistributor (per-CPU)
0x0900_0000  PL011 UART (4 KiB)
0x4000_0000  RAM start (guest memory)
```

---

## File Structure

```
crates/opengoose-sandbox/
├── Cargo.toml
├── build.rs                        -- macOS: link Hypervisor.framework + libkrunfw
├── src/
│   ├── lib.rs                      -- pub API: Sandbox, SandboxPool
│   ├── error.rs                    -- SandboxError enum
│   ├── hypervisor/
│   │   ├── mod.rs                  -- Hypervisor/Vm/Vcpu traits, VcpuState, VcpuExit
│   │   └── hvf.rs                  -- HvfHypervisor + FFI bindings (cfg macos)
│   ├── machine.rs                  -- ARM64 memory map constants, DTB generation
│   ├── initramfs.rs                -- Build cpio initramfs from guest init binary
│   ├── uart.rs                     -- PL011 UART emulation
│   ├── boot.rs                     -- libkrunfw loading, initramfs, VM boot sequence
│   ├── snapshot.rs                 -- VmSnapshot create/save/load/cow_map
│   ├── vm.rs                       -- MicroVm: fork_from, exec, run loop
│   └── pool.rs                     -- SandboxPool: lazy init, acquire (Mutex-guarded)
├── guest/
│   └── init/
│       ├── Cargo.toml              -- standalone (NOT workspace member), musl target
│       ├── Cargo.lock
│       └── src/main.rs             -- PID 1: mount, serial listen, exec, respond
└── tests/
    ├── hvf_test.rs                 -- HVF VM create/destroy, register access
    ├── uart_test.rs                -- PL011 emulation unit tests
    ├── machine_test.rs             -- DTB generation validation
    ├── boot_test.rs                -- VM boot to "READY"
    ├── snapshot_test.rs            -- Snapshot save/load/CoW
    └── integration_test.rs         -- Full flow: boot → snapshot → fork → exec → result
```

Integration points in existing crates:
- Modify: `Cargo.toml` (workspace) — add member
- Modify: `crates/opengoose-rig/Cargo.toml` — add optional dependency
- Modify: `crates/opengoose-rig/src/mcp_tools.rs` — add `SandboxedClient` (later task)

---

## Task 1: Crate scaffold + error types

**Files:**
- Create: `crates/opengoose-sandbox/Cargo.toml`
- Create: `crates/opengoose-sandbox/build.rs`
- Create: `crates/opengoose-sandbox/src/lib.rs`
- Create: `crates/opengoose-sandbox/src/error.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Add workspace member**

In root `Cargo.toml`, add to `[workspace] members`:
```toml
members = [
    "crates/opengoose",
    "crates/opengoose-board",
    "crates/opengoose-rig",
    "crates/opengoose-skills",
    "crates/opengoose-sandbox",
]
```

- [ ] **Step 2: Create Cargo.toml**

```toml
[package]
name = "opengoose-sandbox"
description = "Sub-millisecond microVM sandboxing via CoW snapshot fork"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
libc = "0.2"
vm-fdt = "0.3"
serde = { version = "1", features = ["derive"] }
bincode = "1"
log = "0.4"
thiserror = "2"

[dev-dependencies]
serial_test = "3"
```

- [ ] **Step 3: Create build.rs**

```rust
fn main() {
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=Hypervisor");
}
```

- [ ] **Step 4: Create error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("hypervisor error: {0} (code {1})")]
    Hypervisor(String, i32),

    #[error("boot failed: {0}")]
    Boot(String),

    #[error("snapshot error: {0}")]
    Snapshot(String),

    #[error("uart error: {0}")]
    Uart(String),

    #[error("guest error: status={status}, stderr={stderr}")]
    Guest { status: i32, stderr: String },

    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SandboxError>;
```

- [ ] **Step 5: Create lib.rs**

```rust
pub mod error;
pub mod hypervisor;
pub mod machine;
pub mod uart;
pub mod boot;
pub mod snapshot;
pub mod vm;
pub mod pool;

pub use error::{SandboxError, Result};
pub use pool::SandboxPool;
pub use vm::MicroVm;
```

- [ ] **Step 6: Create stub modules**

Create empty files so the crate compiles:
- `src/hypervisor/mod.rs` — empty
- `src/hypervisor/hvf.rs` — empty
- `src/machine.rs` — empty
- `src/uart.rs` — empty
- `src/boot.rs` — empty
- `src/snapshot.rs` — empty
- `src/vm.rs` — empty
- `src/pool.rs` — empty

- [ ] **Step 7: Verify compilation**

Run: `cargo check -p opengoose-sandbox`
Expected: compiles with no errors (warnings OK for empty modules)

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose-sandbox/ Cargo.toml
git commit -m "feat(sandbox): crate scaffold with error types"
```

---

## Task 2: HVF FFI bindings + Hypervisor trait

**Files:**
- Create: `crates/opengoose-sandbox/src/hypervisor/mod.rs`
- Create: `crates/opengoose-sandbox/src/hypervisor/hvf.rs`
- Test: `crates/opengoose-sandbox/tests/hvf_test.rs`

### Step group A: Hypervisor trait

- [ ] **Step 1: Define traits and types in mod.rs**

```rust
use crate::error::Result;
use serde::{Serialize, Deserialize};

/// ARM64 general-purpose register IDs (maps to HV_REG_*)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum Reg {
    X0 = 0, X1 = 1, X2 = 2, X3 = 3, X4 = 4, X5 = 5, X6 = 6, X7 = 7,
    X8 = 8, X9 = 9, X10 = 10, X11 = 11, X12 = 12, X13 = 13, X14 = 14, X15 = 15,
    X16 = 16, X17 = 17, X18 = 18, X19 = 19, X20 = 20, X21 = 21, X22 = 22, X23 = 23,
    X24 = 24, X25 = 25, X26 = 26, X27 = 27, X28 = 28,
    X29 = 29, // FP
    X30 = 30, // LR
    Pc = 31,
    Fpcr = 32,
    Fpsr = 33,
    Cpsr = 34,
}

/// ARM64 system register IDs (maps to HV_SYS_REG_*)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum SysReg {
    SctlrEl1 = 0xc080,
    TtbrEl10 = 0xc100,
    TtbrEl11 = 0xc101,
    TcrEl1 = 0xc102,
    SpsrEl1 = 0xc200,
    ElrEl1 = 0xc201,
    SpEl0 = 0xc208,
    EsrEl1 = 0xc290,
    FarEl1 = 0xc300,
    MairEl1 = 0xc510,
    VbarEl1 = 0xc600,
    TpidrEl1 = 0xc684,
    TpidrEl0 = 0xde82,
    CntvCtlEl0 = 0xdf19,
    CntvCvalEl0 = 0xdf1a,
    SpEl1 = 0xe208,
    CpcrEl1 = 0xc082,
    CntKctlEl1 = 0xc708,
}

/// Bulk vCPU state for snapshot save/restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcpuState {
    pub regs: Vec<(Reg, u64)>,
    pub sys_regs: Vec<(SysReg, u64)>,
}

/// Decoded VM exit reason
pub enum VcpuExit {
    MmioRead { addr: u64, len: u8, reg: u8 },
    /// MMIO write exit. `srt` is the source register index (0-30, 31=XZR).
    /// Caller must read vcpu.get_reg(Xn) to get the actual data value.
    MmioWrite { addr: u64, len: u8, srt: u8 },
    VtimerActivated,
    SystemEvent,
    Unknown(u32),
}

/// GIC configuration for create_gic
pub struct GicConfig {
    pub dist_addr: u64,
    pub dist_size: u64,
    pub redist_addr: u64,
    pub redist_size: u64,
}

pub trait Hypervisor: Send + Sync {
    type Vm: Vm;
    fn create_vm(&self) -> Result<Self::Vm>;
}

pub trait Vm: Send {
    type Vcpu: Vcpu;
    fn map_memory(&mut self, gpa: u64, host_addr: *mut u8, size: usize) -> Result<()>;
    fn unmap_memory(&mut self, gpa: u64, size: usize) -> Result<()>;
    fn create_gic(&mut self, config: &GicConfig) -> Result<()>;
    fn create_vcpu(&mut self) -> Result<Self::Vcpu>;
}
// Note: Vm impls should implement Drop to clean up (hv_vm_destroy, etc.)

pub trait Vcpu: Send {
    fn get_reg(&self, reg: Reg) -> Result<u64>;
    fn set_reg(&mut self, reg: Reg, val: u64) -> Result<()>;
    fn get_sys_reg(&self, reg: SysReg) -> Result<u64>;
    fn set_sys_reg(&mut self, reg: SysReg, val: u64) -> Result<()>;
    fn get_all_regs(&self) -> Result<VcpuState>;
    fn set_all_regs(&mut self, state: &VcpuState) -> Result<()>;
    fn run(&mut self) -> Result<VcpuExit>;
}

#[cfg(target_os = "macos")]
pub mod hvf;
```

- [ ] **Step 2: Verify trait definitions compile**

Run: `cargo check -p opengoose-sandbox`

### Step group B: HVF FFI declarations

- [ ] **Step 3: Write FFI bindings in hvf.rs**

```rust
use std::ffi::c_void;
use crate::error::{SandboxError, Result};
use super::*;

// --- FFI type aliases ---
type HvReturn = i32;
type HvIpa = u64;
type HvMemoryFlags = u64;
type HvVcpuT = u64;
type HvRegT = u32;
type HvSysRegT = u16;
type HvExitReason = u32;

const HV_SUCCESS: HvReturn = 0;
const HV_MEMORY_READ: HvMemoryFlags = 1 << 0;
const HV_MEMORY_WRITE: HvMemoryFlags = 1 << 1;
const HV_MEMORY_EXEC: HvMemoryFlags = 1 << 2;

const HV_EXIT_REASON_CANCELED: HvExitReason = 0;
const HV_EXIT_REASON_EXCEPTION: HvExitReason = 1;
const HV_EXIT_REASON_VTIMER_ACTIVATED: HvExitReason = 2;
const HV_EXIT_REASON_UNKNOWN: HvExitReason = 3;

#[repr(C)]
struct HvVcpuExitException {
    syndrome: u64,
    virtual_address: u64,
    physical_address: u64,
}

#[repr(C)]
struct HvVcpuExit {
    reason: HvExitReason,
    exception: HvVcpuExitException,
}

extern "C" {
    // VM
    fn hv_vm_create(config: *mut c_void) -> HvReturn;
    fn hv_vm_destroy() -> HvReturn;
    fn hv_vm_map(addr: *mut c_void, ipa: HvIpa, size: usize, flags: HvMemoryFlags) -> HvReturn;
    fn hv_vm_unmap(ipa: HvIpa, size: usize) -> HvReturn;

    // vCPU
    fn hv_vcpu_create(vcpu: *mut HvVcpuT, exit: *mut *const HvVcpuExit, config: *mut c_void) -> HvReturn;
    fn hv_vcpu_destroy(vcpu: HvVcpuT) -> HvReturn;
    fn hv_vcpu_run(vcpu: HvVcpuT) -> HvReturn;

    // Registers
    fn hv_vcpu_get_reg(vcpu: HvVcpuT, reg: HvRegT, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_reg(vcpu: HvVcpuT, reg: HvRegT, value: u64) -> HvReturn;
    fn hv_vcpu_get_sys_reg(vcpu: HvVcpuT, reg: HvSysRegT, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_sys_reg(vcpu: HvVcpuT, reg: HvSysRegT, value: u64) -> HvReturn;

    // GIC (macOS 15.0+)
    fn hv_gic_config_create() -> *mut c_void;
    fn hv_gic_config_set_distributor_base(config: *mut c_void, addr: HvIpa) -> HvReturn;
    fn hv_gic_config_set_redistributor_base(config: *mut c_void, addr: HvIpa) -> HvReturn;
    fn hv_gic_create(config: *mut c_void) -> HvReturn;
    fn hv_gic_reset() -> HvReturn;
    fn hv_gic_set_spi(intid: u32, level: bool) -> HvReturn;

    // GIC state save/restore
    fn hv_gic_state_create() -> *mut c_void;
    fn hv_gic_state_get_size(state: *mut c_void, size: *mut usize) -> HvReturn;
    fn hv_gic_state_get_data(state: *mut c_void, data: *mut c_void) -> HvReturn;
    fn hv_gic_set_state(data: *const c_void, size: usize) -> HvReturn;
}

/// Check HVF return code, convert to Result
fn check(ret: HvReturn, op: &str) -> Result<()> {
    if ret == HV_SUCCESS {
        Ok(())
    } else {
        Err(SandboxError::Hypervisor(op.to_string(), ret))
    }
}

/// Decode ESR_EL2 data abort syndrome into VcpuExit
fn decode_exit(exit: &HvVcpuExit) -> VcpuExit {
    match exit.reason {
        HV_EXIT_REASON_EXCEPTION => {
            let syndrome = exit.exception.syndrome;
            let ec = (syndrome >> 26) & 0x3f;

            match ec {
                // Data Abort from lower EL
                0x24 => {
                    let isv = (syndrome >> 24) & 1;
                    if isv == 0 {
                        return VcpuExit::Unknown(ec as u32);
                    }
                    let sas = (syndrome >> 22) & 3;
                    let srt = ((syndrome >> 16) & 0x1f) as u8;
                    let wnr = (syndrome >> 6) & 1;
                    let len = 1u8 << sas;
                    let addr = exit.exception.physical_address;

                    if wnr == 1 {
                        VcpuExit::MmioWrite { addr, len, srt }
                    } else {
                        VcpuExit::MmioRead { addr, len, reg: srt }
                    }
                }
                // WFI/WFE trap
                0x01 => VcpuExit::SystemEvent,
                // HVC
                0x16 => VcpuExit::SystemEvent,
                _ => VcpuExit::Unknown(ec as u32),
            }
        }
        HV_EXIT_REASON_VTIMER_ACTIVATED => VcpuExit::VtimerActivated,
        HV_EXIT_REASON_CANCELED => VcpuExit::Unknown(0),
        _ => VcpuExit::Unknown(exit.reason),
    }
}
```

- [ ] **Step 4: Implement HvfHypervisor, HvfVm, HvfVcpu**

Continue in `hvf.rs`:

```rust
pub struct HvfHypervisor;

impl Hypervisor for HvfHypervisor {
    type Vm = HvfVm;

    fn create_vm(&self) -> Result<HvfVm> {
        unsafe { check(hv_vm_create(std::ptr::null_mut()), "hv_vm_create")? };
        Ok(HvfVm { created: true })
    }
}

pub struct HvfVm {
    created: bool,
}

impl Vm for HvfVm {
    type Vcpu = HvfVcpu;

    fn map_memory(&mut self, gpa: u64, host_addr: *mut u8, size: usize) -> Result<()> {
        let flags = HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC;
        unsafe { check(hv_vm_map(host_addr as *mut c_void, gpa, size, flags), "hv_vm_map") }
    }

    fn unmap_memory(&mut self, gpa: u64, size: usize) -> Result<()> {
        unsafe { check(hv_vm_unmap(gpa, size), "hv_vm_unmap") }
    }

    fn create_gic(&mut self, config: &GicConfig) -> Result<()> {
        unsafe {
            let gic_config = hv_gic_config_create();
            check(
                hv_gic_config_set_distributor_base(gic_config, config.dist_addr),
                "gic_set_dist_base",
            )?;
            check(
                hv_gic_config_set_redistributor_base(gic_config, config.redist_addr),
                "gic_set_redist_base",
            )?;
            check(hv_gic_create(gic_config), "hv_gic_create")
        }
    }

    fn create_vcpu(&mut self) -> Result<HvfVcpu> {
        let mut vcpu_id: HvVcpuT = 0;
        let mut exit_ptr: *const HvVcpuExit = std::ptr::null();
        unsafe {
            check(
                hv_vcpu_create(&mut vcpu_id, &mut exit_ptr, std::ptr::null_mut()),
                "hv_vcpu_create",
            )?;
        }
        Ok(HvfVcpu { id: vcpu_id, exit_ptr })
    }

    // destroy is handled by Drop impl
}

pub struct HvfVcpu {
    id: HvVcpuT,
    exit_ptr: *const HvVcpuExit,
}

// Safety: HvfVcpu is Send because we enforce single-thread usage via the trait contract.
// The caller must ensure all Vcpu methods are called from the thread that created it.
unsafe impl Send for HvfVcpu {}

impl Vcpu for HvfVcpu {
    fn get_reg(&self, reg: Reg) -> Result<u64> {
        let mut val: u64 = 0;
        unsafe { check(hv_vcpu_get_reg(self.id, reg as HvRegT, &mut val), "get_reg")? };
        Ok(val)
    }

    fn set_reg(&mut self, reg: Reg, val: u64) -> Result<()> {
        unsafe { check(hv_vcpu_set_reg(self.id, reg as HvRegT, val), "set_reg") }
    }

    fn get_sys_reg(&self, reg: SysReg) -> Result<u64> {
        let mut val: u64 = 0;
        unsafe { check(hv_vcpu_get_sys_reg(self.id, reg as HvSysRegT, &mut val), "get_sys_reg")? };
        Ok(val)
    }

    fn set_sys_reg(&mut self, reg: SysReg, val: u64) -> Result<()> {
        unsafe { check(hv_vcpu_set_sys_reg(self.id, reg as HvSysRegT, val), "set_sys_reg") }
    }

    fn get_all_regs(&self) -> Result<VcpuState> {
        let general_regs = [
            Reg::X0, Reg::X1, Reg::X2, Reg::X3, Reg::X4, Reg::X5, Reg::X6, Reg::X7,
            Reg::X8, Reg::X9, Reg::X10, Reg::X11, Reg::X12, Reg::X13, Reg::X14, Reg::X15,
            Reg::X16, Reg::X17, Reg::X18, Reg::X19, Reg::X20, Reg::X21, Reg::X22, Reg::X23,
            Reg::X24, Reg::X25, Reg::X26, Reg::X27, Reg::X28, Reg::X29, Reg::X30,
            Reg::Pc, Reg::Cpsr,
        ];
        let sys_regs_list = [
            SysReg::SctlrEl1, SysReg::TtbrEl10, SysReg::TtbrEl11, SysReg::TcrEl1,
            SysReg::SpsrEl1, SysReg::ElrEl1, SysReg::SpEl0, SysReg::SpEl1,
            SysReg::EsrEl1, SysReg::FarEl1, SysReg::MairEl1, SysReg::VbarEl1,
            SysReg::TpidrEl1, SysReg::TpidrEl0, SysReg::CntvCtlEl0, SysReg::CntvCvalEl0,
            SysReg::CpcrEl1, SysReg::CntKctlEl1,
        ];

        let mut regs = Vec::with_capacity(general_regs.len());
        for r in general_regs {
            regs.push((r, self.get_reg(r)?));
        }
        let mut sys = Vec::with_capacity(sys_regs_list.len());
        for r in sys_regs_list {
            sys.push((r, self.get_sys_reg(r)?));
        }
        Ok(VcpuState { regs, sys_regs: sys })
    }

    fn set_all_regs(&mut self, state: &VcpuState) -> Result<()> {
        for &(reg, val) in &state.regs {
            self.set_reg(reg, val)?;
        }
        for &(reg, val) in &state.sys_regs {
            self.set_sys_reg(reg, val)?;
        }
        Ok(())
    }

    fn run(&mut self) -> Result<VcpuExit> {
        unsafe {
            check(hv_vcpu_run(self.id), "hv_vcpu_run")?;
            Ok(decode_exit(&*self.exit_ptr))
        }
    }
}

impl Drop for HvfVcpu {
    fn drop(&mut self) {
        unsafe { let _ = hv_vcpu_destroy(self.id); }
    }
}

impl Drop for HvfVm {
    fn drop(&mut self) {
        if self.created {
            unsafe { let _ = hv_vm_destroy(); }
            self.created = false;
        }
    }
}
```

- [ ] **Step 5: Write HVF integration test**

Create `tests/hvf_test.rs`:

```rust
use opengoose_sandbox::hypervisor::*;
#[cfg(target_os = "macos")]
use opengoose_sandbox::hypervisor::hvf::HvfHypervisor;
use serial_test::serial;

#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_vm_create_destroy() {
    let hv = HvfHypervisor;
    let vm = hv.create_vm().expect("create VM");
    drop(vm); // triggers hv_vm_destroy via Drop
}

#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_vcpu_register_roundtrip() {
    let hv = HvfHypervisor;
    let mut vm = hv.create_vm().expect("create VM");

    // Allocate 4 KiB of memory so VM has something
    let page_size = 4096usize;
    let mem = unsafe {
        libc::mmap(
            std::ptr::null_mut(), page_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_ANON | libc::MAP_PRIVATE, -1, 0,
        )
    };
    assert_ne!(mem, libc::MAP_FAILED);
    vm.map_memory(0x4000_0000, mem as *mut u8, page_size).expect("map");

    let mut vcpu = vm.create_vcpu().expect("create vcpu");

    // Write and read back PC
    vcpu.set_reg(Reg::Pc, 0x4000_0000).expect("set PC");
    let pc = vcpu.get_reg(Reg::Pc).expect("get PC");
    assert_eq!(pc, 0x4000_0000);

    // Write and read back X0
    vcpu.set_reg(Reg::X0, 0xDEAD_BEEF).expect("set X0");
    let x0 = vcpu.get_reg(Reg::X0).expect("get X0");
    assert_eq!(x0, 0xDEAD_BEEF);

    drop(vcpu);
    unsafe { libc::munmap(mem, page_size); }
    drop(vm);
}

#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_get_all_set_all_regs() {
    let hv = HvfHypervisor;
    let mut vm = hv.create_vm().expect("create VM");

    let page_size = 4096usize;
    let mem = unsafe {
        libc::mmap(
            std::ptr::null_mut(), page_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_ANON | libc::MAP_PRIVATE, -1, 0,
        )
    };
    assert_ne!(mem, libc::MAP_FAILED);
    vm.map_memory(0x4000_0000, mem as *mut u8, page_size).expect("map");

    let mut vcpu = vm.create_vcpu().expect("create vcpu");
    let state = vcpu.get_all_regs().expect("get_all_regs");

    // State should contain PC and CPSR at minimum
    assert!(state.regs.iter().any(|(r, _)| *r == Reg::Pc));
    assert!(state.regs.iter().any(|(r, _)| *r == Reg::Cpsr));
    assert!(!state.sys_regs.is_empty());

    // Roundtrip: set_all should not error
    vcpu.set_all_regs(&state).expect("set_all_regs");

    drop(vcpu);
    unsafe { libc::munmap(mem, page_size); }
    drop(vm);
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p opengoose-sandbox --test hvf_test -- --test-threads=1`
Expected: all 3 tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose-sandbox/src/hypervisor/ crates/opengoose-sandbox/tests/hvf_test.rs
git commit -m "feat(sandbox): Hypervisor trait + HVF backend with FFI bindings"
```

---

## Task 3: PL011 UART emulation

**Files:**
- Create: `crates/opengoose-sandbox/src/uart.rs`
- Test: `crates/opengoose-sandbox/tests/uart_test.rs`

This is pure logic with no HVF dependency — fully unit-testable.

- [ ] **Step 1: Write failing tests**

Create `tests/uart_test.rs`:

```rust
use opengoose_sandbox::uart::Pl011;

#[test]
fn test_write_and_read_output() {
    let mut uart = Pl011::new();
    // Guest writes bytes to UARTDR
    uart.handle_mmio_write(0x000, b'H' as u64);
    uart.handle_mmio_write(0x000, b'i' as u64);

    assert_eq!(uart.take_output(), b"Hi");
    assert_eq!(uart.take_output(), b""); // drained
}

#[test]
fn test_input_and_read() {
    let mut uart = Pl011::new();
    uart.push_input(b"OK\n");

    // Guest reads UARTDR
    assert_eq!(uart.handle_mmio_read(0x000) as u8, b'O');
    assert_eq!(uart.handle_mmio_read(0x000) as u8, b'K');
    assert_eq!(uart.handle_mmio_read(0x000) as u8, b'\n');
}

#[test]
fn test_flag_register() {
    let mut uart = Pl011::new();

    // No input: RXFE (bit 4) set, TXFF (bit 5) clear
    let fr = uart.handle_mmio_read(0x018);
    assert_ne!(fr & (1 << 4), 0, "RXFE should be set when empty");
    assert_eq!(fr & (1 << 5), 0, "TXFF should be clear (can always write)");

    // Push input: RXFE should clear
    uart.push_input(b"x");
    let fr = uart.handle_mmio_read(0x018);
    assert_eq!(fr & (1 << 4), 0, "RXFE should be clear when data available");
}

#[test]
fn test_read_line() {
    let mut uart = Pl011::new();
    uart.push_input(b"hello\nworld\n");

    assert_eq!(uart.read_line(), Some("hello".to_string()));
    assert_eq!(uart.read_line(), Some("world".to_string()));
    assert_eq!(uart.read_line(), None);
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test -p opengoose-sandbox --test uart_test`
Expected: FAIL (Pl011 not defined)

- [ ] **Step 3: Implement PL011 UART**

```rust
/// Minimal PL011 UART emulation for host↔guest serial communication.
///
/// MMIO register offsets:
///   0x000 UARTDR   — data register (read/write)
///   0x018 UARTFR   — flag register (read-only)
///   0x038 UARTIMSC — interrupt mask (write, stored but not acted on)
///   0x044 UARTICR  — interrupt clear (write, no-op)

use std::collections::VecDeque;

/// MMIO base address for PL011 in our memory map
pub const PL011_BASE: u64 = 0x0900_0000;
/// MMIO region size
pub const PL011_SIZE: u64 = 0x1000;
/// SPI interrupt number (GIC SPI 1 = IRQ 33)
pub const PL011_IRQ: u32 = 1;

// Register offsets
const UARTDR: u64 = 0x000;
const UARTFR: u64 = 0x018;
const UARTIMSC: u64 = 0x038;
const UARTICR: u64 = 0x044;

// Flag bits
const FR_RXFE: u64 = 1 << 4; // RX FIFO empty
const FR_TXFE: u64 = 1 << 7; // TX FIFO empty (always set — we consume instantly)

pub struct Pl011 {
    input: VecDeque<u8>,
    output: Vec<u8>,
    output_line_buf: Vec<u8>,
    imsc: u64,
}

impl Pl011 {
    pub fn new() -> Self {
        Pl011 {
            input: VecDeque::new(),
            output: Vec::new(),
            output_line_buf: Vec::new(),
            imsc: 0,
        }
    }

    /// Guest writes to UART MMIO. `offset` is relative to PL011_BASE.
    pub fn handle_mmio_write(&mut self, offset: u64, val: u64) {
        match offset {
            UARTDR => {
                let byte = val as u8;
                self.output.push(byte);
                self.output_line_buf.push(byte);
            }
            UARTIMSC => self.imsc = val,
            UARTICR => {} // clear interrupt — no-op for us
            _ => {} // ignore unknown registers
        }
    }

    /// Guest reads from UART MMIO. Returns register value.
    pub fn handle_mmio_read(&mut self, offset: u64) -> u64 {
        match offset {
            UARTDR => self.input.pop_front().map(|b| b as u64).unwrap_or(0),
            UARTFR => {
                let mut flags = FR_TXFE; // TX always empty (infinite sink)
                if self.input.is_empty() {
                    flags |= FR_RXFE;
                }
                flags
            }
            UARTIMSC => self.imsc,
            _ => 0,
        }
    }

    /// Push data into the UART input buffer (host → guest).
    pub fn push_input(&mut self, data: &[u8]) {
        self.input.extend(data);
    }

    /// Take accumulated output (guest → host), draining the buffer.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// Try to read a complete line from the output buffer.
    /// Returns None if no complete line is available yet.
    pub fn read_line(&mut self) -> Option<String> {
        if let Some(pos) = self.output_line_buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.output_line_buf.drain(..=pos).collect();
            // Strip trailing newline
            let s = String::from_utf8_lossy(&line[..line.len() - 1]).to_string();
            Some(s)
        } else {
            None
        }
    }

    /// Check if the UART has pending input (for interrupt injection).
    pub fn has_pending_input(&self) -> bool {
        !self.input.is_empty()
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p opengoose-sandbox --test uart_test`
Expected: all 4 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-sandbox/src/uart.rs crates/opengoose-sandbox/tests/uart_test.rs
git commit -m "feat(sandbox): PL011 UART emulation"
```

---

## Task 4: Machine definition — memory map + DTB generation

**Files:**
- Create: `crates/opengoose-sandbox/src/machine.rs`
- Test: `crates/opengoose-sandbox/tests/machine_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/machine_test.rs`:

```rust
use opengoose_sandbox::machine;

#[test]
fn test_memory_map_constants() {
    // GIC must be below RAM
    assert!(machine::GIC_DIST_ADDR < machine::RAM_BASE);
    // UART must be below RAM
    assert!(machine::UART_ADDR < machine::RAM_BASE);
    // RAM must be page-aligned
    assert_eq!(machine::RAM_BASE % 4096, 0);
}

#[test]
fn test_create_dtb() {
    let dtb = machine::create_dtb(128 * 1024 * 1024).expect("create DTB");
    // DTB magic number: 0xD00DFEED (big-endian)
    assert_eq!(&dtb[0..4], &[0xD0, 0x0D, 0xFE, 0xED]);
    // Should be reasonably sized (< 64 KiB)
    assert!(dtb.len() < 65536);
    // Should be > 0
    assert!(dtb.len() > 100);
}

#[test]
fn test_dtb_addr_placement() {
    let ram_size: u64 = 128 * 1024 * 1024;
    let kernel_end: u64 = machine::RAM_BASE + 0x100_0000; // 16 MiB kernel
    let dtb_addr = machine::dtb_addr(kernel_end);
    // Must be after kernel
    assert!(dtb_addr >= kernel_end);
    // Must be page-aligned
    assert_eq!(dtb_addr % 4096, 0);
    // Must be within RAM
    assert!(dtb_addr < machine::RAM_BASE + ram_size);
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test -p opengoose-sandbox --test machine_test`

- [ ] **Step 3: Implement machine.rs**

```rust
use crate::error::{SandboxError, Result};
use crate::uart;

// --- Memory map constants ---
pub const GIC_DIST_ADDR: u64 = 0x0800_0000;
pub const GIC_DIST_SIZE: u64 = 0x0001_0000; // 64 KiB
pub const GIC_REDIST_ADDR: u64 = 0x080A_0000;
pub const GIC_REDIST_SIZE: u64 = 0x00F6_0000; // ~1 MiB (per CPU)
pub const UART_ADDR: u64 = uart::PL011_BASE;
pub const UART_SIZE: u64 = uart::PL011_SIZE;
pub const RAM_BASE: u64 = 0x4000_0000; // 1 GiB
pub const DEFAULT_RAM_SIZE: u64 = 128 * 1024 * 1024; // 128 MiB

// DTB/FDT constants
const GIC_PHANDLE: u32 = 1;
const CLOCK_PHANDLE: u32 = 2;
const GIC_FDT_IRQ_TYPE_SPI: u32 = 0;
const GIC_FDT_IRQ_TYPE_PPI: u32 = 1;
const IRQ_TYPE_LEVEL_HI: u32 = 4;
const IRQ_TYPE_EDGE_RISING: u32 = 1;

// ARM timer IRQ IDs
const GTIMER_SEC: u32 = 13;
const GTIMER_HYP: u32 = 14;
const GTIMER_VIRT: u32 = 11;
const GTIMER_PHYS: u32 = 12;

/// Calculate DTB placement address (page-aligned, after kernel).
pub fn dtb_addr(kernel_end: u64) -> u64 {
    (kernel_end + 0xFFF) & !0xFFF
}

/// Helper: pack u64 values into big-endian bytes for FDT `reg` properties.
fn prop64(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn prop32(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

/// Create a minimal ARM64 Device Tree Blob for our VM.
pub fn create_dtb(ram_size: u64) -> Result<Vec<u8>> {
    use vm_fdt::FdtWriter;

    let mut fdt = FdtWriter::new().map_err(|e| SandboxError::Boot(format!("FDT: {e}")))?;
    let map_err = |e: vm_fdt::Error| SandboxError::Boot(format!("FDT: {e}"));

    // Root
    let root = fdt.begin_node("").map_err(map_err)?;
    fdt.property_string("compatible", "linux,dummy-virt").map_err(map_err)?;
    fdt.property_u32("#address-cells", 2).map_err(map_err)?;
    fdt.property_u32("#size-cells", 2).map_err(map_err)?;
    fdt.property_u32("interrupt-parent", GIC_PHANDLE).map_err(map_err)?;

    // CPU
    {
        let cpus = fdt.begin_node("cpus").map_err(map_err)?;
        fdt.property_u32("#address-cells", 2).map_err(map_err)?;
        fdt.property_u32("#size-cells", 0).map_err(map_err)?;
        let cpu = fdt.begin_node("cpu@0").map_err(map_err)?;
        fdt.property_string("device_type", "cpu").map_err(map_err)?;
        fdt.property_string("compatible", "arm,arm-v8").map_err(map_err)?;
        fdt.property_u64("reg", 0).map_err(map_err)?;
        fdt.end_node(cpu).map_err(map_err)?;
        fdt.end_node(cpus).map_err(map_err)?;
    }

    // Memory
    {
        let mem = fdt.begin_node(&format!("memory@{RAM_BASE:x}")).map_err(map_err)?;
        fdt.property_string("device_type", "memory").map_err(map_err)?;
        fdt.property("reg", &prop64(&[RAM_BASE, ram_size])).map_err(map_err)?;
        fdt.end_node(mem).map_err(map_err)?;
    }

    // GICv3
    {
        let intc = fdt.begin_node("intc").map_err(map_err)?;
        fdt.property_string("compatible", "arm,gic-v3").map_err(map_err)?;
        fdt.property_null("interrupt-controller").map_err(map_err)?;
        fdt.property_u32("#interrupt-cells", 3).map_err(map_err)?;
        fdt.property("reg", &prop64(&[
            GIC_DIST_ADDR, GIC_DIST_SIZE,
            GIC_REDIST_ADDR, GIC_REDIST_SIZE,
        ])).map_err(map_err)?;
        fdt.property_u32("phandle", GIC_PHANDLE).map_err(map_err)?;
        fdt.property_u32("#address-cells", 2).map_err(map_err)?;
        fdt.property_u32("#size-cells", 2).map_err(map_err)?;
        fdt.property_null("ranges").map_err(map_err)?;
        fdt.property("interrupts", &prop32(&[
            GIC_FDT_IRQ_TYPE_PPI, 9, IRQ_TYPE_LEVEL_HI,
        ])).map_err(map_err)?;
        fdt.end_node(intc).map_err(map_err)?;
    }

    // Timer
    {
        let timer = fdt.begin_node("timer").map_err(map_err)?;
        fdt.property_string("compatible", "arm,armv8-timer").map_err(map_err)?;
        fdt.property_null("always-on").map_err(map_err)?;
        fdt.property("interrupts", &prop32(&[
            GIC_FDT_IRQ_TYPE_PPI, GTIMER_SEC, IRQ_TYPE_LEVEL_HI,
            GIC_FDT_IRQ_TYPE_PPI, GTIMER_HYP, IRQ_TYPE_LEVEL_HI,
            GIC_FDT_IRQ_TYPE_PPI, GTIMER_VIRT, IRQ_TYPE_LEVEL_HI,
            GIC_FDT_IRQ_TYPE_PPI, GTIMER_PHYS, IRQ_TYPE_LEVEL_HI,
        ])).map_err(map_err)?;
        fdt.end_node(timer).map_err(map_err)?;
    }

    // PL011 UART
    {
        let uart = fdt.begin_node(&format!("uart@{UART_ADDR:x}")).map_err(map_err)?;
        fdt.property_string("compatible", "arm,pl011").map_err(map_err)?;
        fdt.property_string("status", "okay").map_err(map_err)?;
        fdt.property("reg", &prop64(&[UART_ADDR, UART_SIZE])).map_err(map_err)?;
        fdt.property("interrupts", &prop32(&[
            GIC_FDT_IRQ_TYPE_SPI, uart::PL011_IRQ, IRQ_TYPE_EDGE_RISING,
        ])).map_err(map_err)?;
        fdt.property_u32("clocks", CLOCK_PHANDLE).map_err(map_err)?;
        fdt.property_string("clock-names", "apb_pclk").map_err(map_err)?;
        fdt.end_node(uart).map_err(map_err)?;
    }

    // Clock (required by PL011)
    {
        let clk = fdt.begin_node("apb-pclk").map_err(map_err)?;
        fdt.property_string("compatible", "fixed-clock").map_err(map_err)?;
        fdt.property_u32("#clock-cells", 0).map_err(map_err)?;
        fdt.property_u32("clock-frequency", 24_000_000).map_err(map_err)?;
        fdt.property_u32("phandle", CLOCK_PHANDLE).map_err(map_err)?;
        fdt.end_node(clk).map_err(map_err)?;
    }

    // PSCI
    {
        let psci = fdt.begin_node("psci").map_err(map_err)?;
        fdt.property_string("compatible", "arm,psci-0.2").map_err(map_err)?;
        fdt.property_string("method", "hvc").map_err(map_err)?;
        fdt.end_node(psci).map_err(map_err)?;
    }

    // Chosen
    {
        let chosen = fdt.begin_node("chosen").map_err(map_err)?;
        fdt.property_string("bootargs", "console=ttyAMA0 earlycon=pl011,0x09000000 reboot=t panic=-1").map_err(map_err)?;
        fdt.property_string("stdout-path", &format!("/uart@{UART_ADDR:x}")).map_err(map_err)?;
        fdt.end_node(chosen).map_err(map_err)?;
    }

    fdt.end_node(root).map_err(map_err)?;
    fdt.finish().map_err(map_err)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p opengoose-sandbox --test machine_test`
Expected: all 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-sandbox/src/machine.rs crates/opengoose-sandbox/tests/machine_test.rs
git commit -m "feat(sandbox): ARM64 machine definition with DTB generation"
```

---

## Task 5: Boot — libkrunfw kernel loading + VM boot

**Files:**
- Create: `crates/opengoose-sandbox/src/boot.rs`
- Test: `crates/opengoose-sandbox/tests/boot_test.rs`

This task boots a Linux guest to the point where the kernel prints to the UART. It does NOT yet require a guest init binary — kernel boot messages on the serial console are sufficient to prove the VMM works.

- [ ] **Step 1: Write failing test**

Create `tests/boot_test.rs`:

```rust
use opengoose_sandbox::boot::BootedVm;
use serial_test::serial;

#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_boot_prints_to_uart() {
    // Boot a VM and collect UART output for up to 5 seconds.
    // The kernel should print *something* to the console.
    let mut vm = BootedVm::boot_default().expect("boot VM");
    let output = vm.collect_uart_output(std::time::Duration::from_secs(5));
    assert!(!output.is_empty(), "kernel should produce UART output");
    // Linux kernel typically prints a version string
    // (exact text depends on libkrunfw kernel version)
}
```

- [ ] **Step 2: Verify test fails**

Run: `cargo test -p opengoose-sandbox --test boot_test -- --test-threads=1`

- [ ] **Step 3: Implement boot.rs**

```rust
use crate::error::{SandboxError, Result};
use crate::hypervisor::*;
use crate::machine;
use crate::uart::{self, Pl011};
use std::ffi::c_void;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use crate::hypervisor::hvf::HvfHypervisor;

// libkrunfw FFI
extern "C" {
    fn krunfw_get_kernel(
        load_addr: *mut usize,
        entry_addr: *mut usize,
        size: *mut usize,
    ) -> *const u8;
}

/// A booted VM with UART, ready for snapshot or direct interaction.
pub struct BootedVm<V: Vm> {
    pub vm: V,
    pub vcpu: V::Vcpu,
    pub uart: Pl011,
    pub mem_ptr: *mut u8,
    pub mem_size: usize,
}

#[cfg(target_os = "macos")]
impl BootedVm<<HvfHypervisor as Hypervisor>::Vm> {
    /// Boot a VM with default settings (128 MiB RAM).
    pub fn boot_default() -> Result<Self> {
        let hv = HvfHypervisor;
        boot(&hv, machine::DEFAULT_RAM_SIZE as usize)
    }
}

/// Boot a Linux VM using libkrunfw kernel.
pub fn boot<H: Hypervisor>(hv: &H, ram_size: usize) -> Result<BootedVm<H::Vm>> {
    // 1. Load kernel from libkrunfw
    let (kernel_ptr, kernel_load_addr, kernel_entry_addr, kernel_size) = load_kernel()?;

    // 2. Allocate guest memory via mmap
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

    // 3. Copy kernel into guest memory at RAM_BASE offset
    // libkrunfw returns load_addr = 0x8000_0000 for aarch64, but we place RAM at 0x4000_0000.
    // The kernel expects to run at a fixed offset from RAM base, so just copy to start of RAM.
    unsafe {
        std::ptr::copy_nonoverlapping(kernel_ptr, mem_ptr, kernel_size);
    }

    // 4. Create and place DTB after kernel
    let kernel_end_gpa = machine::RAM_BASE + kernel_size as u64;
    let dtb_gpa = machine::dtb_addr(kernel_end_gpa);
    let dtb_offset = (dtb_gpa - machine::RAM_BASE) as usize;
    let dtb_bytes = machine::create_dtb(ram_size as u64)?;
    unsafe {
        std::ptr::copy_nonoverlapping(
            dtb_bytes.as_ptr(),
            mem_ptr.add(dtb_offset),
            dtb_bytes.len(),
        );
    }

    // 5. Create VM
    let mut vm = hv.create_vm()?;
    vm.map_memory(machine::RAM_BASE, mem_ptr, ram_size)?;

    // 6. Create GIC
    vm.create_gic(&GicConfig {
        dist_addr: machine::GIC_DIST_ADDR,
        dist_size: machine::GIC_DIST_SIZE,
        redist_addr: machine::GIC_REDIST_ADDR,
        redist_size: machine::GIC_REDIST_SIZE,
    })?;

    // 7. Create vCPU and set boot registers
    let mut vcpu = vm.create_vcpu()?;

    // PC = kernel entry (start of RAM, since we copied kernel there)
    vcpu.set_reg(Reg::Pc, machine::RAM_BASE)?;
    // X0 = DTB address
    vcpu.set_reg(Reg::X0, dtb_gpa)?;
    // X1, X2, X3 = 0 (ARM64 boot protocol)
    vcpu.set_reg(Reg::X1, 0)?;
    vcpu.set_reg(Reg::X2, 0)?;
    vcpu.set_reg(Reg::X3, 0)?;
    // PSTATE = EL1h with DAIF masked
    let pstate: u64 = (0b0101 << 0) // EL1h (M[3:0] = 0b0101)
        | (1 << 6)  // FIQ mask
        | (1 << 7)  // IRQ mask
        | (1 << 8)  // SError mask
        | (1 << 9); // Debug mask
    vcpu.set_reg(Reg::Cpsr, pstate)?;

    let uart = Pl011::new();
    Ok(BootedVm { vm, vcpu, uart, mem_ptr, mem_size: ram_size })
}

/// Load kernel bytes from libkrunfw shared library.
fn load_kernel() -> Result<(*const u8, usize, usize, usize)> {
    let mut load_addr: usize = 0;
    let mut entry_addr: usize = 0;
    let mut size: usize = 0;

    let ptr = unsafe { krunfw_get_kernel(&mut load_addr, &mut entry_addr, &mut size) };
    if ptr.is_null() || size == 0 {
        return Err(SandboxError::Boot(
            "krunfw_get_kernel returned null — is libkrunfw installed? (brew tap slp/krun && brew install libkrunfw)".into()
        ));
    }

    Ok((ptr, load_addr, entry_addr, size))
}

impl<V: Vm> BootedVm<V> {
    /// Run the VM, processing UART MMIO exits, until timeout.
    /// Returns accumulated UART output.
    pub fn collect_uart_output(&mut self, timeout: Duration) -> String {
        let start = Instant::now();
        while start.elapsed() < timeout {
            match self.vcpu.run() {
                Ok(VcpuExit::MmioWrite { addr, data, len }) => {
                    if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                        // For writes, read the value from the source register
                        // data field is 0 (placeholder) — we need to read from vcpu
                        // Actually for UART writes, the guest writes the byte value.
                        // On HVF, we need the SRT register to get the data.
                        // Workaround: get the value after the exit.
                        let offset = addr - uart::PL011_BASE;
                        // For simplicity, re-read via the MmioWrite data field
                        // TODO: properly extract SRT register value
                        self.uart.handle_mmio_write(offset, data);
                    }
                }
                Ok(VcpuExit::MmioRead { addr, len, reg }) => {
                    if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                        let offset = addr - uart::PL011_BASE;
                        let val = self.uart.handle_mmio_read(offset);
                        // Write result back to guest register
                        if reg < 31 {
                            let r = Reg::from_index(reg);
            // Note: Reg::from_index(n) returns X0..X30 for n=0..30, None for 31+
            // Add this method to the Reg enum in hypervisor/mod.rs:
            // pub fn from_index(idx: u8) -> Option<Reg> { ... match on 0..=30 }
                            let _ = self.vcpu.set_reg(r, val);
                        }
                        // reg == 31 means XZR (zero register), discard
                    }
                }
                Ok(VcpuExit::VtimerActivated) => {
                    // Timer fired — acknowledge and continue
                    // Set vtimer mask to prevent immediate re-exit
                    continue;
                }
                Ok(VcpuExit::SystemEvent) => {
                    // WFI/HVC — skip the instruction and continue
                    // Advance PC by 4 bytes (ARM64 fixed-width instructions)
                    if let Ok(pc) = self.vcpu.get_reg(Reg::Pc) {
                        let _ = self.vcpu.set_reg(Reg::Pc, pc + 4);
                    }
                    continue;
                }
                Ok(VcpuExit::Unknown(_)) | Err(_) => {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&self.uart.take_output()).to_string()
    }

    /// Run until a specific marker string appears in UART output.
    pub fn run_until_marker(&mut self, marker: &str, timeout: Duration) -> Result<String> {
        let start = Instant::now();
        let mut all_output = String::new();
        while start.elapsed() < timeout {
            match self.vcpu.run() {
                Ok(VcpuExit::MmioWrite { addr, data, len }) => {
                    if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                        let offset = addr - uart::PL011_BASE;
                        self.uart.handle_mmio_write(offset, data);
                        // Check for marker in accumulated output
                        if let Some(line) = self.uart.read_line() {
                            all_output.push_str(&line);
                            all_output.push('\n');
                            if line.contains(marker) {
                                return Ok(all_output);
                            }
                        }
                    }
                }
                Ok(VcpuExit::MmioRead { addr, len, reg }) => {
                    if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                        let offset = addr - uart::PL011_BASE;
                        let val = self.uart.handle_mmio_read(offset);
                        if reg < 31 {
                            let r = Reg::from_index(reg);
            // Note: Reg::from_index(n) returns X0..X30 for n=0..30, None for 31+
            // Add this method to the Reg enum in hypervisor/mod.rs:
            // pub fn from_index(idx: u8) -> Option<Reg> { ... match on 0..=30 }
                            let _ = self.vcpu.set_reg(r, val);
                        }
                    }
                }
                Ok(VcpuExit::VtimerActivated) => continue,
                Ok(VcpuExit::SystemEvent) => {
                    if let Ok(pc) = self.vcpu.get_reg(Reg::Pc) {
                        let _ = self.vcpu.set_reg(Reg::Pc, pc + 4);
                    }
                    continue;
                }
                Ok(VcpuExit::Unknown(_)) | Err(_) => break,
            }
        }
        Err(SandboxError::Timeout(timeout))
    }
}

impl<V: Vm> Drop for BootedVm<V> {
    fn drop(&mut self) {
        if !self.mem_ptr.is_null() {
            unsafe { libc::munmap(self.mem_ptr as *mut c_void, self.mem_size); }
            self.mem_ptr = std::ptr::null_mut();
        }
    }
}
```

**Note:** The MMIO write data extraction is a known TODO. On HVF, when the guest does `str X5, [uart_addr]`, the exit tells us the address and the source register index (SRT), but NOT the value. We need to read `vcpu.get_reg(Xn)` where n=SRT. The current MmioWrite variant passes `data: 0` as placeholder. This needs to be fixed in the run loop by reading the SRT register after the exit. This will be addressed as part of making the boot test pass — the exact fix depends on what the kernel actually does.

- [ ] **Step 4: Fix MMIO write data extraction in hvf.rs**

Update `decode_exit` to return SRT for writes, and fix the run loop in `boot.rs` to read the register:

In `hvf.rs`, change `MmioWrite` to carry `srt`:
```rust
// In VcpuExit enum (mod.rs):
MmioWrite { addr: u64, len: u8, srt: u8 },
```

In `boot.rs` run loop, after getting `MmioWrite { addr, len, srt }`:
```rust
Ok(VcpuExit::MmioWrite { addr, len, srt }) => {
    let data = if srt < 31 {
        let r = unsafe { std::mem::transmute::<u32, Reg>(srt as u32) };
        self.vcpu.get_reg(r).unwrap_or(0)
    } else {
        0 // XZR
    };
    if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
        let offset = addr - uart::PL011_BASE;
        self.uart.handle_mmio_write(offset, data);
    }
}
```

- [ ] **Step 5: Add libkrunfw linking to Cargo.toml and build.rs**

In `Cargo.toml` add note:
```toml
# libkrunfw must be installed: brew tap slp/krun && brew install libkrunfw
```

In `build.rs`, add libkrunfw linking:
```rust
fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Hypervisor");
        // libkrunfw — dynamically loaded at runtime via FFI
        // The krunfw_get_kernel symbol is resolved from libkrunfw.dylib
        println!("cargo:rustc-link-lib=dylib=krunfw");
    }
}
```

- [ ] **Step 6: Run boot test**

Run: `cargo test -p opengoose-sandbox --test boot_test -- --test-threads=1`
Expected: kernel boots and prints to UART (test passes)

**If test fails:** Debug by checking UART output, adjusting memory layout, or checking libkrunfw compatibility. The kernel + DTB + GIC + UART combination must be compatible.

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose-sandbox/src/boot.rs crates/opengoose-sandbox/tests/boot_test.rs crates/opengoose-sandbox/build.rs
git commit -m "feat(sandbox): VM boot via libkrunfw with PL011 UART output"
```

---

## Task 6: Snapshot — save, load, CoW memory mapping

**Files:**
- Create: `crates/opengoose-sandbox/src/snapshot.rs`
- Test: `crates/opengoose-sandbox/tests/snapshot_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/snapshot_test.rs`:

```rust
use opengoose_sandbox::snapshot::{VmSnapshot, cow_map};
use opengoose_sandbox::hypervisor::VcpuState;
use serial_test::serial;
use std::path::PathBuf;

#[test]
fn test_snapshot_serialize_roundtrip() {
    let snap = VmSnapshot {
        vcpu_state: VcpuState {
            regs: vec![
                (opengoose_sandbox::hypervisor::Reg::Pc, 0x4000_0000),
                (opengoose_sandbox::hypervisor::Reg::X0, 0xDEAD),
            ],
            sys_regs: vec![],
        },
        mem_size: 4096,
        kernel_hash: "test123".into(),
    };
    let dir = tempfile::tempdir().unwrap();
    let meta_path = dir.path().join("snapshot.meta");
    let mem_path = dir.path().join("snapshot.mem");

    // Create a fake mem file
    std::fs::write(&mem_path, vec![0u8; 4096]).unwrap();

    snap.save(&meta_path).unwrap();
    let loaded = VmSnapshot::load(&meta_path).unwrap();
    assert_eq!(loaded.mem_size, 4096);
    assert_eq!(loaded.kernel_hash, "test123");
    assert_eq!(loaded.vcpu_state.regs.len(), 2);
}

#[test]
fn test_cow_map_write_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let mem_path = dir.path().join("snapshot.mem");

    // Create a 4 KiB file with known content
    let original = vec![0xAA_u8; 4096];
    std::fs::write(&mem_path, &original).unwrap();

    // CoW map it
    let (ptr, size) = cow_map(&mem_path, 4096).unwrap();
    assert_eq!(size, 4096);

    // Read should see original content
    let first_byte = unsafe { *ptr };
    assert_eq!(first_byte, 0xAA);

    // Write to CoW mapping
    unsafe { *ptr = 0xBB; }

    // Our view should see the write
    assert_eq!(unsafe { *ptr }, 0xBB);

    // Original file should be unchanged
    let file_content = std::fs::read(&mem_path).unwrap();
    assert_eq!(file_content[0], 0xAA, "original file must not be modified");

    // Cleanup
    unsafe { libc::munmap(ptr as *mut libc::c_void, size); }
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test -p opengoose-sandbox --test snapshot_test`

- [ ] **Step 3: Implement snapshot.rs**

```rust
use crate::error::{SandboxError, Result};
use crate::hypervisor::VcpuState;
use serde::{Serialize, Deserialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSnapshot {
    pub vcpu_state: VcpuState,
    pub mem_size: usize,
    pub kernel_hash: String,
}

impl VmSnapshot {
    /// Save snapshot metadata to disk (bincode).
    pub fn save(&self, path: &Path) -> Result<()> {
        let data = bincode::serialize(self)
            .map_err(|e| SandboxError::Snapshot(format!("serialize: {e}")))?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Load snapshot metadata from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read(path)?;
        bincode::deserialize(&data)
            .map_err(|e| SandboxError::Snapshot(format!("deserialize: {e}")))
    }

    /// Snapshot cache directory.
    pub fn cache_dir() -> Result<std::path::PathBuf> {
        let home = std::env::var("HOME")
            .map_err(|_| SandboxError::Snapshot("HOME not set".into()))?;
        let dir = std::path::PathBuf::from(home)
            .join(".opengoose")
            .join("snapshots")
            .join("aarch64");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

/// Create a CoW memory mapping from a snapshot memory file.
/// Returns (host_ptr, size). The mapping is MAP_PRIVATE — writes go to private pages.
pub fn cow_map(mem_path: &Path, mem_size: usize) -> Result<(*mut u8, usize)> {
    use std::os::unix::io::AsRawFd;

    let file = std::fs::File::open(mem_path)?;
    let fd = file.as_raw_fd();

    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            mem_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_NORESERVE,
            fd,
            0,
        )
    };

    if ptr == libc::MAP_FAILED {
        return Err(SandboxError::Snapshot("CoW mmap failed".into()));
    }

    // Keep the file open is not needed — mmap holds a reference to the vnode.
    // But we must not drop the File before mmap completes.
    // Since mmap is synchronous, this is fine.
    // mmap holds a vnode reference; closing the fd is safe after mmap returns.
    drop(file);

    Ok((ptr as *mut u8, mem_size))
}

/// Save guest memory to a file for later CoW mapping.
pub fn save_memory(mem_ptr: *const u8, mem_size: usize, path: &Path) -> Result<()> {
    let data = unsafe { std::slice::from_raw_parts(mem_ptr, mem_size) };
    std::fs::write(path, data)?;
    Ok(())
}
```

- [ ] **Step 4: Add tempfile to dev-dependencies**

In `Cargo.toml`:
```toml
[dev-dependencies]
serial_test = "3"
tempfile = "3"
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p opengoose-sandbox --test snapshot_test`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-sandbox/src/snapshot.rs crates/opengoose-sandbox/tests/snapshot_test.rs crates/opengoose-sandbox/Cargo.toml
git commit -m "feat(sandbox): snapshot save/load with CoW memory mapping"
```

---

## Task 7: MicroVm — fork from snapshot + exec via serial

**Files:**
- Create: `crates/opengoose-sandbox/src/vm.rs`
- Test: `crates/opengoose-sandbox/tests/vm_test.rs`

- [ ] **Step 1: Write failing test**

Create `tests/vm_test.rs`:

```rust
use opengoose_sandbox::vm::MicroVm;
use opengoose_sandbox::snapshot::VmSnapshot;
use serial_test::serial;

/// This test requires:
/// 1. A snapshot to exist (created by boot + snapshot save)
/// 2. A guest init binary that responds to commands on serial
///
/// For now, test the fork + immediate destroy path.
#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_fork_and_destroy() {
    // First, create a snapshot (boot → pause → save)
    let snap_result = MicroVm::ensure_snapshot();
    // This may fail if libkrunfw is not installed or guest init is missing.
    // Skip gracefully in that case.
    if snap_result.is_err() {
        eprintln!("Skipping: snapshot creation failed: {:?}", snap_result.err());
        return;
    }
    let (snapshot, mem_path) = snap_result.unwrap();

    // Fork a VM from the snapshot
    let vm = MicroVm::fork_from(&snapshot, &mem_path);
    assert!(vm.is_ok(), "fork should succeed: {:?}", vm.err());

    // Drop should clean up without error
    drop(vm);
}
```

- [ ] **Step 2: Implement vm.rs**

```rust
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

        // Run until guest init prints "READY"
        booted.run_until_marker("READY", Duration::from_secs(10))?;

        // Save snapshot
        let vcpu_state = booted.vcpu.get_all_regs()?;
        let snap = VmSnapshot {
            vcpu_state,
            mem_size: booted.mem_size,
            kernel_hash: "libkrunfw".into(), // TODO: compute actual hash
        };
        snap.save(&meta_path)?;
        snapshot::save_memory(booted.mem_ptr, booted.mem_size, &mem_path)?;

        // Destroy the booted VM
        drop(booted.vcpu);
        booted.vm.destroy()?;

        Ok((snap, mem_path))
    }

    /// Fork a new VM from a snapshot using CoW memory mapping.
    #[cfg(target_os = "macos")]
    pub fn fork_from(snapshot: &VmSnapshot, mem_path: &Path) -> Result<Self> {
        // CoW map the snapshot memory
        let (mem_ptr, mem_size) = snapshot::cow_map(mem_path, snapshot.mem_size)?;

        // Create VM
        let hv = HvfHypervisor;
        let mut vm = hv.create_vm()?;
        vm.map_memory(machine::RAM_BASE, mem_ptr, mem_size)?;

        // Create GIC
        vm.create_gic(&GicConfig {
            dist_addr: machine::GIC_DIST_ADDR,
            dist_size: machine::GIC_DIST_SIZE,
            redist_addr: machine::GIC_REDIST_ADDR,
            redist_size: machine::GIC_REDIST_SIZE,
        })?;

        // Create vCPU and restore registers
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
        // Build JSON command
        let all_args: Vec<&str> = std::iter::once(cmd).chain(args.iter().copied()).collect();
        let json = serde_json::json!({"cmd": "exec", "args": all_args});
        let input = format!("{}\n", json);

        // Push command to UART input
        self.uart.push_input(input.as_bytes());

        // Run VM until we get a response line
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            match self.vcpu.run() {
                Ok(VcpuExit::MmioWrite { addr, len, srt }) => {
                    let data = self.read_srt(srt);
                    if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                        self.uart.handle_mmio_write(addr - uart::PL011_BASE, data);
                        if let Some(line) = self.uart.read_line() {
                            // Try to parse as JSON response
                            if let Ok(resp) = serde_json::from_str::<ExecResponse>(&line) {
                                return Ok(ExecResult {
                                    status: resp.status,
                                    stdout: resp.stdout,
                                    stderr: resp.stderr,
                                });
                            }
                        }
                    }
                }
                Ok(VcpuExit::MmioRead { addr, len, reg }) => {
                    if addr >= uart::PL011_BASE && addr < uart::PL011_BASE + uart::PL011_SIZE {
                        let val = self.uart.handle_mmio_read(addr - uart::PL011_BASE);
                        self.write_srt(reg, val);
                    }
                }
                Ok(VcpuExit::VtimerActivated) => continue,
                Ok(VcpuExit::SystemEvent) => {
                    if let Ok(pc) = self.vcpu.get_reg(Reg::Pc) {
                        let _ = self.vcpu.set_reg(Reg::Pc, pc + 4);
                    }
                }
                _ => break,
            }
        }
        Err(SandboxError::Timeout(timeout))
    }

    fn read_srt(&self, srt: u8) -> u64 {
        if srt < 31 {
            let r = Reg::from_index(srt).unwrap();
            self.vcpu.get_reg(r).unwrap_or(0)
        } else {
            0
        }
    }

    fn write_srt(&mut self, reg: u8, val: u64) {
        if reg < 31 {
            let r = Reg::from_index(reg);
            // Note: Reg::from_index(n) returns X0..X30 for n=0..30, None for 31+
            // Add this method to the Reg enum in hypervisor/mod.rs:
            // pub fn from_index(idx: u8) -> Option<Reg> { ... match on 0..=30 }
            let _ = self.vcpu.set_reg(r, val);
        }
    }
}

impl Drop for MicroVm {
    fn drop(&mut self) {
        if !self.mem_ptr.is_null() {
            unsafe { libc::munmap(self.mem_ptr as *mut libc::c_void, self.mem_size); }
            self.mem_ptr = std::ptr::null_mut();
        }
        // vcpu and vm are dropped automatically
    }
}

/// Result of executing a command in the sandbox.
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
```

- [ ] **Step 3: Add serde_json dependency**

In `Cargo.toml`:
```toml
serde_json = "1"
```

- [ ] **Step 4: Run test**

Run: `cargo test -p opengoose-sandbox --test vm_test -- --test-threads=1`
Expected: fork_and_destroy test passes (or skips gracefully if libkrunfw not installed)

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-sandbox/src/vm.rs crates/opengoose-sandbox/tests/vm_test.rs crates/opengoose-sandbox/Cargo.toml
git commit -m "feat(sandbox): MicroVm fork from snapshot with exec protocol"
```

---

## Task 8: Guest init binary

**Files:**
- Create: `crates/opengoose-sandbox/guest/init/Cargo.toml`
- Create: `crates/opengoose-sandbox/guest/init/src/main.rs`

This is a standalone Rust binary, NOT a workspace member. Built for `aarch64-unknown-linux-musl` target.

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "sandbox-guest-init"
version = "0.1.0"
edition = "2021"  # musl target compatibility — don't use 2024

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
opt-level = "s"
lto = true
strip = true
panic = "abort"
```

- [ ] **Step 2: Implement guest init**

```rust
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
    // PID 1 duties: mount essential filesystems
    let _ = fs::create_dir_all("/proc");
    let _ = fs::create_dir_all("/sys");
    let _ = fs::create_dir_all("/dev");
    let _ = fs::create_dir_all("/tmp");

    mount_or_ignore("proc", "/proc", "proc");
    mount_or_ignore("sysfs", "/sys", "sysfs");
    mount_or_ignore("devtmpfs", "/dev", "devtmpfs");

    // Open serial device (PL011 = /dev/ttyAMA0)
    let serial_path = if std::path::Path::new("/dev/ttyAMA0").exists() {
        "/dev/ttyAMA0"
    } else if std::path::Path::new("/dev/hvc0").exists() {
        "/dev/hvc0"
    } else {
        // Fallback to console
        "/dev/console"
    };

    let serial_in = File::open(serial_path).expect("open serial for reading");
    let mut serial_out = OpenOptions::new()
        .write(true)
        .open(serial_path)
        .expect("open serial for writing");

    // Signal readiness
    writeln!(serial_out, "READY").expect("write READY");

    // Command loop
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
```

- [ ] **Step 3: Build guest init**

```bash
# Install musl target if needed
rustup target add aarch64-unknown-linux-musl

# Build
cd crates/opengoose-sandbox/guest/init
cargo build --release --target aarch64-unknown-linux-musl
```

Expected: produces `target/aarch64-unknown-linux-musl/release/sandbox-guest-init` (static binary)

- [ ] **Step 4: Verify binary is static**

```bash
file target/aarch64-unknown-linux-musl/release/sandbox-guest-init
```
Expected: `ELF 64-bit LSB executable, ARM aarch64, ... statically linked`

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-sandbox/guest/
git commit -m "feat(sandbox): guest init binary for serial command execution"
```

---

## Task 8b: Initramfs creation + boot integration

**Files:**
- Create: `crates/opengoose-sandbox/src/initramfs.rs`

The guest init binary must be packaged into a cpio initramfs and loaded into guest memory alongside the kernel. The kernel then mounts this as the root filesystem and execs `/init`.

- [ ] **Step 1: Implement initramfs.rs**

Build a minimal newc-format cpio archive containing `/init` (the guest binary):

```rust
use crate::error::{SandboxError, Result};
use std::path::Path;

/// Build a minimal cpio newc archive containing a single file as /init.
/// Format: https://man7.org/linux/man-pages/man5/cpio.5.html (newc format)
pub fn build_initramfs(init_binary: &[u8]) -> Vec<u8> {
    let mut archive = Vec::new();

    // /init entry
    append_cpio_entry(&mut archive, "init", init_binary, 0o100755);

    // TRAILER!!! marks end of archive
    append_cpio_entry(&mut archive, "TRAILER!!!", &[], 0);

    // Pad to 512-byte boundary (some kernels expect this)
    while archive.len() % 512 != 0 {
        archive.push(0);
    }

    archive
}

fn append_cpio_entry(archive: &mut Vec<u8>, name: &str, data: &[u8], mode: u32) {
    let name_with_nul = format!("{}\0", name);
    let namesize = name_with_nul.len();

    // newc header: 6 bytes magic + 13 fields of 8 hex chars each = 110 bytes
    let header = format!(
        "070701\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}\
         {:08X}",
        0,              // ino
        mode,           // mode
        0,              // uid
        0,              // gid
        1,              // nlink
        0,              // mtime
        data.len(),     // filesize
        0, 0,           // devmajor, devminor
        0, 0,           // rdevmajor, rdevminor
        namesize,       // namesize
        0,              // checksum
    );

    archive.extend_from_slice(header.as_bytes());
    archive.extend_from_slice(name_with_nul.as_bytes());

    // Pad to 4-byte boundary after header + name
    let header_plus_name = 110 + namesize;
    while archive.len() % 4 != 0 {
        archive.push(0);
    }

    // File data
    archive.extend_from_slice(data);

    // Pad to 4-byte boundary after data
    while archive.len() % 4 != 0 {
        archive.push(0);
    }
}

/// Load the pre-built guest init binary.
/// Expects it at a known path relative to the crate, or embedded via include_bytes.
pub fn load_guest_init() -> Result<Vec<u8>> {
    // Try multiple locations
    let candidates = [
        // Relative to crate root (dev builds)
        concat!(env!("CARGO_MANIFEST_DIR"), "/guest/init/target/aarch64-unknown-linux-musl/release/sandbox-guest-init"),
        // System install path
        "/usr/local/share/opengoose/sandbox-guest-init",
    ];

    for path in &candidates {
        if let Ok(data) = std::fs::read(path) {
            return Ok(data);
        }
    }

    Err(SandboxError::Boot(format!(
        "guest init binary not found. Build it with: \
         cd crates/opengoose-sandbox/guest/init && \
         cargo build --release --target aarch64-unknown-linux-musl"
    )))
}
```

- [ ] **Step 2: Update boot.rs to load initramfs into guest memory**

In `boot.rs`, after placing the DTB, add initramfs loading:

```rust
// After DTB placement:
use crate::initramfs;

// 5. Build and place initramfs
let guest_init = initramfs::load_guest_init()?;
let initramfs_data = initramfs::build_initramfs(&guest_init);
let initramfs_gpa = machine::dtb_addr(dtb_gpa + dtb_bytes.len() as u64);
let initramfs_offset = (initramfs_gpa - machine::RAM_BASE) as usize;
let initramfs_end = initramfs_gpa + initramfs_data.len() as u64;
unsafe {
    std::ptr::copy_nonoverlapping(
        initramfs_data.as_ptr(),
        mem_ptr.add(initramfs_offset),
        initramfs_data.len(),
    );
}
```

And update the DTB chosen node to include initrd addresses. In `machine.rs`, add parameters:

```rust
pub fn create_dtb(ram_size: u64, initrd_start: Option<u64>, initrd_end: Option<u64>) -> Result<Vec<u8>> {
    // ... in the chosen node:
    if let (Some(start), Some(end)) = (initrd_start, initrd_end) {
        fdt.property_u64("linux,initrd-start", start).map_err(map_err)?;
        fdt.property_u64("linux,initrd-end", end).map_err(map_err)?;
    }
}
```

- [ ] **Step 3: Add libc to guest init Cargo.toml**

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
libc = "0.2"
```

- [ ] **Step 4: Test that boot reaches READY marker**

Update `boot_test.rs`:

```rust
#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_boot_to_ready() {
    let mut vm = BootedVm::boot_default().expect("boot VM");
    let result = vm.run_until_marker("READY", std::time::Duration::from_secs(10));
    assert!(result.is_ok(), "VM should boot to READY: {:?}", result.err());
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-sandbox/src/initramfs.rs crates/opengoose-sandbox/src/boot.rs crates/opengoose-sandbox/src/machine.rs
git commit -m "feat(sandbox): initramfs creation and boot integration"
```

---

## Task 9: SandboxPool — public API

**Files:**
- Create: `crates/opengoose-sandbox/src/pool.rs`
- Update: `crates/opengoose-sandbox/src/lib.rs`

- [ ] **Step 1: Implement pool.rs**

```rust
use crate::error::Result;
use crate::snapshot::VmSnapshot;
use crate::vm::MicroVm;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Manages snapshot lifecycle and VM acquisition.
/// First `acquire()` auto-creates and caches the snapshot.
/// HVF constraint: only one VM at a time (sequential reuse).
/// Mutex ensures no concurrent VM creation (hv_vm_create is not reentrant).
pub struct SandboxPool {
    snapshot: OnceLock<(VmSnapshot, PathBuf)>,
    lock: Mutex<()>,
}

impl SandboxPool {
    pub fn new() -> Self {
        SandboxPool {
            snapshot: OnceLock::new(),
            lock: Mutex::new(()),
        }
    }

    /// Acquire a forked MicroVm. Creates snapshot on first call.
    /// Only one VM can exist at a time (HVF constraint).
    /// The returned MicroVm must be dropped before calling acquire() again.
    #[cfg(target_os = "macos")]
    pub fn acquire(&self) -> Result<MicroVm> {
        let _guard = self.lock.lock()
            .map_err(|_| SandboxError::Hypervisor("pool lock poisoned".into(), -1))?;
        let (snapshot, mem_path) = self.snapshot.get_or_try_init(|| {
            MicroVm::ensure_snapshot()
        })?;
        MicroVm::fork_from(snapshot, mem_path)
    }

    /// Clear cached snapshot (forces re-creation on next acquire).
    pub fn invalidate(&mut self) {
        self.snapshot = OnceLock::new();
    }
}
```

- [ ] **Step 2: Update lib.rs exports**

Ensure `lib.rs` exports are clean:
```rust
pub mod error;
pub mod hypervisor;
pub mod machine;
pub mod uart;
pub mod boot;
pub mod snapshot;
pub mod vm;
pub mod pool;

pub use error::{SandboxError, Result};
pub use pool::SandboxPool;
pub use vm::{MicroVm, ExecResult};
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p opengoose-sandbox`

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-sandbox/src/pool.rs crates/opengoose-sandbox/src/lib.rs
git commit -m "feat(sandbox): SandboxPool with lazy snapshot init"
```

---

## Task 10: Integration test — end-to-end

**Files:**
- Create: `crates/opengoose-sandbox/tests/integration_test.rs`

This test exercises the full flow: pool → acquire → exec → result.

- [ ] **Step 1: Write integration test**

```rust
use opengoose_sandbox::{SandboxPool, MicroVm};
use serial_test::serial;
use std::time::Duration;

#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_full_sandbox_flow() {
    let pool = SandboxPool::new();

    // First acquire — triggers snapshot creation (slow, ~3-5s)
    let mut vm = pool.acquire().expect("acquire VM");

    // Execute a simple command
    let result = vm.exec("echo", &["hello", "sandbox"], Duration::from_secs(5))
        .expect("exec echo");

    assert_eq!(result.status, 0);
    assert!(result.stdout.contains("hello sandbox"));
    assert!(result.stderr.is_empty());

    drop(vm);

    // Second acquire — should be fast (CoW fork from cached snapshot)
    let start = std::time::Instant::now();
    let mut vm2 = pool.acquire().expect("acquire VM again");
    let fork_time = start.elapsed();

    // Should be sub-10ms (ideally ~1ms)
    assert!(
        fork_time < Duration::from_millis(100),
        "fork took {:?}, expected < 100ms",
        fork_time
    );

    // Execute another command to verify isolation
    let result2 = vm2.exec("echo", &["second"], Duration::from_secs(5))
        .expect("exec in second VM");
    assert_eq!(result2.status, 0);
    assert!(result2.stdout.contains("second"));
}

#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_sandbox_error_handling() {
    let pool = SandboxPool::new();
    let mut vm = pool.acquire().expect("acquire VM");

    // Execute a nonexistent command
    let result = vm.exec("nonexistent_command_12345", &[], Duration::from_secs(5));

    match result {
        Ok(r) => {
            assert_ne!(r.status, 0, "nonexistent command should fail");
            assert!(!r.stderr.is_empty());
        }
        Err(_) => {
            // Timeout or guest error is also acceptable
        }
    }
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p opengoose-sandbox --test integration_test -- --test-threads=1`
Expected: both tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose-sandbox/tests/integration_test.rs
git commit -m "test(sandbox): end-to-end integration tests"
```

---

## Task 11: Wire up to opengoose-rig (optional, can defer)

**Files:**
- Modify: `crates/opengoose-rig/Cargo.toml`
- Modify: `crates/opengoose-rig/src/mcp_tools.rs`

This task adds the `SandboxedClient` wrapper. Defer if the core sandbox is still being stabilized.

- [ ] **Step 1: Add optional dependency**

In `crates/opengoose-rig/Cargo.toml`:
```toml
[dependencies]
opengoose-sandbox = { workspace = true, optional = true }

[features]
sandbox = ["dep:opengoose-sandbox"]
```

In root `Cargo.toml` workspace dependencies:
```toml
opengoose-sandbox = { path = "crates/opengoose-sandbox" }
```

- [ ] **Step 2: Add SandboxedClient to mcp_tools.rs**

Add at the end of the file:

```rust
#[cfg(feature = "sandbox")]
pub mod sandboxed {
    use super::*;
    use opengoose_sandbox::{SandboxPool, MicroVm};
    use std::sync::Arc;
    use std::time::Duration;

    pub struct SandboxedClient<C: McpClientTrait> {
        inner: C,
        pool: Arc<SandboxPool>,
        sandboxed_tools: Vec<String>,
    }

    impl<C: McpClientTrait> SandboxedClient<C> {
        pub fn new(inner: C, pool: Arc<SandboxPool>, sandboxed_tools: Vec<String>) -> Self {
            SandboxedClient { inner, pool, sandboxed_tools }
        }
    }

    #[async_trait]
    impl<C: McpClientTrait + Send + Sync> McpClientTrait for SandboxedClient<C> {
        async fn initialize(&self) -> std::result::Result<InitializeResult, Error> {
            self.inner.initialize().await
        }

        async fn list_tools(&self) -> std::result::Result<ListToolsResult, Error> {
            self.inner.list_tools().await
        }

        async fn call_tool(
            &self,
            tool_name: &str,
            arguments: Option<JsonObject>,
        ) -> std::result::Result<CallToolResult, Error> {
            if self.sandboxed_tools.contains(&tool_name.to_string()) {
                // Execute in sandbox
                let mut vm = self.pool.acquire()
                    .map_err(|e| Error::Other(format!("sandbox: {e}")))?;
                let args_str = serde_json::to_string(&arguments).unwrap_or_default();
                let result = vm.exec(tool_name, &[&args_str], Duration::from_secs(30))
                    .map_err(|e| Error::Other(format!("sandbox exec: {e}")))?;

                Ok(CallToolResult {
                    content: vec![Content::text(result.stdout)],
                    is_error: Some(result.status != 0),
                    ..Default::default()
                })
            } else {
                self.inner.call_tool(tool_name, arguments).await
            }
        }

        fn get_info(&self) -> Option<&InitializeResult> {
            self.inner.get_info()
        }
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p opengoose-rig --features sandbox`

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/Cargo.toml crates/opengoose-rig/src/mcp_tools.rs Cargo.toml
git commit -m "feat(rig): SandboxedClient wrapper for sandboxed tool execution"
```

---

## Open Items (for future tasks)

- **Guest init embedding**: Currently the guest init binary must be built separately and embedded into the kernel rootfs (initramfs). Need a build pipeline for this.
- **libkrunfw kernel compatibility**: Verify that the libkrunfw kernel's `.config` enables PL011 UART and matches our DTB.
- **GIC state save/restore**: Test whether `hv_gic_state_create()` / `hv_gic_set_state()` work correctly for snapshot fork.
- **VTimer handling**: Proper virtual timer forwarding in the run loop.
- **x86_64 support**: Add KVM backend implementing the same Hypervisor trait.
- **virtio-fs**: Host filesystem mounting for sharing project directories.
- **Network**: virtio-net for sandboxed network access.
