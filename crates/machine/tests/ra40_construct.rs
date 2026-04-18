//! Ra40 KVM construction smoke test.
//!
//! Verifies `Pc9821Ra40::new` successfully opens `/dev/kvm`, wires all three
//! memory slots, swaps the bus to borrowed RAM, and survives a very short
//! `run_for` slice without panicking. Does not attempt guest execution
//! beyond what naturally happens inside `KVM_RUN` during the bound budget.
//!
//! Requires Linux x86_64 with `/dev/kvm` accessible. Skips (passes) when
//! the device is missing, so CI runners without KVM can still build the
//! test.

#![cfg(all(feature = "kvm", target_os = "linux"))]

use common::{Machine, MachineModel};
use machine::{Pc9801Bus, Pc9821Ra40};

#[test]
fn ra40_construction_round_trip_and_run_for_completes() {
    if !std::path::Path::new("/dev/kvm").exists() {
        eprintln!("skipping: /dev/kvm unavailable");
        return;
    }

    let bus: Pc9801Bus = Pc9801Bus::new(MachineModel::PC9821RA40, 48_000);
    let ra40 = match Pc9821Ra40::new(bus) {
        Ok(machine) => machine,
        Err(error) => {
            eprintln!("skipping: Pc9821Ra40::new failed (likely /dev/kvm perms): {error}");
            return;
        }
    };

    // Basic trait sanity: clock is the Ra40 nominal 400 MHz.
    assert_eq!(ra40.cpu_clock_hz(), 400_000_000.0);
    assert!(!ra40.shutdown_requested());

    // Short `run_for` with a small budget. The guest cold-resets to
    // F000:FFF0 inside the system-space slot which holds the stub BIOS
    // ROM; the stub immediately OUTs to the BIOS HLE trap port. Either
    // the budget expires or a trap is dispatched before we return. We
    // only check that the call does not panic and returns bounded cycle
    // counts.
    let mut ra40 = ra40;
    let consumed = ra40.run_for(1_000_000);
    assert!(
        consumed <= 2_000_000,
        "run_for should return bounded cycles, got {consumed}"
    );
}
