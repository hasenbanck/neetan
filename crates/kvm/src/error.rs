//! Error type for the KVM wrapper.

/// Errors returned by the KVM wrapper.
#[derive(Debug)]
pub enum Error {
    /// Failed to open `/dev/kvm` or issue a KVM ioctl.
    #[cfg(target_os = "linux")]
    Kvm(kvm_ioctls::Error),
    /// Failed to allocate or map guest-backing host memory.
    Memory(std::io::Error),
    /// The `KVM_API_VERSION` exposed by the kernel does not match the version
    /// this crate was built against (12).
    ApiVersionMismatch {
        /// Version reported by the kernel.
        actual: i32,
    },
    /// The host does not support unrestricted-guest real-mode execution.
    UnrestrictedGuestUnavailable,
    /// The caller asked for more memory slots than the kernel supports.
    TooManySlots,
    /// The MSR list exceeded the KVM FAM limit.
    MsrListTooLong,
    /// The CPUID list exceeded the KVM FAM limit (`KVM_MAX_CPUID_ENTRIES`).
    CpuidListTooLong,
    /// Raw OS errno from an ioctl invoked directly with `libc::ioctl`.
    Os(std::io::Error),
    /// The KVM backend is not available on this platform.
    NotSupported,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(target_os = "linux")]
            Self::Kvm(error) => write!(f, "KVM error: {error}"),
            Self::Memory(error) => write!(f, "guest-memory allocation failed: {error}"),
            Self::ApiVersionMismatch { actual } => {
                write!(
                    f,
                    "KVM API version mismatch: kernel reports {actual}, expected 12"
                )
            }
            Self::UnrestrictedGuestUnavailable => f.write_str(
                "host CPU does not support unrestricted guest (needed for real-mode boot)",
            ),
            Self::TooManySlots => f.write_str("too many KVM memory slots requested"),
            Self::MsrListTooLong => f.write_str("MSR list exceeds KVM FAM limit"),
            Self::CpuidListTooLong => f.write_str("CPUID list exceeds KVM FAM limit"),
            Self::Os(error) => write!(f, "ioctl failed: {error}"),
            Self::NotSupported => f.write_str("KVM backend is not supported on this platform"),
        }
    }
}

impl std::error::Error for Error {}
