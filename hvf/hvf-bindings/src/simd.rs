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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::tests::{TestVcpu, TestVm};
    use crate::functions::{hv_vcpu_get_reg, hv_vcpu_get_simd_fp_reg, hv_vcpu_set_reg};
    use crate::regs::{HV_REG_X0, HV_REG_X1, HV_SIMD_FP_REG_Q31};
    use crate::types::{HV_BAD_ARGUMENT, HV_SUCCESS};

    fn pattern(reg: hv_simd_fp_reg_t, salt: u8) -> hv_simd_fp_uchar16_t {
        let mut v = [0u8; 16];
        for (i, b) in v.iter_mut().enumerate() {
            *b = (reg as u8).wrapping_mul(16).wrapping_add(i as u8) ^ salt;
        }
        hv_simd_fp_uchar16_t(v)
    }

    fn get(vcpu: &TestVcpu, reg: hv_simd_fp_reg_t) -> hv_simd_fp_uchar16_t {
        let mut v = hv_simd_fp_uchar16_t::default();
        // SAFETY: the vCPU is owned by this thread and `v` is valid for
        // writes.
        let ret = unsafe { hv_vcpu_get_simd_fp_reg(vcpu.id, reg, &mut v) };
        assert_eq!(ret, HV_SUCCESS, "get Q{reg}");
        v
    }

    fn set(vcpu: &TestVcpu, reg: hv_simd_fp_reg_t, v: &hv_simd_fp_uchar16_t) -> hv_return_t {
        // SAFETY: the vCPU is owned by this thread and `v` is valid for
        // reading 16 bytes.
        unsafe { hv_vcpu_set_simd_fp_reg_by_ref(vcpu.id, reg, v) }
    }

    #[test]
    fn test_set_simd_fp_reg_by_ref() {
        let Some(vm) = TestVm::new() else { return };
        let vcpu = TestVcpu::new(&vm);

        // SAFETY: the vCPU is owned by this thread.
        unsafe {
            assert_eq!(hv_vcpu_set_reg(vcpu.id, HV_REG_X0, 0x1111), HV_SUCCESS);
            assert_eq!(hv_vcpu_set_reg(vcpu.id, HV_REG_X1, 0x2222), HV_SUCCESS);
        }

        for reg in 0..=HV_SIMD_FP_REG_Q31 {
            assert_eq!(set(&vcpu, reg, &pattern(reg, 0)), HV_SUCCESS, "set Q{reg}");
        }
        for reg in 0..=HV_SIMD_FP_REG_Q31 {
            assert_eq!(get(&vcpu, reg), pattern(reg, 0), "Q{reg}");
        }

        // Setting one register leaves the other 31 alone.
        assert_eq!(set(&vcpu, 7, &pattern(7, 0xa5)), HV_SUCCESS);
        for reg in 0..=HV_SIMD_FP_REG_Q31 {
            let salt = if reg == 7 { 0xa5 } else { 0 };
            assert_eq!(get(&vcpu, reg), pattern(reg, salt), "Q{reg}");
        }

        // The framework's error code comes back through the tail call.
        assert_eq!(
            set(&vcpu, HV_SIMD_FP_REG_Q31 + 1, &pattern(0, 0)),
            HV_BAD_ARGUMENT
        );

        // The trampoline only touches q0: the vCPU's general purpose
        // registers are unaffected by the calls above.
        let mut x = [0u64; 2];
        // SAFETY: the vCPU is owned by this thread and `x` is valid for
        // writes.
        unsafe {
            assert_eq!(hv_vcpu_get_reg(vcpu.id, HV_REG_X0, &mut x[0]), HV_SUCCESS);
            assert_eq!(hv_vcpu_get_reg(vcpu.id, HV_REG_X1, &mut x[1]), HV_SUCCESS);
        }
        assert_eq!(x, [0x1111, 0x2222]);

        drop(vcpu);
        drop(vm);
    }
}
