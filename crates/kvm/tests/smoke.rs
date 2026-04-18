//! Smoke test: run a one-byte real-mode HLT program through KVM.
//!
//! Gated to Linux. Skips (passes) if `/dev/kvm` is not accessible.

#![cfg(target_os = "linux")]

use kvm::{HostMemory, KvmSystem, VmExit};

#[test]
fn real_mode_hlt_returns_via_kvm_hlt_exit() {
    let kvm_system = match KvmSystem::open() {
        Ok(system) => system,
        Err(error) => {
            eprintln!("skipping: /dev/kvm unavailable: {error}");
            return;
        }
    };

    let mut vm = kvm_system.create_vm().expect("create_vm");

    // Intel hosts require a TSS address before the first vCPU entry in real
    // mode. Place it at a conventional hole address that is outside the small
    // guest mmap used below.
    vm.set_tss_address(0xFFFB_D000).expect("set_tss_address");

    // Allocate 64 KiB of guest RAM, write a single HLT (0xF4) at offset 0.
    const GUEST_SIZE: usize = 0x1_0000;
    let mut memory = HostMemory::new(GUEST_SIZE).expect("HostMemory::new");
    memory.as_mut_slice()[0] = 0xF4;
    vm.register_ram_slot(0, &mut memory, 0, GUEST_SIZE)
        .expect("register_ram_slot");

    let mut vcpu = vm.create_vcpu(0).expect("create_vcpu");

    // Start execution in real mode at CS:IP = 0000:0000 with a clean RFLAGS.
    let mut sregs = vcpu.get_sregs().expect("get_sregs");
    sregs.cs.base = 0;
    sregs.cs.selector = 0;
    vcpu.set_sregs(&sregs).expect("set_sregs");

    let mut regs = vcpu.get_regs().expect("get_regs");
    regs.rip = 0;
    regs.rflags = 0x2;
    vcpu.set_regs(&regs).expect("set_regs");

    let exit = vcpu.run().expect("KVM_RUN");
    assert!(
        matches!(exit, VmExit::Hlt),
        "expected VmExit::Hlt, got {exit:?}",
    );
}
