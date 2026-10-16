// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! libSystem helpers needed by the bindings: `os_release` for the
//! framework's `OS_OBJECT` types and `dlsym` for run-time symbol resolution.

use core::ffi::{c_char, c_void};

/// Pseudo-handle for `dlsym` that searches every loaded image
/// (`RTLD_DEFAULT` from `<dlfcn.h>`).
pub const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

unsafe extern "C" {
    /// Releases a reference to an `os_object` (for example an
    /// [`hv_vm_config_t`](crate::hv_vm_config_t)).
    pub fn os_release(object: *mut c_void);

    /// Returns the address of the symbol `symbol` (a NUL-terminated string),
    /// or null if it cannot be found.
    pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
