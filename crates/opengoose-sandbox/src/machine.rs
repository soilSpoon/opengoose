use crate::error::{SandboxError, Result};
use crate::uart;

pub const GIC_DIST_ADDR: u64 = 0x0800_0000;
pub const GIC_DIST_SIZE: u64 = 0x0001_0000;
pub const GIC_REDIST_ADDR: u64 = 0x080A_0000;
pub const GIC_REDIST_SIZE: u64 = 0x0002_0000; // 128KB: 1 CPU × (64KB RD_base + 64KB SGI_base)
pub const UART_ADDR: u64 = uart::PL011_BASE;
pub const UART_SIZE: u64 = uart::PL011_SIZE;
pub const RAM_BASE: u64 = 0x4000_0000;
pub const DEFAULT_RAM_SIZE: u64 = 256 * 1024 * 1024;

/// Virtio-mmio console device
pub const VIRTIO_MMIO_BASE: u64 = 0x0A00_0000;
pub const VIRTIO_MMIO_SIZE: u64 = 0x200;
pub const VIRTIO_IRQ: u32 = 2; // SPI 2

/// Emulate GIC redistributor MMIO reads. Used by both boot and exec VM loops.
pub fn handle_gic_redist_read(addr: u64) -> Option<u64> {
    if addr < GIC_REDIST_ADDR || addr >= GIC_REDIST_ADDR + GIC_REDIST_SIZE {
        return None;
    }
    let offset = addr - GIC_REDIST_ADDR;
    let val = match offset {
        0x0000 => 0,                // GICR_CTLR
        0x0004 => 0x0100_043B,      // GICR_IIDR (ARM GICv3)
        0x0008 => 1 << 4,           // GICR_TYPER low: Last=1
        0x000C => 0,                // GICR_TYPER high
        0x0010 => 0,                // GICR_STATUSR
        0x0014 => 0,                // GICR_WAKER
        0xFFE8 => 0x3B,             // GICR_PIDR2
        0x10080 => 0,               // GICR_IGROUPR0
        0x10100 => 0,               // GICR_ISENABLER0
        0x10180 => 0,               // GICR_ICENABLER0
        0x10C00 | 0x10C04 => 0,     // GICR_ICFGR0/1
        o if (0x10400..0x10420).contains(&o) => 0, // GICR_IPRIORITYR
        _ => 0,
    };
    Some(val)
}

const GIC_PHANDLE: u32 = 1;
const CLOCK_PHANDLE: u32 = 2;
const GIC_FDT_IRQ_TYPE_SPI: u32 = 0;
const GIC_FDT_IRQ_TYPE_PPI: u32 = 1;
const IRQ_TYPE_LEVEL_HI: u32 = 4;
const GTIMER_SEC: u32 = 13;
const GTIMER_HYP: u32 = 14;
const GTIMER_VIRT: u32 = 11;
const GTIMER_PHYS: u32 = 12;

pub fn dtb_addr(kernel_end: u64) -> u64 {
    (kernel_end + 0xFFF) & !0xFFF
}

fn prop64(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn prop32(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

/// Optional initrd location for the chosen node.
pub struct InitrdInfo {
    pub start_gpa: u64,
    pub end_gpa: u64,
}

pub fn create_dtb(ram_size: u64) -> Result<Vec<u8>> {
    create_dtb_with_initrd(ram_size, None)
}

pub fn create_dtb_with_initrd(ram_size: u64, initrd: Option<&InitrdInfo>) -> Result<Vec<u8>> {
    use vm_fdt::FdtWriter;
    let mut fdt = FdtWriter::new().map_err(|e| SandboxError::Boot(format!("FDT: {e}")))?;
    let map_err = |e: vm_fdt::Error| SandboxError::Boot(format!("FDT: {e}"));

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
        fdt.property("reg", &prop64(&[GIC_DIST_ADDR, GIC_DIST_SIZE, GIC_REDIST_ADDR, GIC_REDIST_SIZE])).map_err(map_err)?;
        fdt.property_u32("phandle", GIC_PHANDLE).map_err(map_err)?;
        fdt.property_u32("#address-cells", 2).map_err(map_err)?;
        fdt.property_u32("#size-cells", 2).map_err(map_err)?;
        fdt.property_null("ranges").map_err(map_err)?;
        fdt.property("interrupts", &prop32(&[GIC_FDT_IRQ_TYPE_PPI, 9, IRQ_TYPE_LEVEL_HI])).map_err(map_err)?;
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
        let uart_node = fdt.begin_node(&format!("uart@{UART_ADDR:x}")).map_err(map_err)?;
        fdt.property_string_list("compatible", vec!["arm,pl011".into(), "arm,primecell".into()]).map_err(map_err)?;
        fdt.property_string("status", "okay").map_err(map_err)?;
        fdt.property("reg", &prop64(&[UART_ADDR, UART_SIZE])).map_err(map_err)?;
        fdt.property("interrupts", &prop32(&[GIC_FDT_IRQ_TYPE_SPI, uart::PL011_IRQ, IRQ_TYPE_LEVEL_HI])).map_err(map_err)?;
        fdt.property_u32("clocks", CLOCK_PHANDLE).map_err(map_err)?;
        fdt.property_string("clock-names", "apb_pclk").map_err(map_err)?;
        fdt.end_node(uart_node).map_err(map_err)?;
    }

    // Clock
    {
        let clk = fdt.begin_node("apb-pclk").map_err(map_err)?;
        fdt.property_string("compatible", "fixed-clock").map_err(map_err)?;
        fdt.property_u32("#clock-cells", 0).map_err(map_err)?;
        fdt.property_u32("clock-frequency", 24_000_000).map_err(map_err)?;
        fdt.property_u32("phandle", CLOCK_PHANDLE).map_err(map_err)?;
        fdt.end_node(clk).map_err(map_err)?;
    }

    // Virtio-mmio console device
    {
        let virtio = fdt.begin_node(&format!("virtio_mmio@{VIRTIO_MMIO_BASE:x}")).map_err(map_err)?;
        fdt.property_string("compatible", "virtio,mmio").map_err(map_err)?;
        fdt.property("reg", &prop64(&[VIRTIO_MMIO_BASE, VIRTIO_MMIO_SIZE])).map_err(map_err)?;
        fdt.property("interrupts", &prop32(&[GIC_FDT_IRQ_TYPE_SPI, VIRTIO_IRQ, IRQ_TYPE_LEVEL_HI])).map_err(map_err)?;
        fdt.end_node(virtio).map_err(map_err)?;
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
        if let Some(initrd) = initrd {
            fdt.property_u64("linux,initrd-start", initrd.start_gpa).map_err(map_err)?;
            fdt.property_u64("linux,initrd-end", initrd.end_gpa).map_err(map_err)?;
        }
        fdt.end_node(chosen).map_err(map_err)?;
    }

    fdt.end_node(root).map_err(map_err)?;
    fdt.finish().map_err(map_err)
}
