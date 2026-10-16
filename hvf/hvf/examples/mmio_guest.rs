// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Runs a small guest that performs MMIO accesses.
//!
//! Run with `cargo run -p hvf --example mmio_guest`; the cargo runner of this
//! repository signs the binary with the hypervisor entitlement.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() {
    use hvf_bindings::{HV_SUCCESS, hv_vm_create, hv_vm_destroy};

    // SAFETY: a null configuration requests the default VM configuration.
    let ret = unsafe { hv_vm_create(std::ptr::null_mut()) };
    println!("hv_vm_create: {ret:#x}");
    if ret == HV_SUCCESS {
        // SAFETY: the VM was created above and has no vCPUs.
        let ret = unsafe { hv_vm_destroy() };
        assert_eq!(ret, HV_SUCCESS, "hv_vm_destroy: {ret:#x}");
    } else {
        std::process::exit(1);
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("This example requires macOS on Apple silicon.");
}
