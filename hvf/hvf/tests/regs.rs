// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! General purpose, special and system register access.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use common::{GUEST_IPA, GuestRam, SPIN, boot_vcpu, expect_exception, hv_guard, hvc, mach_time};
use hvf::{CacheType, Error, ExceptionClass, FeatureReg, Hvf, Reg, SysReg, VcpuConfig, VmConfig};

#[test]
fn test_initial_state() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let vcpu = vm.create_vcpu(None).unwrap();
    // The framework leaves PSTATE and the affinity to the VMM.
    assert_eq!(vcpu.get_reg(Reg::CPSR).unwrap(), 0);
    let mpidr = vcpu.get_sys_reg(SysReg::MPIDR_EL1).unwrap();
    assert_eq!(mpidr & 0xff_00ff_ffff, 0, "MPIDR {mpidr:#x}");
    // The ID registers describe a 64-bit capable CPU.
    let pfr0 = vcpu.get_sys_reg(SysReg::ID_AA64PFR0_EL1).unwrap();
    assert_ne!(pfr0 & 0xf, 0, "ID_AA64PFR0_EL1 {pfr0:#x}");
    assert_ne!(vcpu.get_sys_reg(SysReg::MIDR_EL1).unwrap(), 0);
}

#[test]
fn test_general_purpose_registers() {
    let Some(_g) = hv_guard() else { return };
    assert_eq!(Reg::x(31), None);
    assert_eq!(Reg::x(29), Some(Reg::FP));
    assert_eq!(Reg::x(30), Some(Reg::LR));

    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let mut vcpu = vm.create_vcpu(None).unwrap();
    let value = |n: u8| 0x0123_4567_89ab_cdef_u64.rotate_left(u32::from(n) * 3) ^ u64::from(n);
    for n in 0..=30 {
        vcpu.set_reg(Reg::x(n).unwrap(), value(n)).unwrap();
    }
    for n in 0..=30 {
        assert_eq!(vcpu.get_reg(Reg::x(n).unwrap()).unwrap(), value(n), "X{n}");
    }

    vcpu.set_reg(Reg::PC, 0x4000_1234).unwrap();
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), 0x4000_1234);
    vcpu.advance_pc().unwrap();
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), 0x4000_1238);

    // EL1h, all of DAIF masked, and the NZCV flags.
    for cpsr in [0x3c5, 0x3c4, 0x6000_03c5] {
        vcpu.set_reg(Reg::CPSR, cpsr).unwrap();
        assert_eq!(vcpu.get_reg(Reg::CPSR).unwrap(), cpsr);
    }

    // Rounding mode and flush-to-zero; cumulative exception flags.
    vcpu.set_reg(Reg::FPCR, (0b11 << 22) | (1 << 24)).unwrap();
    assert_eq!(vcpu.get_reg(Reg::FPCR).unwrap(), (0b11 << 22) | (1 << 24));
    vcpu.set_reg(Reg::FPSR, 0x1f).unwrap();
    assert_eq!(vcpu.get_reg(Reg::FPSR).unwrap(), 0x1f);

    // Unknown register numbers are rejected by the framework.
    let e = vcpu.get_reg(Reg(0x1000)).unwrap_err();
    assert!(
        matches!(e, Error::Hv { call: "hv_vcpu_get_reg", err } if err.is_bad_argument()),
        "{e:?}"
    );
}

#[test]
fn test_system_registers() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let mut vcpu = vm.create_vcpu(None).unwrap();

    // Registers that hold any value written (within their defined fields).
    let table = [
        (SysReg::MPIDR_EL1, 0x0000_0000_0001_0203),
        (SysReg::VBAR_EL1, 0x4000_0800),
        (SysReg::TTBR0_EL1, 0x4001_0000),
        (SysReg::TTBR1_EL1, 0x0001_0000_4002_0000),
        (SysReg::TCR_EL1, 0x0000_0005_b519_3519),
        (SysReg::MAIR_EL1, 0x0000_0000_00ff_4400),
        (SysReg::ELR_EL1, 0x4000_1000),
        (SysReg::SPSR_EL1, 0x3c5),
        (SysReg::SP_EL0, 0x4800_0000),
        (SysReg::SP_EL1, 0x4900_0000),
        (SysReg::TPIDR_EL0, 0x1111_2222_3333_4444),
        (SysReg::TPIDRRO_EL0, 0x5555_6666_7777_8888),
        (SysReg::TPIDR_EL1, 0x9999_aaaa_bbbb_cccc),
        (SysReg::CONTEXTIDR_EL1, 0x42),
        (SysReg::FAR_EL1, 0xdead_0000),
        (SysReg::ESR_EL1, 0x5600_0000),
        (SysReg::CNTV_CVAL_EL0, 0x1234_5678_9abc),
    ];
    for (reg, value) in table {
        vcpu.set_sys_reg(reg, value).unwrap();
    }
    for (reg, value) in table {
        assert_eq!(
            vcpu.get_sys_reg(reg).unwrap(),
            value,
            "{reg:?} {:?}",
            reg.encoding()
        );
    }

    // SCTLR_EL1 has fixed bits; the M (MMU enable) bit toggles.
    let sctlr = vcpu.get_sys_reg(SysReg::SCTLR_EL1).unwrap();
    vcpu.set_sys_reg(SysReg::SCTLR_EL1, sctlr | 1).unwrap();
    assert_eq!(vcpu.get_sys_reg(SysReg::SCTLR_EL1).unwrap() & 1, 1);
    vcpu.set_sys_reg(SysReg::SCTLR_EL1, sctlr & !1).unwrap();
    assert_eq!(vcpu.get_sys_reg(SysReg::SCTLR_EL1).unwrap() & 1, 0);

    // ID registers are read-only for the guest but are reported.
    vcpu.get_sys_reg(SysReg::ID_AA64MMFR0_EL1).unwrap();
    // The encoding of a system register is its `hv_sys_reg_t` value.
    assert_eq!(
        SysReg::from_encoding(3, 0, 12, 0, 0),
        SysReg::VBAR_EL1,
        "VBAR_EL1 is S3_0_C12_C0_0"
    );
    // A register this crate does not know about is rejected by HVF.
    let e = vcpu.get_sys_reg(SysReg(0x0001)).unwrap_err();
    assert!(
        matches!(
            e,
            Error::Hv {
                call: "hv_vcpu_get_sys_reg",
                ..
            }
        ),
        "{e:?}"
    );
}

/// The guest sees the values the VMM set, and the VMM sees the values the
/// guest wrote.
#[test]
fn test_registers_seen_by_guest() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    // mrs x1, mpidr_el1; msr tpidr_el1, x2; hvc #0; b .
    let _code = GuestRam::with_code(&vm, &[0xd538_00a1, 0xd518_d082, hvc(0), SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    vcpu.set_sys_reg(SysReg::MPIDR_EL1, 0x0000_0000_8000_0102)
        .unwrap();
    vcpu.set_reg(Reg::x(2).unwrap(), 0xfeed_face_cafe_beef)
        .unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    let mpidr = vcpu.get_reg(Reg::x(1).unwrap()).unwrap();
    assert_eq!(mpidr & 0xff_00ff_ffff, 0x0102, "guest MPIDR {mpidr:#x}");
    assert_eq!(
        vcpu.get_sys_reg(SysReg::TPIDR_EL1).unwrap(),
        0xfeed_face_cafe_beef
    );
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), GUEST_IPA + 0xc);
}

#[test]
fn test_vtimer_offset_and_mask() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let mut vcpu = vm.create_vcpu(None).unwrap();
    for offset in [0, 1, 0x1234_5678, u64::MAX / 2] {
        vcpu.set_vtimer_offset(offset).unwrap();
        assert_eq!(vcpu.vtimer_offset().unwrap(), offset);
    }
    for mask in [true, false, true] {
        vcpu.set_vtimer_mask(mask).unwrap();
        assert_eq!(vcpu.vtimer_mask().unwrap(), mask);
    }
    for trap in [true, false] {
        vcpu.set_trap_debug_exceptions(trap).unwrap();
        assert_eq!(vcpu.trap_debug_exceptions().unwrap(), trap);
        vcpu.set_trap_debug_reg_accesses(trap).unwrap();
        assert_eq!(vcpu.trap_debug_reg_accesses().unwrap(), trap);
    }
}

/// The guest's virtual counter is the host's Mach absolute time minus the
/// vtimer offset.
#[test]
fn test_vtimer_offset_seen_by_guest() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    // mrs x0, cntvct_el0; hvc #0; b .
    let _code = GuestRam::with_code(&vm, &[0xd53b_e040, hvc(0), SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);

    let now = mach_time::now();
    vcpu.set_vtimer_offset(now).unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    let elapsed = mach_time::now() - now;
    let counter = vcpu.get_reg(Reg::x(0).unwrap()).unwrap();
    assert!(counter <= elapsed, "counter {counter} > elapsed {elapsed}");
}

#[test]
fn test_vcpu_config() {
    let Some(_g) = hv_guard() else { return };
    let cfg = VcpuConfig::new().unwrap();
    let pfr0 = cfg.feature_reg(FeatureReg::ID_AA64PFR0_EL1).unwrap();
    assert_ne!(pfr0, 0);
    // EL0 and EL1 are implemented (AArch64 at least).
    assert_ne!(pfr0 & 0xf, 0);
    assert_ne!((pfr0 >> 4) & 0xf, 0);
    assert_ne!(cfg.feature_reg(FeatureReg::CTR_EL0).unwrap(), 0);
    for cache in [CacheType::Data, CacheType::Instruction] {
        let ccsidr = cfg.ccsidr_el1_values(cache).unwrap();
        assert_ne!(ccsidr[0], 0, "{cache:?} L1");
    }

    // A vCPU created with the configuration reports the same ID register.
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let vcpu = vm.create_vcpu(Some(&cfg)).unwrap();
    assert_eq!(vcpu.get_sys_reg(SysReg::ID_AA64PFR0_EL1).unwrap(), pfr0);
}
