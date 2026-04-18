//! Headless Ra40 boot-progress probe.
//!
//! Runs the Ra40 machine for a bounded number of iterations without SDL,
//! sampling CS:IP and text VRAM at the KVM-mapped guest-physical address
//! `0xA0000` so we can see how far the HLE-BIOS cold boot has progressed.
//!
//! This is a diagnostic / milestone-tracking test; it always passes so the
//! full `cargo test` run stays green while boot work is in progress. Run
//! manually with `cargo test --features kvm -p machine --test ra40_boot_probe
//! -- --nocapture` to see the trace.

#![cfg(all(feature = "kvm", target_os = "linux"))]

use common::{Machine, MachineModel};
use machine::{Pc9801Bus, Pc9821Ra40};

/// Guest physical address of the first text-VRAM character cell.
const TEXT_VRAM_GPA: u64 = 0x000A_0000;
/// Number of characters to sample each iteration (first line = 80 chars,
/// each cell is 2 bytes: char code + attribute).
const SAMPLE_BYTES: usize = 80 * 2;

#[test]
fn ra40_boot_probe_traces_cs_ip_and_text_vram() {
    if !std::path::Path::new("/dev/kvm").exists() {
        eprintln!("skipping: /dev/kvm unavailable");
        return;
    }

    let bus: Pc9801Bus = Pc9801Bus::new(MachineModel::PC9821RA40, 48_000);
    let mut ra40 = match Pc9821Ra40::new(bus) {
        Ok(m) => m,
        Err(error) => {
            eprintln!("skipping: Pc9821Ra40::new failed: {error}");
            return;
        }
    };

    const SLICE_BUDGET: u64 = 2_000_000;
    const NUM_SLICES: usize = 16;

    eprintln!(
        "initial CS:IP = {:04X}:{:04X} (CS.base={:08X})",
        ra40.cs_ip_snapshot().0,
        ra40.cs_ip_snapshot().2,
        ra40.cs_ip_snapshot().1,
    );

    for iteration in 0..NUM_SLICES {
        let cycles = ra40.run_for(SLICE_BUDGET);
        let (cs, cs_base, ip) = ra40.cs_ip_snapshot();
        let vram = ra40
            .peek_guest_memory(TEXT_VRAM_GPA, SAMPLE_BYTES)
            .unwrap_or_default();
        let non_zero = vram.iter().filter(|&&b| b != 0).count();
        // First 16 character cells decoded as raw bytes (JIS, not decoded).
        let preview: String = vram
            .iter()
            .step_by(2)
            .take(32)
            .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
            .collect();
        eprintln!(
            "iter {iteration:2}: cycles={cycles:>7} CS:IP={cs:04X}:{ip:04X} CS.base={cs_base:08X} \
             VRAM_nonzero={non_zero:>3} preview={preview:?}",
        );
        if ra40.shutdown_requested() {
            eprintln!("shutdown_requested — stopping probe");
            break;
        }
    }
}
