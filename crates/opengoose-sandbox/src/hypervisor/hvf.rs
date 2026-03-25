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

#[repr(C)]
struct HvVcpuExitException {
    syndrome: u64,
    virtual_address: u64,
    physical_address: u64,
}

#[repr(C)]
struct HvVcpuExit {
    reason: HvExitReason,
    _pad: u32,
    exception: HvVcpuExitException,
}

unsafe extern "C" {
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
    fn hv_gic_set_spi(intid: u32, level: bool) -> HvReturn;

    // Force vCPU exit from another thread
    fn hv_vcpus_exit(vcpus: *const HvVcpuT, count: u32) -> HvReturn;

    // Set pending interrupt on vCPU
    fn hv_vcpu_set_pending_interrupt(vcpu: HvVcpuT, r#type: u32, pending: bool) -> HvReturn;

    // Virtual timer offset
    fn hv_vcpu_get_vtimer_offset(vcpu: HvVcpuT, offset: *mut u64) -> HvReturn;
    fn hv_vcpu_set_vtimer_offset(vcpu: HvVcpuT, offset: u64) -> HvReturn;
    fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpuT, masked: bool) -> HvReturn;

    // GIC state save/restore
    fn hv_gic_state_create() -> *mut c_void;
    fn hv_gic_state_get_size(state: *mut c_void, size: *mut usize) -> HvReturn;
    fn hv_gic_state_get_data(state: *mut c_void, data: *mut c_void) -> HvReturn;
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
                0x01 => VcpuExit::WaitForEvent,
                // HVC
                0x16 => VcpuExit::HypervisorCall { imm: (syndrome & 0xFFFF) as u16 },
                // SMC from AArch64
                0x17 => VcpuExit::HypervisorCall { imm: (syndrome & 0xFFFF) as u16 },
                // MSR/MRS system register access trap
                0x18 => VcpuExit::SystemRegAccess { syndrome },
                _ => VcpuExit::Unknown(ec as u32),
            }
        }
        HV_EXIT_REASON_VTIMER_ACTIVATED => VcpuExit::VtimerActivated,
        HV_EXIT_REASON_CANCELED => VcpuExit::Unknown(0),
        _ => VcpuExit::Unknown(exit.reason),
    }
}

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
        Ok(HvfVcpu { id: vcpu_id, exit_ptr, irq_pending: false, irq_was_injected: false })
    }

    fn set_spi(&self, intid: u32, level: bool) -> Result<()> {
        unsafe { check(hv_gic_set_spi(intid, level), "hv_gic_set_spi") }
    }

    fn save_gic_state(&self) -> Result<Vec<u8>> {
        unsafe {
            let state = hv_gic_state_create();
            if state.is_null() {
                return Err(SandboxError::Snapshot("hv_gic_state_create returned null".into()));
            }
            let mut size: usize = 0;
            check(hv_gic_state_get_size(state, &mut size), "hv_gic_state_get_size")?;
            let mut data = vec![0u8; size];
            check(hv_gic_state_get_data(state, data.as_mut_ptr() as *mut c_void), "hv_gic_state_get_data")?;
            Ok(data)
        }
    }

    // destroy is handled by Drop impl
}

pub struct HvfVcpu {
    id: HvVcpuT,
    exit_ptr: *const HvVcpuExit,
    irq_pending: bool,
    irq_was_injected: bool,
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
            // Pointer Authentication keys — required for CoW fork
            SysReg::ApiaKeyLo, SysReg::ApiaKeyHi,
            SysReg::ApibKeyLo, SysReg::ApibKeyHi,
            SysReg::ApdaKeyLo, SysReg::ApdaKeyHi,
            SysReg::ApdbKeyLo, SysReg::ApdbKeyHi,
            SysReg::ApgaKeyLo, SysReg::ApgaKeyHi,
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
            // Only call hv_vcpu_set_pending_interrupt on state transition
            // to avoid repeated CANCELED exits.
            if self.irq_pending && !self.irq_was_injected {
                let _ = hv_vcpu_set_pending_interrupt(self.id, 0, true);
                self.irq_was_injected = true;
            } else if !self.irq_pending && self.irq_was_injected {
                let _ = hv_vcpu_set_pending_interrupt(self.id, 0, false);
                self.irq_was_injected = false;
            }
            check(hv_vcpu_run(self.id), "hv_vcpu_run")?;
            Ok(decode_exit(&*self.exit_ptr))
        }
    }

    fn vcpu_id(&self) -> u64 {
        self.id
    }

    fn set_irq_pending(&mut self, pending: bool) {
        self.irq_pending = pending;
    }

    fn reset_irq_injection(&mut self) {
        self.irq_was_injected = false;
    }

    fn set_vtimer_mask(&mut self, masked: bool) {
        unsafe { let _ = hv_vcpu_set_vtimer_mask(self.id, masked); }
    }

    fn get_vtimer_offset(&self) -> Result<u64> {
        let mut offset: u64 = 0;
        unsafe { check(hv_vcpu_get_vtimer_offset(self.id, &mut offset), "get_vtimer_offset")? };
        Ok(offset)
    }

    fn set_vtimer_offset(&mut self, offset: u64) -> Result<()> {
        unsafe { check(hv_vcpu_set_vtimer_offset(self.id, offset), "set_vtimer_offset") }
    }
}


/// Request a running vCPU to exit. Can be called from any thread.
pub fn force_vcpu_exit(vcpu_id: u64) -> Result<()> {
    unsafe { check(hv_vcpus_exit(&vcpu_id, 1), "hv_vcpus_exit") }
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
