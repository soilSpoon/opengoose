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
    MpidrEl1 = 0xc005,
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
    // Pointer Authentication keys
    ApiaKeyLo = 0xc108,
    ApiaKeyHi = 0xc109,
    ApibKeyLo = 0xc10a,
    ApibKeyHi = 0xc10b,
    ApdaKeyLo = 0xc110,
    ApdaKeyHi = 0xc111,
    ApdbKeyLo = 0xc112,
    ApdbKeyHi = 0xc113,
    ApgaKeyLo = 0xc118,
    ApgaKeyHi = 0xc119,
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
    /// WFI/WFE trap. PC points to the trapping instruction; caller must advance.
    WaitForEvent,
    /// HVC/SMC trap. PC already points past the instruction (ARM64 convention).
    /// `imm` is the immediate value from the HVC/SMC instruction (ISS[15:0]).
    HypervisorCall { imm: u16 },
    /// MSR/MRS system register access trap. Must advance PC.
    /// `syndrome` is the ESR value — contains Op0/Op1/CRn/CRm/Op2/Rt/direction.
    SystemRegAccess { syndrome: u64 },
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
    /// Inject or de-assert an SPI interrupt via the GIC.
    fn set_spi(&self, intid: u32, level: bool) -> Result<()> {
        let _ = (intid, level);
        Ok(())
    }
    /// Save GIC state (distributor + redistributor configuration).
    fn save_gic_state(&self) -> Result<Vec<u8>> { Ok(Vec::new()) }
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
    /// Platform-specific vCPU identifier for force-exit. Default 0 (unused).
    fn vcpu_id(&self) -> u64 { 0 }
    /// Set pending IRQ state for next vcpu_run call.
    fn set_irq_pending(&mut self, pending: bool) { let _ = pending; }
    /// Reset injection tracking so next set_irq_pending(true) re-injects.
    fn reset_irq_injection(&mut self) {}
    /// Get virtual timer offset.
    fn get_vtimer_offset(&self) -> Result<u64> { Ok(0) }
    /// Set virtual timer offset.
    fn set_vtimer_offset(&mut self, offset: u64) -> Result<()> { let _ = offset; Ok(()) }
    /// Set vtimer mask (HVF auto-masks on VTIMER_ACTIVATED exit).
    fn set_vtimer_mask(&mut self, masked: bool) { let _ = masked; }
}

/// Convert an ARM64 register index (0-30) to Reg enum.
/// Index 31 = XZR (zero register), returns None.
pub fn reg_from_index(idx: u8) -> Option<Reg> {
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

#[cfg(target_os = "macos")]
pub mod hvf;
