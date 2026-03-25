use opengoose_sandbox::uart::Pl011;

#[test]
fn test_write_and_read_output() {
    let mut uart = Pl011::new();
    uart.handle_mmio_write(0x000, b'H' as u64);
    uart.handle_mmio_write(0x000, b'i' as u64);
    assert_eq!(uart.take_output(), b"Hi");
    assert_eq!(uart.take_output(), b"");
}

#[test]
fn test_input_and_read() {
    let mut uart = Pl011::new();
    uart.push_input(b"OK\n");
    assert_eq!(uart.handle_mmio_read(0x000) as u8, b'O');
    assert_eq!(uart.handle_mmio_read(0x000) as u8, b'K');
    assert_eq!(uart.handle_mmio_read(0x000) as u8, b'\n');
}

#[test]
fn test_flag_register() {
    let mut uart = Pl011::new();
    let fr = uart.handle_mmio_read(0x018);
    assert_ne!(fr & (1 << 4), 0, "RXFE should be set when empty");
    assert_eq!(fr & (1 << 5), 0, "TXFF should be clear");
    uart.push_input(b"x");
    let fr = uart.handle_mmio_read(0x018);
    assert_eq!(fr & (1 << 4), 0, "RXFE should be clear when data available");
}

#[test]
fn test_read_line() {
    let mut uart = Pl011::new();
    for b in b"hello\nworld\n" {
        uart.handle_mmio_write(0x000, *b as u64);
    }
    assert_eq!(uart.read_line(), Some("hello".to_string()));
    assert_eq!(uart.read_line(), Some("world".to_string()));
    assert_eq!(uart.read_line(), None);
}
