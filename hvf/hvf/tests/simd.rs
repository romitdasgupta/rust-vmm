// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! SIMD/FP register access.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.
//!
//! The setter goes through a hand-written trampoline that passes the value in
//! `q0` (AAPCS64), so these tests also run in release builds:
//! `cargo test -p hvf --test simd --release`.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use common::{GUEST_IPA, GuestRam, SPIN, boot_vcpu, expect_exception, hv_guard, hvc};
use hvf::{Error, ExceptionClass, Hvf, SimdReg, VmConfig};

/// A distinct, asymmetric 128-bit pattern for register `n`.
fn pattern(n: u8, seed: u8) -> u128 {
    let bytes: [u8; 16] = std::array::from_fn(|i| (i as u8).wrapping_mul(17) ^ n ^ seed);
    u128::from_le_bytes(bytes) ^ (u128::from(n) << 120)
}

fn q(n: u8) -> SimdReg {
    SimdReg::q(n).unwrap()
}

#[test]
fn test_all_q_registers() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let mut vcpu = vm.create_vcpu(None).unwrap();

    for n in 0..32 {
        vcpu.set_simd_fp_reg(q(n), pattern(n, 0)).unwrap();
    }
    for n in 0..32 {
        assert_eq!(
            vcpu.get_simd_fp_reg(q(n)).unwrap(),
            pattern(n, 0),
            "Q{n} after setting all registers"
        );
    }

    // Setting one register leaves the other 31 alone.
    for target in [0, 1, 7, 15, 16, 31] {
        vcpu.set_simd_fp_reg(q(target), pattern(target, 0xa5))
            .unwrap();
        for n in 0..32 {
            let expected = if n == target {
                pattern(n, 0xa5)
            } else {
                pattern(n, 0)
            };
            assert_eq!(
                vcpu.get_simd_fp_reg(q(n)).unwrap(),
                expected,
                "Q{n} after setting Q{target}"
            );
        }
        vcpu.set_simd_fp_reg(q(target), pattern(target, 0)).unwrap();
    }

    // Extreme values survive the trip through the vector register.
    for value in [0, u128::MAX, 1, 1 << 127, 1 << 64, u128::from(u64::MAX)] {
        vcpu.set_simd_fp_reg(q(5), value).unwrap();
        assert_eq!(vcpu.get_simd_fp_reg(q(5)).unwrap(), value);
    }
}

#[test]
fn test_invalid_q_register() {
    let Some(_g) = hv_guard() else { return };
    assert_eq!(SimdReg::q(32), None);
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let mut vcpu = vm.create_vcpu(None).unwrap();
    // Raw out-of-range numbers are rejected by the framework, through the
    // trampoline as well.
    let e = vcpu.set_simd_fp_reg(SimdReg(32), 1).unwrap_err();
    assert!(
        matches!(e, Error::Hv { err, .. } if err.is_bad_argument()),
        "{e:?}"
    );
    let e = vcpu.get_simd_fp_reg(SimdReg(32)).unwrap_err();
    assert!(
        matches!(e, Error::Hv { err, .. } if err.is_bad_argument()),
        "{e:?}"
    );
}

/// Values set by the VMM are what the guest sees, and survive guest
/// execution that does not touch them.
#[test]
fn test_q_registers_across_run() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    // CPACR_EL1.FPEN must allow FP/SIMD at EL1 for the guest instructions.
    // mov v0.16b, v1.16b (orr v0.16b, v1.16b, v1.16b); hvc #0; b .
    let _code = GuestRam::with_code(&vm, &[0x4ea1_1c20, hvc(0), SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    vcpu.set_sys_reg(hvf::SysReg::CPACR_EL1, 0b11 << 20)
        .unwrap();
    for n in 0..32 {
        vcpu.set_simd_fp_reg(q(n), pattern(n, 0x3c)).unwrap();
    }
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    // The guest copied Q1 into Q0; every other register is unchanged.
    assert_eq!(vcpu.get_simd_fp_reg(q(0)).unwrap(), pattern(1, 0x3c));
    for n in 1..32 {
        assert_eq!(vcpu.get_simd_fp_reg(q(n)).unwrap(), pattern(n, 0x3c));
    }
}
