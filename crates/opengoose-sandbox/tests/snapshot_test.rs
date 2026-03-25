use opengoose_sandbox::hypervisor::VcpuState;
use opengoose_sandbox::snapshot::{VmSnapshot, cow_map};

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
        gic_state: None,
        vtimer_offset: None,
        virtio_state: None,
    };
    let dir = tempfile::tempdir().unwrap();
    let meta_path = dir.path().join("snapshot.meta");
    let mem_path = dir.path().join("snapshot.mem");

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

    let original = vec![0xAA_u8; 4096];
    std::fs::write(&mem_path, &original).unwrap();

    let (ptr, size) = cow_map(&mem_path, 4096).unwrap();
    assert_eq!(size, 4096);

    let first_byte = unsafe { *ptr };
    assert_eq!(first_byte, 0xAA);

    unsafe {
        *ptr = 0xBB;
    }
    assert_eq!(unsafe { *ptr }, 0xBB);

    let file_content = std::fs::read(&mem_path).unwrap();
    assert_eq!(file_content[0], 0xAA, "original file must not be modified");

    unsafe {
        libc::munmap(ptr as *mut libc::c_void, size);
    }
}
