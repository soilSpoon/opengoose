use opengoose_sandbox::hypervisor::*;
#[cfg(target_os = "macos")]
use opengoose_sandbox::hypervisor::hvf::HvfHypervisor;
use serial_test::serial;

/// Return the host page size (4 KiB on x86-64, 16 KiB on ARM64 macOS).
#[cfg(target_os = "macos")]
fn host_page_size() -> usize {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

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

    // Allocate one host page so HVF accepts the mapping (ARM64 macOS: 16 KiB).
    let page_size = host_page_size();
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

    let page_size = host_page_size();
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
