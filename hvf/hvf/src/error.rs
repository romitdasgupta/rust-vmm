// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Error types.

use std::fmt;

use hvf_bindings::hv_return_t;

/// A non-success `hv_return_t` returned by Hypervisor.framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HvError(pub hv_return_t);

impl HvError {
    /// `HV_BUSY`: the owning resource is busy (for example a second vCPU on
    /// the same thread, or destroying a VM that still has vCPUs).
    pub fn is_busy(&self) -> bool {
        todo!()
    }

    /// `HV_DENIED`: the system does not allow the operation, usually because
    /// the binary lacks the `com.apple.security.hypervisor` entitlement.
    pub fn is_denied(&self) -> bool {
        todo!()
    }

    /// `HV_UNSUPPORTED`: the hypervisor does not support the operation.
    pub fn is_unsupported(&self) -> bool {
        todo!()
    }

    /// `HV_BAD_ARGUMENT`: an argument is invalid (this includes calling a
    /// vCPU function from a thread that does not own the vCPU).
    pub fn is_bad_argument(&self) -> bool {
        todo!()
    }

    /// Whether the error means the hypervisor cannot be used at all on this
    /// host or by this binary: `HV_DENIED`, `HV_UNSUPPORTED` or
    /// `HV_NO_DEVICE`.
    pub fn is_unavailable(&self) -> bool {
        todo!()
    }

    /// The closest `errno` value: `HV_BUSY` maps to `EBUSY`,
    /// `HV_BAD_ARGUMENT` to `EINVAL`, `HV_DENIED` to `EPERM`,
    /// `HV_UNSUPPORTED` to `ENOTSUP`, `HV_NO_RESOURCES` to `ENOMEM`,
    /// `HV_NO_DEVICE` to `ENODEV`, `HV_EXISTS` to `EEXIST` and everything else
    /// to `EIO`.
    pub fn to_errno(&self) -> i32 {
        todo!()
    }
}

impl fmt::Display for HvError {
    /// Formats the symbolic name, the raw code and a short explanation, for
    /// example `HV_DENIED (0xfae94007): binary lacks
    /// com.apple.security.hypervisor`.
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
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
        /// The exclusive upper limit.
        limit: u64,
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

impl fmt::Display for Error {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for Error {}

impl From<Error> for std::io::Error {
    fn from(_e: Error) -> Self {
        todo!()
    }
}

/// A specialized `Result` type for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Converts the return value of the C function `call` into a `Result`.
#[allow(dead_code)]
pub(crate) fn check(_call: &'static str, _r: hv_return_t) -> Result<()> {
    todo!()
}
