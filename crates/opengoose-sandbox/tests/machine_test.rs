use opengoose_sandbox::machine;

#[test]
fn test_memory_map_constants() {
    assert!(machine::GIC_DIST_ADDR < machine::RAM_BASE);
    assert!(machine::UART_ADDR < machine::RAM_BASE);
    assert_eq!(machine::RAM_BASE % 4096, 0);
}

#[test]
fn test_create_dtb() {
    let dtb = machine::create_dtb(128 * 1024 * 1024).expect("create DTB");
    assert_eq!(&dtb[0..4], &[0xD0, 0x0D, 0xFE, 0xED]);
    assert!(dtb.len() < 65536);
    assert!(dtb.len() > 100);
}

#[test]
fn test_dtb_addr_placement() {
    let ram_size: u64 = 128 * 1024 * 1024;
    let kernel_end: u64 = machine::RAM_BASE + 0x100_0000;
    let dtb_addr = machine::dtb_addr(kernel_end);
    assert!(dtb_addr >= kernel_end);
    assert_eq!(dtb_addr % 4096, 0);
    assert!(dtb_addr < machine::RAM_BASE + ram_size);
}
