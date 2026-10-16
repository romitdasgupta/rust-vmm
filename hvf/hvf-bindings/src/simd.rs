// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Setter for SIMD/FP registers.
//!
//! `hv_vcpu_set_simd_fp_reg` takes its 128-bit value by value, which AAPCS64
//! passes in `q0`. Stable Rust cannot pass a vector type through an
//! `extern "C"` declaration, so the real symbol is only declared privately
//! and [`hv_vcpu_set_simd_fp_reg_by_ref`] reaches it through a naked tail-call
//! trampoline.

use crate::regs::hv_simd_fp_reg_t;
use crate::types::{hv_return_t, hv_simd_fp_uchar16_t, hv_vcpu_t};

#[cfg_attr(target_vendor = "apple", link(name = "Hypervisor", kind = "framework"))]
unsafe extern "C" {
    // Declared without arguments on purpose: it must only be reached through
    // the trampoline below, which sets up the vector argument register.
    #[link_name = "hv_vcpu_set_simd_fp_reg"]
    fn __hv_vcpu_set_simd_fp_reg();
}

/// Calls `hv_vcpu_set_simd_fp_reg(vcpu, reg, *value)`.
///
/// The trampoline loads `*value` into `q0` and tail-calls the framework
/// function, leaving `x0` (`vcpu`) and `w1` (`reg`) untouched, which is the
/// AAPCS64 calling sequence of the C prototype. The framework function
/// returns straight to the caller. Available since macOS 11.0.
///
/// # Safety
///
/// Same as `hv_vcpu_set_simd_fp_reg`: it must be called on the thread that
/// owns `vcpu`. `value` must be valid for reading 16 bytes.
#[unsafe(naked)]
pub unsafe extern "C" fn hv_vcpu_set_simd_fp_reg_by_ref(
    vcpu: hv_vcpu_t,
    reg: hv_simd_fp_reg_t,
    value: *const hv_simd_fp_uchar16_t,
) -> hv_return_t {
    core::arch::naked_asm!(
        "ldr q0, [x2]",
        "b {f}",
        f = sym __hv_vcpu_set_simd_fp_reg,
    )
}
