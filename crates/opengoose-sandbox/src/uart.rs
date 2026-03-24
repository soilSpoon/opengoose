use std::collections::VecDeque;

pub const PL011_BASE: u64 = 0x0900_0000;
pub const PL011_SIZE: u64 = 0x1000;
pub const PL011_IRQ: u32 = 1;

const UARTDR: u64 = 0x000;
const UARTFR: u64 = 0x018;
const UARTIMSC: u64 = 0x038;
const UARTICR: u64 = 0x044;

const FR_RXFE: u64 = 1 << 4;
const FR_TXFE: u64 = 1 << 7;

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

    pub fn handle_mmio_write(&mut self, offset: u64, val: u64) {
        match offset {
            UARTDR => {
                let byte = val as u8;
                self.output.push(byte);
                self.output_line_buf.push(byte);
            }
            UARTIMSC => self.imsc = val,
            UARTICR => {}
            _ => {}
        }
    }

    pub fn handle_mmio_read(&mut self, offset: u64) -> u64 {
        match offset {
            UARTDR => self.input.pop_front().map(|b| b as u64).unwrap_or(0),
            UARTFR => {
                let mut flags = FR_TXFE;
                if self.input.is_empty() {
                    flags |= FR_RXFE;
                }
                flags
            }
            UARTIMSC => self.imsc,
            _ => 0,
        }
    }

    pub fn push_input(&mut self, data: &[u8]) {
        self.input.extend(data);
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
