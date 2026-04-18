//! Pentium II CPUID rewrite.
//!
//! Takes the host-supplied CPUID leaves (via
//! [`KvmSystem::supported_cpuid`](crate::KvmSystem::supported_cpuid)) and
//! produces a Pentium II feature view. Guest code then sees:
//!
//! - Vendor: "GenuineIntel" (passthrough from the host; KVM always reports
//!   this on Intel hosts).
//! - Signature (leaf 1 EAX): family 6, model 3 ("Klamath" Pentium II),
//!   stepping 1. This identifies the CPU to PC-98 BIOSes and Windows as a
//!   Pentium II, which matches the Ra40's real hardware.
//! - Features (leaf 1 EDX): Pentium II baseline = FPU, VME, DE, PSE, TSC,
//!   MSR, PAE, MCE, CX8, APIC, SEP, MTRR, PGE, MCA, CMOV, PAT, PSE36, MMX,
//!   FXSR. SSE and everything later are masked off, since the Ra40 shipped
//!   with Pentium II (pre-SSE).
//! - Leaf 1 ECX: cleared. Pentium II does not report ECX features.

use kvm_bindings::{CpuId, KVM_MAX_CPUID_ENTRIES, kvm_cpuid_entry2};

use crate::error::Error;

/// Pentium II family/model/stepping packed into CPUID leaf 1 EAX.
///
/// EAX bit layout:
///   `[reserved:4][ext family:8][ext model:4][type:2][reserved:2]
///    [family:4][model:4][stepping:4]`
/// Pentium II has ext family = 0, ext model = 0, type = 0, family = 6,
/// model = 3 (Klamath), stepping = 1.
const PENTIUM2_SIGNATURE_EAX: u32 = 0x0000_0631;

/// Pentium II baseline leaf-1 EDX features.
const PENTIUM2_LEAF1_EDX_KEEP_MASK: u32 = FEATURE_FPU
    | FEATURE_VME
    | FEATURE_DE
    | FEATURE_PSE
    | FEATURE_TSC
    | FEATURE_MSR
    | FEATURE_PAE
    | FEATURE_MCE
    | FEATURE_CX8
    | FEATURE_APIC
    | FEATURE_SEP
    | FEATURE_MTRR
    | FEATURE_PGE
    | FEATURE_MCA
    | FEATURE_CMOV
    | FEATURE_PAT
    | FEATURE_PSE36
    | FEATURE_MMX
    | FEATURE_FXSR;

const FEATURE_FPU: u32 = 1 << 0;
const FEATURE_VME: u32 = 1 << 1;
const FEATURE_DE: u32 = 1 << 2;
const FEATURE_PSE: u32 = 1 << 3;
const FEATURE_TSC: u32 = 1 << 4;
const FEATURE_MSR: u32 = 1 << 5;
const FEATURE_PAE: u32 = 1 << 6;
const FEATURE_MCE: u32 = 1 << 7;
const FEATURE_CX8: u32 = 1 << 8;
const FEATURE_APIC: u32 = 1 << 9;
const FEATURE_SEP: u32 = 1 << 11;
const FEATURE_MTRR: u32 = 1 << 12;
const FEATURE_PGE: u32 = 1 << 13;
const FEATURE_MCA: u32 = 1 << 14;
const FEATURE_CMOV: u32 = 1 << 15;
const FEATURE_PAT: u32 = 1 << 16;
const FEATURE_PSE36: u32 = 1 << 17;
const FEATURE_MMX: u32 = 1 << 23;
const FEATURE_FXSR: u32 = 1 << 24;

/// Owning wrapper around a KVM CPUID FAM (flexible-array-member) struct.
///
/// Thin newtype around `kvm_bindings::CpuId` that guarantees the entries are
/// non-empty and lets us pass it into [`KvmVcpu::set_cpuid2`](crate::KvmVcpu::set_cpuid2).
pub struct CpuidEntries {
    inner: CpuId,
}

impl CpuidEntries {
    /// Returns a borrow of the backing FAM, suitable for
    /// [`KvmVcpu::set_cpuid2`](crate::KvmVcpu::set_cpuid2).
    pub fn as_raw(&self) -> &CpuId {
        &self.inner
    }
}

/// Builds a Pentium II CPUID view from the host-supported leaves.
///
/// Call [`KvmSystem::supported_cpuid`](crate::KvmSystem::supported_cpuid) first
/// and pass the result in as `host`. The returned value is ready to be applied
/// via [`KvmVcpu::set_cpuid2`](crate::KvmVcpu::set_cpuid2).
pub fn pentium2_cpuid(host: &CpuId) -> Result<CpuidEntries, Error> {
    let host_entries = host.as_slice();
    if host_entries.len() > KVM_MAX_CPUID_ENTRIES {
        return Err(Error::CpuidListTooLong);
    }
    let rewritten: Vec<kvm_cpuid_entry2> = host_entries
        .iter()
        .map(|entry| {
            let mut copy = *entry;
            rewrite_entry(&mut copy);
            copy
        })
        .collect();
    let inner = CpuId::from_entries(&rewritten).map_err(|_| Error::CpuidListTooLong)?;
    Ok(CpuidEntries { inner })
}

fn rewrite_entry(entry: &mut kvm_cpuid_entry2) {
    match entry.function {
        0x0000_0000 => {
            // Leaf 0: max basic leaf + vendor string.
            // Clamp max basic leaf to 2 (Pentium II exposes 0, 1, 2). Higher
            // leaves are Pentium III/4 and beyond.
            if entry.eax > 2 {
                entry.eax = 2;
            }
        }
        0x0000_0001 => {
            // Leaf 1: signature + features.
            entry.eax = PENTIUM2_SIGNATURE_EAX;
            // EBX: keep CLFLUSH size, logical-count and brand index as host
            // reports but zero out APIC ID (PC-98 Ra40 is single-CPU POC).
            entry.ebx &= 0x0000_FFFF;
            // ECX: clear. Pentium II has no ECX features.
            entry.ecx = 0;
            // EDX: keep only Pentium II baseline features.
            entry.edx &= PENTIUM2_LEAF1_EDX_KEEP_MASK;
            // Guarantee the baseline bits the guest expects are set even if
            // the host masked some.
            entry.edx |= FEATURE_FPU | FEATURE_TSC | FEATURE_MSR | FEATURE_CX8 | FEATURE_MMX;
        }
        0x0000_0002 => {
            // Leaf 2: cache/TLB descriptors. Passthrough is fine.
        }
        0x0000_0003..=0x7FFF_FFFF | 0xC000_0000..=u32::MAX => {
            // Leaves above 2 are Pentium III and later. Zero them out so the
            // guest observes an authentic Pentium II surface.
            entry.eax = 0;
            entry.ebx = 0;
            entry.ecx = 0;
            entry.edx = 0;
        }
        0x8000_0000 => {
            // Extended leaf 0: clamp max extended leaf to 0 (Pentium II has
            // no useful extended leaves for guests to query).
            entry.eax = 0x8000_0000;
            entry.ebx = 0;
            entry.ecx = 0;
            entry.edx = 0;
        }
        0x8000_0001..=0xBFFF_FFFF => {
            // Extended leaves above 0x80000000 are AMD/Intel late-model
            // extensions; zero them.
            entry.eax = 0;
            entry.ebx = 0;
            entry.ecx = 0;
            entry.edx = 0;
        }
    }
}
