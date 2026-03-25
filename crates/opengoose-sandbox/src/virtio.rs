//! Minimal virtio-mmio console device.
//! Only implements what the Linux virtio-console driver needs:
//! - 2 queues: RX (receiveq, idx=0) and TX (transmitq, idx=1)
//! - No feature negotiation (features=0)
//! - No multiport

use crate::machine;

// Virtio MMIO register offsets
const MAGIC_VALUE: u64 = 0x000;
const VERSION: u64 = 0x004;
const DEVICE_ID: u64 = 0x008;
const VENDOR_ID: u64 = 0x00C;
const DEVICE_FEATURES: u64 = 0x010;
const DEVICE_FEATURES_SEL: u64 = 0x014;
const DRIVER_FEATURES: u64 = 0x020;
const DRIVER_FEATURES_SEL: u64 = 0x024;
const QUEUE_SEL: u64 = 0x030;
const QUEUE_NUM_MAX: u64 = 0x034;
const QUEUE_NUM: u64 = 0x038;
const QUEUE_READY: u64 = 0x044;
const QUEUE_NOTIFY: u64 = 0x050;
const INTERRUPT_STATUS: u64 = 0x060;
const INTERRUPT_ACK: u64 = 0x064;
const STATUS: u64 = 0x070;
const QUEUE_DESC_LOW: u64 = 0x080;
const QUEUE_DESC_HIGH: u64 = 0x084;
const QUEUE_DRIVER_LOW: u64 = 0x090;
const QUEUE_DRIVER_HIGH: u64 = 0x094;
const QUEUE_DEVICE_LOW: u64 = 0x0A0;
const QUEUE_DEVICE_HIGH: u64 = 0x0A4;
const CONFIG_GENERATION: u64 = 0x0FC;

// Virtio console config (at offset 0x100+)
const CONFIG_COLS: u64 = 0x100; // u16
const CONFIG_ROWS: u64 = 0x102; // u16
const CONFIG_MAX_NR_PORTS: u64 = 0x104; // u32
const CONFIG_EMERG_WR: u64 = 0x108; // u32

const VIRTIO_MAGIC: u32 = 0x7472_6976; // "virt"
const VIRTIO_VERSION: u32 = 2;
const VIRTIO_DEVICE_CONSOLE: u32 = 3;
const VIRTIO_VENDOR: u32 = 0x554D_4551; // "QEMU"

const MAX_QUEUE_SIZE: u32 = 256;
const NUM_QUEUES: usize = 2; // 0=RX, 1=TX

// Vring descriptor flags
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

/// Serializable virtio queue state for snapshot/restore.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VirtioState {
    pub status: u32,
    pub queue_sel: u32,
    pub device_features_sel: u32,
    pub queues: Vec<QueueState>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueueState {
    pub ready: bool,
    pub num: u32,
    pub desc_addr: u64,
    pub driver_addr: u64,
    pub device_addr: u64,
    pub last_avail_idx: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VringDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[derive(Default)]
struct VirtQueue {
    ready: bool,
    num: u32,
    desc_addr: u64,   // GPA of descriptor table
    driver_addr: u64,  // GPA of available ring
    device_addr: u64,  // GPA of used ring
    last_avail_idx: u16,
}

pub struct VirtioConsole {
    status: u32,
    queue_sel: u32,
    device_features_sel: u32,
    interrupt_status: u32,
    queues: [VirtQueue; NUM_QUEUES],
    /// Data received from guest (TX queue output)
    tx_output: Vec<u8>,
    /// Data to send to guest (RX queue input)
    rx_input: Vec<u8>,
    /// Whether there's pending RX data to deliver
    rx_pending: bool,
}

impl VirtioConsole {
    pub fn new() -> Self {
        VirtioConsole {
            status: 0,
            queue_sel: 0,
            device_features_sel: 0,
            interrupt_status: 0,
            queues: Default::default(),
            tx_output: Vec::new(),
            rx_input: Vec::new(),
            rx_pending: false,
        }
    }

    /// Push data to be sent to the guest (host→guest).
    pub fn push_input(&mut self, data: &[u8]) {
        self.rx_input.extend_from_slice(data);
        self.rx_pending = true;
    }

    /// Take accumulated TX output from guest.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.tx_output)
    }

    /// Read a line from TX output (newline-delimited).
    pub fn read_line(&mut self) -> Option<String> {
        if let Some(pos) = self.tx_output.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.tx_output.drain(..=pos).collect();
            Some(String::from_utf8_lossy(&line[..line.len()-1]).to_string())
        } else {
            None
        }
    }

    /// Save virtio device state for snapshot.
    pub fn save_state(&self) -> VirtioState {
        VirtioState {
            status: self.status,
            queue_sel: self.queue_sel,
            device_features_sel: self.device_features_sel,
            queues: self.queues.iter().map(|q| QueueState {
                ready: q.ready,
                num: q.num,
                desc_addr: q.desc_addr,
                driver_addr: q.driver_addr,
                device_addr: q.device_addr,
                last_avail_idx: q.last_avail_idx,
            }).collect(),
        }
    }

    /// Restore virtio device state from snapshot.
    pub fn restore_state(&mut self, state: &VirtioState) {
        self.status = state.status;
        self.queue_sel = state.queue_sel;
        self.device_features_sel = state.device_features_sel;
        for (i, qs) in state.queues.iter().enumerate() {
            if i < NUM_QUEUES {
                self.queues[i].ready = qs.ready;
                self.queues[i].num = qs.num;
                self.queues[i].desc_addr = qs.desc_addr;
                self.queues[i].driver_addr = qs.driver_addr;
                self.queues[i].device_addr = qs.device_addr;
                self.queues[i].last_avail_idx = qs.last_avail_idx;
            }
        }
    }

    /// Whether an IRQ should be injected to the guest.
    pub fn irq_pending(&self) -> bool {
        self.interrupt_status != 0
    }

    /// Handle MMIO read from guest.
    pub fn handle_mmio_read(&self, offset: u64) -> u64 {
        match offset {
            MAGIC_VALUE => VIRTIO_MAGIC as u64,
            VERSION => VIRTIO_VERSION as u64,
            DEVICE_ID => VIRTIO_DEVICE_CONSOLE as u64,
            VENDOR_ID => VIRTIO_VENDOR as u64,
            DEVICE_FEATURES => {
                // Feature bit 32 (VIRTIO_F_VERSION_1) is in the high 32-bit word (sel=1)
                // Feature bits 0-31 are in sel=0
                if self.device_features_sel == 1 {
                    1 // bit 32 = VIRTIO_F_VERSION_1
                } else {
                    0
                }
            }
            QUEUE_NUM_MAX => MAX_QUEUE_SIZE as u64,
            QUEUE_READY => {
                let q = &self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.ready as u64
            }
            INTERRUPT_STATUS => self.interrupt_status as u64,
            STATUS => self.status as u64,
            CONFIG_GENERATION => 0,
            CONFIG_COLS => 80,
            CONFIG_ROWS => 24,
            CONFIG_MAX_NR_PORTS => 1,
            CONFIG_EMERG_WR => 0,
            _ => 0,
        }
    }

    /// Handle MMIO write from guest.
    pub fn handle_mmio_write(&mut self, offset: u64, val: u64) {
        match offset {
            DEVICE_FEATURES_SEL => self.device_features_sel = val as u32,
            DRIVER_FEATURES_SEL | DRIVER_FEATURES => {}
            QUEUE_SEL => self.queue_sel = val as u32,
            QUEUE_NUM => {
                let q = &mut self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.num = val as u32;
            }
            QUEUE_READY => {
                let q = &mut self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.ready = val != 0;
            }
            QUEUE_DESC_LOW => {
                let q = &mut self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.desc_addr = (q.desc_addr & 0xFFFF_FFFF_0000_0000) | val;
            }
            QUEUE_DESC_HIGH => {
                let q = &mut self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.desc_addr = (q.desc_addr & 0x0000_0000_FFFF_FFFF) | (val << 32);
            }
            QUEUE_DRIVER_LOW => {
                let q = &mut self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.driver_addr = (q.driver_addr & 0xFFFF_FFFF_0000_0000) | val;
            }
            QUEUE_DRIVER_HIGH => {
                let q = &mut self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.driver_addr = (q.driver_addr & 0x0000_0000_FFFF_FFFF) | (val << 32);
            }
            QUEUE_DEVICE_LOW => {
                let q = &mut self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.device_addr = (q.device_addr & 0xFFFF_FFFF_0000_0000) | val;
            }
            QUEUE_DEVICE_HIGH => {
                let q = &mut self.queues[self.queue_sel as usize % NUM_QUEUES];
                q.device_addr = (q.device_addr & 0x0000_0000_FFFF_FFFF) | (val << 32);
            }
            QUEUE_NOTIFY => {
                // Guest is notifying us about a queue
                // val = queue index
                // We'll process this in process_notify()
            }
            INTERRUPT_ACK => {
                self.interrupt_status &= !(val as u32);
            }
            STATUS => {
                self.status = val as u32;
                if val == 0 {
                    // Device reset
                    self.queues = Default::default();
                    self.interrupt_status = 0;
                }
            }
            _ => {}
        }
    }

    /// Process a QueueNotify from the guest. Must be called with guest RAM pointer.
    /// queue_idx: 0=RX, 1=TX
    pub fn process_notify(&mut self, queue_idx: u32, mem_ptr: *mut u8, mem_size: usize) {
        match queue_idx {
            1 => self.process_tx(mem_ptr, mem_size),
            _ => {} // RX notify = guest posted buffers, we'll fill them when we have data
        }
    }

    /// Process TX queue: read guest data from descriptors.
    fn process_tx(&mut self, mem_ptr: *mut u8, mem_size: usize) {
        let q = &mut self.queues[1]; // TX queue
        if !q.ready || q.num == 0 { return; }

        loop {
            // Read available ring index
            let avail_idx = read_u16(mem_ptr, mem_size, q.driver_addr + 2);
            if q.last_avail_idx == avail_idx { break; }

            // Get descriptor index from available ring
            let ring_idx = (q.last_avail_idx as u64 % q.num as u64) * 2 + 4;
            let desc_idx = read_u16(mem_ptr, mem_size, q.driver_addr + ring_idx) as u64;

            // Read descriptor chain
            let mut idx = desc_idx;
            loop {
                let desc = read_desc(mem_ptr, mem_size, q.desc_addr, idx);
                if desc.flags & VRING_DESC_F_WRITE == 0 {
                    // Read-only buffer = data from guest
                    let data = read_guest_buf(mem_ptr, mem_size, desc.addr, desc.len as usize);
                    self.tx_output.extend_from_slice(&data);
                }
                if desc.flags & VRING_DESC_F_NEXT != 0 {
                    idx = desc.next as u64;
                } else {
                    break;
                }
            }

            // Add to used ring
            let used_idx = read_u16(mem_ptr, mem_size, q.device_addr + 2);
            let used_ring_off = (used_idx as u64 % q.num as u64) * 8 + 4;
            write_u32(mem_ptr, mem_size, q.device_addr + used_ring_off, desc_idx as u32);
            write_u32(mem_ptr, mem_size, q.device_addr + used_ring_off + 4, 0);
            write_u16(mem_ptr, mem_size, q.device_addr + 2, used_idx.wrapping_add(1));

            q.last_avail_idx = q.last_avail_idx.wrapping_add(1);
        }

        // Signal interrupt
        self.interrupt_status |= 1;
    }

    /// Deliver pending RX data to the guest via RX queue.
    pub fn deliver_rx(&mut self, mem_ptr: *mut u8, mem_size: usize) {
        if !self.rx_pending || self.rx_input.is_empty() { return; }

        let q = &mut self.queues[0]; // RX queue
        if !q.ready || q.num == 0 { return; }

        // Check if guest has posted buffers
        let avail_idx = read_u16(mem_ptr, mem_size, q.driver_addr + 2);
        if q.last_avail_idx == avail_idx { return; } // No buffers available

        // Get descriptor
        let ring_idx = (q.last_avail_idx as u64 % q.num as u64) * 2 + 4;
        let desc_idx = read_u16(mem_ptr, mem_size, q.driver_addr + ring_idx) as u64;
        let desc = read_desc(mem_ptr, mem_size, q.desc_addr, desc_idx);

        if desc.flags & VRING_DESC_F_WRITE == 0 { return; } // Not writable

        // Write data to guest buffer
        let len = self.rx_input.len().min(desc.len as usize);
        write_guest_buf(mem_ptr, mem_size, desc.addr, &self.rx_input[..len]);
        self.rx_input.drain(..len);
        if self.rx_input.is_empty() { self.rx_pending = false; }

        // Update used ring
        let used_idx = read_u16(mem_ptr, mem_size, q.device_addr + 2);
        let used_ring_off = (used_idx as u64 % q.num as u64) * 8 + 4;
        write_u32(mem_ptr, mem_size, q.device_addr + used_ring_off, desc_idx as u32);
        write_u32(mem_ptr, mem_size, q.device_addr + used_ring_off + 4, len as u32);
        write_u16(mem_ptr, mem_size, q.device_addr + 2, used_idx.wrapping_add(1));

        q.last_avail_idx = q.last_avail_idx.wrapping_add(1);

        // Signal interrupt
        self.interrupt_status |= 1;
    }

}

// Free functions for guest memory access (avoids borrow checker issues with &mut self + &self)
fn gpa_to_offset(gpa: u64) -> usize {
    (gpa - machine::RAM_BASE) as usize
}

fn read_desc(mem_ptr: *mut u8, mem_size: usize, desc_base: u64, idx: u64) -> VringDesc {
    let offset = gpa_to_offset(desc_base + idx * 16);
    if offset + 16 > mem_size { return VringDesc::default(); }
    unsafe {
        let ptr = mem_ptr.add(offset);
        VringDesc {
            addr: (ptr as *const u64).read_unaligned(),
            len: (ptr.add(8) as *const u32).read_unaligned(),
            flags: (ptr.add(12) as *const u16).read_unaligned(),
            next: (ptr.add(14) as *const u16).read_unaligned(),
        }
    }
}

fn read_guest_buf(mem_ptr: *mut u8, mem_size: usize, gpa: u64, len: usize) -> Vec<u8> {
    let offset = gpa_to_offset(gpa);
    if offset + len > mem_size { return Vec::new(); }
    unsafe {
        let src = mem_ptr.add(offset);
        std::slice::from_raw_parts(src, len).to_vec()
    }
}

fn write_guest_buf(mem_ptr: *mut u8, mem_size: usize, gpa: u64, data: &[u8]) {
    let offset = gpa_to_offset(gpa);
    if offset + data.len() > mem_size { return; }
    unsafe {
        let dst = mem_ptr.add(offset);
        std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
    }
}

fn read_u16(mem_ptr: *mut u8, mem_size: usize, gpa: u64) -> u16 {
    let offset = gpa_to_offset(gpa);
    if offset + 2 > mem_size { return 0; }
    unsafe { (mem_ptr.add(offset) as *const u16).read_unaligned() }
}

fn write_u16(mem_ptr: *mut u8, mem_size: usize, gpa: u64, val: u16) {
    let offset = gpa_to_offset(gpa);
    if offset + 2 > mem_size { return; }
    unsafe { (mem_ptr.add(offset) as *mut u16).write_unaligned(val); }
}

fn write_u32(mem_ptr: *mut u8, mem_size: usize, gpa: u64, val: u32) {
    let offset = gpa_to_offset(gpa);
    if offset + 4 > mem_size { return; }
    unsafe { (mem_ptr.add(offset) as *mut u32).write_unaligned(val); }
}
