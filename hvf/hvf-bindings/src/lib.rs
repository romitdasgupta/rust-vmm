// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

#![cfg_attr(not(test), no_std)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![allow(non_camel_case_types, non_upper_case_globals)]
// `hv_fns!` munches one declaration per recursion step.
#![recursion_limit = "512"]

//! Raw FFI bindings to the Apple silicon (arm64) interface of macOS
//! Hypervisor.framework.
//!
//! The bindings are written by hand from the SDK headers. Every item has the
//! same name as in the SDK:
//!
//! * `OS_ENUM` types are integer type aliases (for example [`hv_reg_t`]) with
//!   one `pub const` per value, because Apple adds values in new releases.
//! * `OS_OBJECT` types (VM, vCPU and GIC configurations, GIC state) are
//!   pointers to opaque structs, released with `os_release`.
//! * Functions available since macOS 11.0 are linked directly. Functions
//!   introduced later are resolved at run time through `DynApi`, so binaries
//!   keep loading on older releases; hard-linked declarations of those are in
//!   the `direct` module.
//!
//! The types, constants and register encodings compile on every target. The
//! function declarations, the SIMD setter trampoline and the `os` helpers are
//! only available on `aarch64` macOS.
//!
//! Using the framework requires the calling binary to be signed with the
//! `com.apple.security.hypervisor` entitlement. Without it `hv_vm_create`
//! returns [`HV_DENIED`].

#[macro_use]
pub mod macros;

pub mod gic;
pub mod regs;
pub mod sme;
pub mod types;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod functions;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod os;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod simd;

#[doc(hidden)]
pub use macros::{ConstEntry, FnSig};

// Every module emits a hidden `__CONSTS` table; the glob re-exports below are
// ambiguous only for those, which are reached through their module path.
#[allow(ambiguous_glob_reexports)]
pub use gic::*;
#[allow(ambiguous_glob_reexports)]
pub use regs::*;
#[allow(ambiguous_glob_reexports)]
pub use sme::*;
#[allow(ambiguous_glob_reexports)]
pub use types::*;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use functions::*;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use os::*;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use simd::*;

/// The `__CONSTS` tables of every module, for the SDK check.
#[doc(hidden)]
pub const __ALL_CONSTS: &[&[ConstEntry]] = &[
    types::__CONSTS,
    regs::__CONSTS,
    gic::__CONSTS,
    sme::__CONSTS,
];
