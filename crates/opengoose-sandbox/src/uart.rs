use std::collections::VecDeque;

pub const PL011_BASE: u64 = 0x0900_0000;
pub const PL011_SIZE: u64 = 0x1000;
pub const PL011_IRQ: u32 = 1;

// Register offsets
const UARTDR: u64 = 0x000;
const UARTFR: u64 = 0x018;
const UARTCR: u64 = 0x030;
const UARTIFLS: u64 = 0x034;
const UARTIMSC: u64 = 0x038;
const UARTRIS: u64 = 0x03C;
const UARTMIS: u64 = 0x040;
const UARTICR: u64 = 0x044;

// Flag register bits
const FR_RXFE: u64 = 1 << 4;  // RX FIFO empty
const FR_TXFE: u64 = 1 << 7;  // TX FIFO empty

// Interrupt bits
const INT_RX: u64 = 1 << 4;   // RX interrupt
const INT_TX: u64 = 1 << 5;   // TX interrupt

// PL011 identification registers (PeriphID and PrimeCellID)
// These must return correct values for the Linux AMBA bus driver to recognize PL011.
const UARTPERIPHID0: u64 = 0xFE0; // PartNumber[7:0] = 0x11
const UARTPERIPHID1: u64 = 0xFE4; // Designer[3:0]:PartNumber[11:8] = 0x10
const UARTPERIPHID2: u64 = 0xFE8; // Revision:Designer[7:4] = 0x34 (rev 3, ARM)
const UARTPERIPHID3: u64 = 0xFEC; // Configuration = 0x00
const UARTPCELLID0: u64 = 0xFF0;  // 0x0D
const UARTPCELLID1: u64 = 0xFF4;  // 0xF0
const UARTPCELLID2: u64 = 0xFF8;  // 0x05
const UARTPCELLID3: u64 = 0xFFC;  // 0xB1

pub struct Pl011 {
    input: VecDeque<u8>,
    output: Vec<u8>,
    output_line_buf: Vec<u8>,
    imsc: u64,          // Interrupt Mask Set/Clear
    ris: u64,           // Raw Interrupt Status
    cr: u64,            // Control Register
    ifls: u64,          // Interrupt FIFO Level Select
}

impl Pl011 {
    pub fn new() -> Self {
        Pl011 {
            input: VecDeque::new(),
            output: Vec::new(),
            output_line_buf: Vec::new(),
            imsc: 0,
            ris: INT_TX, // TX FIFO is always empty → TX raw interrupt always set
            cr: 0x0300,  // TXE + RXE enabled by default
            ifls: 0,
        }
    }

    pub fn handle_mmio_write(&mut self, offset: u64, val: u64) {
        match offset {
            UARTDR => {
                let byte = val as u8;
                self.output.push(byte);
                self.output_line_buf.push(byte);
                // TX FIFO is instantly "empty" since we consume immediately
                self.ris |= INT_TX;
            }
            UARTCR => self.cr = val,
            UARTIFLS => self.ifls = val,
            UARTIMSC => self.imsc = val,
            UARTICR => {
                // Clear specified raw interrupt bits
                self.ris &= !val;
                // TX interrupt re-asserts immediately (FIFO always empty)
                self.ris |= INT_TX;
            }
            _ => {}
        }
    }

    pub fn handle_mmio_read(&mut self, offset: u64) -> u64 {
        match offset {
            UARTDR => {
                let val = self.input.pop_front().map(|b| b as u64).unwrap_or(0);
                if self.input.is_empty() {
                    self.ris &= !INT_RX;
                }
                val
            }
            UARTFR => {
                let mut flags = FR_TXFE; // TX FIFO always empty
                if self.input.is_empty() {
                    flags |= FR_RXFE;
                }
                flags
            }
            UARTCR => self.cr,
            UARTIFLS => self.ifls,
            UARTIMSC => self.imsc,
            UARTRIS => self.ris,
            UARTMIS => self.ris & self.imsc, // Masked Interrupt Status
            // PL011 identification registers — required for AMBA driver probe
            UARTPERIPHID0 => 0x11,
            UARTPERIPHID1 => 0x10,
            UARTPERIPHID2 => 0x34,  // revision 3, designer ARM
            UARTPERIPHID3 => 0x00,
            UARTPCELLID0 => 0x0D,
            UARTPCELLID1 => 0xF0,
            UARTPCELLID2 => 0x05,
            UARTPCELLID3 => 0xB1,
            _ => 0,
        }
    }

    /// Returns true if the PL011 interrupt line should be asserted.
    pub fn irq_pending(&self) -> bool {
        (self.ris & self.imsc) != 0
    }

    pub fn push_input(&mut self, data: &[u8]) {
        self.input.extend(data);
        if !self.input.is_empty() {
            self.ris |= INT_RX;
        }
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    pub fn read_line(&mut self) -> Option<String> {
        if let Some(pos) = self.output_line_buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.output_line_buf.drain(..=pos).collect();
            let s = String::from_utf8_lossy(&line[..line.len() - 1]).to_string();
            Some(s)
        } else {
            None
        }
    }

    pub fn has_pending_input(&self) -> bool {
        !self.input.is_empty()
    }
}
