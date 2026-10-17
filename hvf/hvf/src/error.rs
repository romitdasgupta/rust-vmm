// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Error types.

use std::{fmt, io};

use hvf_bindings::{
    HV_BAD_ARGUMENT, HV_BUSY, HV_DENIED, HV_ERROR, HV_EXISTS, HV_ILLEGAL_GUEST_STATE, HV_NO_DEVICE,
    HV_NO_RESOURCES, HV_SUCCESS, HV_UNSUPPORTED, hv_return_t,
};

/// `errno` values of the host, used by [`HvError::to_errno`].
mod errno {
    #[cfg(target_os = "macos")]
    pub(super) use libc::{EBUSY, EEXIST, EINVAL, EIO, ENODEV, ENOMEM, ENOTSUP, EPERM};

    // `libc` is only a dependency on macOS. These are the Linux values, which
    // is where the portable part of the crate is built and tested otherwise.
    // All but `ENOTSUP` are the same on every Unix.
    #[cfg(not(target_os = "macos"))]
    mod values {
        pub const EPERM: i32 = 1;
        pub const EIO: i32 = 5;
        pub const ENOMEM: i32 = 12;
        pub const EBUSY: i32 = 16;
        pub const EEXIST: i32 = 17;
        pub const ENODEV: i32 = 19;
        pub const EINVAL: i32 = 22;
        pub const ENOTSUP: i32 = 95;
    }
    #[cfg(not(target_os = "macos"))]
    pub(super) use values::*;
}

/// A non-success `hv_return_t` returned by Hypervisor.framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HvError(pub hv_return_t);

impl HvError {
    /// `HV_BUSY`: the owning resource is busy (for example a second vCPU on
    /// the same thread, or destroying a VM that still has vCPUs).
    pub fn is_busy(&self) -> bool {
        self.0 == HV_BUSY
    }

    /// `HV_DENIED`: the system does not allow the operation, usually because
    /// the binary lacks the `com.apple.security.hypervisor` entitlement.
    pub fn is_denied(&self) -> bool {
        self.0 == HV_DENIED
    }

    /// `HV_UNSUPPORTED`: the hypervisor does not support the operation.
    pub fn is_unsupported(&self) -> bool {
        self.0 == HV_UNSUPPORTED
    }

    /// `HV_BAD_ARGUMENT`: an argument is invalid (this includes calling a
    /// vCPU function from a thread that does not own the vCPU).
    pub fn is_bad_argument(&self) -> bool {
        self.0 == HV_BAD_ARGUMENT
    }

    /// `HV_DENIED`, `HV_UNSUPPORTED` or `HV_NO_DEVICE`: when `hv_vm_create`
    /// returns one of them, the hypervisor cannot be used on this host or by
    /// this binary.
    ///
    /// Only meaningful for `hv_vm_create`. Other calls return
    /// `HV_UNSUPPORTED` or `HV_NO_DEVICE` for configuration errors on a
    /// working host: an IPA size above the maximum, a GIC call on a VM
    /// without a GIC, or `Vcpu::wfi_time` without a GIC. Prefer [`Error::is_unavailable`], which checks the call.
    pub fn is_unavailable(&self) -> bool {
        matches!(self.0, HV_DENIED | HV_UNSUPPORTED | HV_NO_DEVICE)
    }

    /// The closest `errno` value: `HV_BUSY` maps to `EBUSY`,
    /// `HV_BAD_ARGUMENT` to `EINVAL`, `HV_DENIED` to `EPERM`,
    /// `HV_UNSUPPORTED` to `ENOTSUP`, `HV_NO_RESOURCES` to `ENOMEM`,
    /// `HV_NO_DEVICE` to `ENODEV`, `HV_EXISTS` to `EEXIST` and everything else
    /// to `EIO`.
    pub fn to_errno(&self) -> i32 {
        match self.0 {
            HV_BUSY => errno::EBUSY,
            HV_BAD_ARGUMENT => errno::EINVAL,
            HV_DENIED => errno::EPERM,
            HV_UNSUPPORTED => errno::ENOTSUP,
            HV_NO_RESOURCES => errno::ENOMEM,
            HV_NO_DEVICE => errno::ENODEV,
            HV_EXISTS => errno::EEXIST,
            _ => errno::EIO,
        }
    }

    /// The [`io::ErrorKind`] matching [`HvError::to_errno`].
    fn io_error_kind(&self) -> io::ErrorKind {
        match self.0 {
            HV_BUSY => io::ErrorKind::ResourceBusy,
            HV_BAD_ARGUMENT => io::ErrorKind::InvalidInput,
            HV_DENIED => io::ErrorKind::PermissionDenied,
            HV_UNSUPPORTED => io::ErrorKind::Unsupported,
            HV_NO_RESOURCES => io::ErrorKind::OutOfMemory,
            HV_NO_DEVICE => io::ErrorKind::NotFound,
            HV_EXISTS => io::ErrorKind::AlreadyExists,
            _ => io::ErrorKind::Other,
        }
    }

    /// The symbolic name and a short explanation of the code, if known.
    fn describe(&self) -> Option<(&'static str, &'static str)> {
        Some(match self.0 {
            HV_SUCCESS => ("HV_SUCCESS", "success"),
            HV_ERROR => ("HV_ERROR", "error"),
            HV_BUSY => ("HV_BUSY", "resource busy"),
            HV_BAD_ARGUMENT => (
                "HV_BAD_ARGUMENT",
                "invalid argument, or vCPU used from a thread that does not own it",
            ),
            HV_ILLEGAL_GUEST_STATE => ("HV_ILLEGAL_GUEST_STATE", "illegal guest state"),
            HV_NO_RESOURCES => ("HV_NO_RESOURCES", "out of resources"),
            HV_NO_DEVICE => ("HV_NO_DEVICE", "no such device (no VM or no GIC)"),
            HV_DENIED => ("HV_DENIED", "binary lacks com.apple.security.hypervisor"),
            HV_EXISTS => ("HV_EXISTS", "already exists"),
            HV_UNSUPPORTED => ("HV_UNSUPPORTED", "operation not supported"),
            _ => return None,
        })
    }
}

impl fmt::Display for HvError {
    /// Formats the symbolic name, the raw code and a short explanation, for
    /// example `HV_DENIED (0xfae94007): binary lacks
    /// com.apple.security.hypervisor`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The codes are conventionally written as unsigned hexadecimal.
        let code = self.0 as u32;
        match self.describe() {
            Some((name, text)) => write!(f, "{name} ({code:#x}): {text}"),
            None => write!(f, "unknown hv_return_t ({code:#x})"),
        }
    }
}

impl std::error::Error for HvError {}

/// Errors returned by this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A Hypervisor.framework function failed.
    Hv {
        /// Name of the failing C function, for example `"hv_vm_map"`.
        call: &'static str,
        /// The returned error code.
        err: HvError,
    },
    /// A function is not provided by the running macOS release.
    Unavailable {
        /// Name of the missing C function.
        api: &'static str,
        /// The first macOS release `(major, minor)` providing it.
        since: (u16, u16),
    },
    /// A VM already exists in this process (Hypervisor.framework supports one
    /// VM per process).
    VmExists,
    /// A value is not aligned as required.
    Misaligned {
        /// What the value is, for example `"ipa"`.
        what: &'static str,
        /// The offending value.
        value: u64,
        /// The required alignment.
        align: u64,
    },
    /// A value or range is out of the supported range (for example beyond
    /// the VM's IPA size, or overflowing).
    OutOfRange {
        /// What the value is, for example `"ipa + size"`.
        what: &'static str,
        /// The offending value.
        value: u64,
        /// The limit the value violates. This is an exclusive upper bound,
        /// except for the end of a range (`what` is `"... + size"`), which may
        /// equal it, and for values that must be at least a minimum (`what`
        /// says so), where it is that minimum.
        limit: u64,
    },
    /// Two guest physical regions overlap.
    Overlap {
        /// The first region, for example `"distributor"`.
        what: &'static str,
        /// The region it overlaps with.
        other: &'static str,
    },
    /// The GIC must be created before the first vCPU.
    GicAfterVcpu,
    /// The VM already has a GIC.
    GicExists,
    /// The register index is not valid.
    InvalidRegister(u32),
    /// A `*_create()` function returned a null object.
    NullObject(&'static str),
}

impl Error {
    /// Whether `Hvf::create_vm` failed because Hypervisor.framework cannot be
    /// used on this host or by this binary: `hv_vm_create` returned
    /// `HV_DENIED` (the binary lacks the `com.apple.security.hypervisor`
    /// entitlement), `HV_UNSUPPORTED` or `HV_NO_DEVICE` (Intel Mac, or a VM
    /// without nested virtualization).
    ///
    /// False for every other error, including the `HV_UNSUPPORTED` and
    /// `HV_NO_DEVICE` that other calls return for configuration errors (see
    /// [`HvError::is_unavailable`]).
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Error::Hv { call: "hv_vm_create", err } if err.is_unavailable())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Hv { call, err } => write!(f, "{call} failed: {err}"),
            Error::Unavailable {
                api,
                since: (major, minor),
            } => write!(
                f,
                "{api} is not available on this system (requires macOS {major}.{minor} or newer)"
            ),
            Error::VmExists => write!(f, "a VM already exists in this process"),
            Error::Misaligned { what, value, align } => {
                write!(f, "{what} ({value:#x}) is not aligned to {align:#x}")
            }
            Error::OutOfRange { what, value, limit } => {
                write!(f, "{what} ({value:#x}) is out of range (limit {limit:#x})")
            }
            Error::Overlap { what, other } => write!(f, "the {what} region overlaps the {other}"),
            Error::GicAfterVcpu => write!(f, "the GIC must be created before the first vCPU"),
            Error::GicExists => write!(f, "the VM already has a GIC"),
            Error::InvalidRegister(reg) => write!(f, "invalid register {reg}"),
            Error::NullObject(call) => write!(f, "{call} returned a null object"),
        }
    }
}

// The `Display` of `Error::Hv` already includes the `HvError`, so `source()`
// does not return it as well: error-chain reporters would print it twice.
// Match `Error::Hv { err, .. }` to get the code.
impl std::error::Error for Error {}

impl From<Error> for io::Error {
    /// Converts to an [`io::Error`] that keeps `e` as its inner error. The
    /// [`io::ErrorKind`] of an [`Error::Hv`] corresponds to
    /// [`HvError::to_errno`] (`ENODEV` becomes [`io::ErrorKind::NotFound`],
    /// `EIO` becomes [`io::ErrorKind::Other`]).
    fn from(e: Error) -> Self {
        let kind = match &e {
            Error::Hv { err, .. } => err.io_error_kind(),
            Error::Unavailable { .. } => io::ErrorKind::Unsupported,
            Error::VmExists | Error::GicExists => io::ErrorKind::AlreadyExists,
            Error::Misaligned { .. }
            | Error::OutOfRange { .. }
            | Error::Overlap { .. }
            | Error::GicAfterVcpu
            | Error::InvalidRegister(_) => io::ErrorKind::InvalidInput,
            Error::NullObject(_) => io::ErrorKind::OutOfMemory,
        };
        io::Error::new(kind, e)
    }
}

/// A specialized `Result` type for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Converts the return value of the C function `call` into a `Result`.
// Only used by the macOS/aarch64 modules.
#[allow(dead_code)]
pub(crate) fn check(call: &'static str, r: hv_return_t) -> Result<()> {
    if r == HV_SUCCESS {
        Ok(())
    } else {
        Err(Error::Hv {
            call,
            err: HvError(r),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CODES: [hv_return_t; 10] = [
        HV_SUCCESS,
        HV_ERROR,
        HV_BUSY,
        HV_BAD_ARGUMENT,
        HV_ILLEGAL_GUEST_STATE,
        HV_NO_RESOURCES,
        HV_NO_DEVICE,
        HV_DENIED,
        HV_EXISTS,
        HV_UNSUPPORTED,
    ];

    #[test]
    fn test_predicates() {
        for code in ALL_CODES {
            let e = HvError(code);
            assert_eq!(e.is_busy(), code == HV_BUSY);
            assert_eq!(e.is_denied(), code == HV_DENIED);
            assert_eq!(e.is_unsupported(), code == HV_UNSUPPORTED);
            assert_eq!(e.is_bad_argument(), code == HV_BAD_ARGUMENT);
            assert_eq!(
                e.is_unavailable(),
                [HV_DENIED, HV_UNSUPPORTED, HV_NO_DEVICE].contains(&code)
            );
        }
    }

    #[test]
    fn test_error_is_unavailable() {
        let hv = |call, code| Error::Hv {
            call,
            err: HvError(code),
        };
        for code in [HV_DENIED, HV_UNSUPPORTED, HV_NO_DEVICE] {
            assert!(hv("hv_vm_create", code).is_unavailable(), "{code:#x}");
        }
        for code in [HV_BUSY, HV_ERROR, HV_BAD_ARGUMENT, HV_NO_RESOURCES] {
            assert!(!hv("hv_vm_create", code).is_unavailable(), "{code:#x}");
        }
        // Configuration errors on a working host, not an unusable hypervisor.
        for (call, code) in [
            ("hv_vm_config_set_ipa_size", HV_UNSUPPORTED),
            ("hv_gic_get_redistributor_base", HV_NO_DEVICE),
            ("hv_vcpu_get_wait_for_interrupt_time", HV_UNSUPPORTED),
            ("hv_vcpu_set_pending_interrupt", HV_UNSUPPORTED),
        ] {
            assert!(!hv(call, code).is_unavailable(), "{call}");
        }
        assert!(
            !Error::Unavailable {
                api: "hv_gic_create",
                since: (15, 0),
            }
            .is_unavailable()
        );
        assert!(!Error::VmExists.is_unavailable());
    }

    #[test]
    fn test_to_errno() {
        let cases = [
            (HV_BUSY, errno::EBUSY),
            (HV_BAD_ARGUMENT, errno::EINVAL),
            (HV_DENIED, errno::EPERM),
            (HV_UNSUPPORTED, errno::ENOTSUP),
            (HV_NO_RESOURCES, errno::ENOMEM),
            (HV_NO_DEVICE, errno::ENODEV),
            (HV_EXISTS, errno::EEXIST),
            (HV_ERROR, errno::EIO),
            (HV_ILLEGAL_GUEST_STATE, errno::EIO),
            (0x1234, errno::EIO),
        ];
        for (code, errno) in cases {
            assert_eq!(HvError(code).to_errno(), errno, "{code:#x}");
        }
        // Spot-check the host values.
        assert_eq!(HvError(HV_BUSY).to_errno(), 16);
        assert_eq!(HvError(HV_BAD_ARGUMENT).to_errno(), 22);
        #[cfg(target_os = "macos")]
        assert_eq!(HvError(HV_UNSUPPORTED).to_errno(), 45);
        #[cfg(target_os = "linux")]
        assert_eq!(HvError(HV_UNSUPPORTED).to_errno(), 95);
    }

    #[test]
    fn test_hv_error_display() {
        assert_eq!(
            HvError(HV_DENIED).to_string(),
            "HV_DENIED (0xfae94007): binary lacks com.apple.security.hypervisor"
        );
        assert_eq!(
            HvError(HV_BUSY).to_string(),
            "HV_BUSY (0xfae94002): resource busy"
        );
        assert_eq!(
            HvError(HV_UNSUPPORTED).to_string(),
            "HV_UNSUPPORTED (0xfae9400f): operation not supported"
        );
        assert_eq!(
            HvError(0xfae9_4009_u32 as i32).to_string(),
            "unknown hv_return_t (0xfae94009)"
        );
        for code in ALL_CODES {
            let s = HvError(code).to_string();
            assert!(s.starts_with("HV_"), "{s}");
            assert!(s.contains(&format!("{:#x}", code as u32)), "{s}");
        }
    }

    #[test]
    fn test_error_display() {
        let cases = [
            (
                Error::Hv {
                    call: "hv_vm_map",
                    err: HvError(HV_BAD_ARGUMENT),
                },
                "hv_vm_map failed: HV_BAD_ARGUMENT (0xfae94003): invalid argument, \
                 or vCPU used from a thread that does not own it",
            ),
            (
                Error::Unavailable {
                    api: "hv_gic_create",
                    since: (15, 0),
                },
                "hv_gic_create is not available on this system (requires macOS 15.0 or newer)",
            ),
            (Error::VmExists, "a VM already exists in this process"),
            (
                Error::Misaligned {
                    what: "ipa",
                    value: 0x1000,
                    align: 0x4000,
                },
                "ipa (0x1000) is not aligned to 0x4000",
            ),
            (
                Error::OutOfRange {
                    what: "ipa + size",
                    value: 0x100_0000_0000,
                    limit: 0x10_0000_0000,
                },
                "ipa + size (0x10000000000) is out of range (limit 0x1000000000)",
            ),
            (
                Error::Overlap {
                    what: "distributor",
                    other: "redistributor",
                },
                "the distributor region overlaps the redistributor",
            ),
            (
                Error::GicAfterVcpu,
                "the GIC must be created before the first vCPU",
            ),
            (Error::GicExists, "the VM already has a GIC"),
            (Error::InvalidRegister(31), "invalid register 31"),
            (
                Error::NullObject("hv_vm_config_create"),
                "hv_vm_config_create returned a null object",
            ),
        ];
        for (e, s) in cases {
            assert_eq!(e.to_string(), s);
        }
    }

    #[test]
    fn test_error_source() {
        use std::error::Error as _;

        // The code is part of the message, so it is not also the source,
        // which a chain reporter would print a second time.
        let e = Error::Hv {
            call: "hv_vcpu_run",
            err: HvError(HV_ERROR),
        };
        assert!(e.source().is_none());
        assert!(e.to_string().contains(&HvError(HV_ERROR).to_string()));
        assert!(io::Error::from(e).source().is_none());
        assert!(Error::VmExists.source().is_none());
    }

    #[test]
    fn test_into_io_error() {
        let cases = [
            (
                Error::Hv {
                    call: "hv_vcpu_create",
                    err: HvError(HV_BUSY),
                },
                io::ErrorKind::ResourceBusy,
            ),
            (
                Error::Hv {
                    call: "hv_vm_create",
                    err: HvError(HV_DENIED),
                },
                io::ErrorKind::PermissionDenied,
            ),
            (
                Error::Hv {
                    call: "hv_vm_map",
                    err: HvError(HV_BAD_ARGUMENT),
                },
                io::ErrorKind::InvalidInput,
            ),
            (
                Error::Hv {
                    call: "hv_vcpu_set_pending_interrupt",
                    err: HvError(HV_UNSUPPORTED),
                },
                io::ErrorKind::Unsupported,
            ),
            (
                Error::Hv {
                    call: "hv_vm_create",
                    err: HvError(HV_NO_RESOURCES),
                },
                io::ErrorKind::OutOfMemory,
            ),
            (
                Error::Hv {
                    call: "hv_vm_create",
                    err: HvError(HV_EXISTS),
                },
                io::ErrorKind::AlreadyExists,
            ),
            (
                Error::Hv {
                    call: "hv_vm_create",
                    err: HvError(HV_NO_DEVICE),
                },
                io::ErrorKind::NotFound,
            ),
            (
                Error::Hv {
                    call: "hv_vcpu_run",
                    err: HvError(HV_ERROR),
                },
                io::ErrorKind::Other,
            ),
            (
                Error::Unavailable {
                    api: "hv_vcpu_get_wfi_time",
                    since: (27, 0),
                },
                io::ErrorKind::Unsupported,
            ),
            (Error::VmExists, io::ErrorKind::AlreadyExists),
            (Error::GicExists, io::ErrorKind::AlreadyExists),
            (Error::GicAfterVcpu, io::ErrorKind::InvalidInput),
            (
                Error::Misaligned {
                    what: "size",
                    value: 1,
                    align: 0x4000,
                },
                io::ErrorKind::InvalidInput,
            ),
            (
                Error::OutOfRange {
                    what: "ipa",
                    value: 1 << 40,
                    limit: 1 << 36,
                },
                io::ErrorKind::InvalidInput,
            ),
            (
                Error::Overlap {
                    what: "msi",
                    other: "distributor",
                },
                io::ErrorKind::InvalidInput,
            ),
            (Error::InvalidRegister(32), io::ErrorKind::InvalidInput),
            (
                Error::NullObject("hv_gic_config_create"),
                io::ErrorKind::OutOfMemory,
            ),
        ];
        for (e, kind) in cases {
            let msg = e.to_string();
            let io_err = io::Error::from(e.clone());
            assert_eq!(io_err.kind(), kind, "{msg}");
            assert_eq!(io_err.to_string(), msg);
            let inner = io_err.into_inner().unwrap().downcast::<Error>().unwrap();
            assert_eq!(*inner, e);
        }
    }

    #[test]
    fn test_check() {
        assert_eq!(check("hv_vm_create", HV_SUCCESS), Ok(()));
        assert_eq!(
            check("hv_vm_create", HV_DENIED),
            Err(Error::Hv {
                call: "hv_vm_create",
                err: HvError(HV_DENIED),
            })
        );
    }
}
