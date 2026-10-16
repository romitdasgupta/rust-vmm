// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Hypervisor.framework functions (arm64).
//!
//! Functions available since macOS 11.0 are declared here and linked
//! directly. Newer functions are resolved at run time through [`DynApi`]; their
//! hard-linked declarations are in [`direct`].

use crate::types::*;

hv_fns! {
    /// Returns the maximum number of vCPUs the hypervisor supports.
    (11, 0) fn hv_vm_get_max_vcpu_count(max_vcpu_count: *mut u32) -> hv_return_t;
    /// Creates a VM instance for the current process. `config` may be null
    /// for the default configuration.
    (11, 0) fn hv_vm_create(config: hv_vm_config_t) -> hv_return_t;
    /// Destroys the VM instance of the current process.
    (11, 0) fn hv_vm_destroy() -> hv_return_t;
    /// Creates a VM configuration object, to be released with `os_release`.
    (13, 0) fn hv_vm_config_create() -> hv_vm_config_t;
}
