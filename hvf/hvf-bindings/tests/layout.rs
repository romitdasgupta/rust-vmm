// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Size, alignment and field offset checks of the `#[repr(C)]` bindings,
//! plus consistency checks of the constant tables.
//!
//! These run on every target. `sdk_check.rs` compares the same properties with
//! the C headers on macOS.

use std::collections::HashSet;
use std::mem::{align_of, offset_of, size_of};

use hvf_bindings::*;

#[test]
fn test_exit_layout() {
    assert_eq!(size_of::<hv_vcpu_exit_exception_t>(), 24);
    assert_eq!(align_of::<hv_vcpu_exit_exception_t>(), 8);
    assert_eq!(offset_of!(hv_vcpu_exit_exception_t, syndrome), 0);
    assert_eq!(offset_of!(hv_vcpu_exit_exception_t, virtual_address), 8);
    assert_eq!(offset_of!(hv_vcpu_exit_exception_t, physical_address), 16);

    assert_eq!(size_of::<hv_vcpu_exit_t>(), 32);
    assert_eq!(align_of::<hv_vcpu_exit_t>(), 8);
    assert_eq!(offset_of!(hv_vcpu_exit_t, reason), 0);
    assert_eq!(offset_of!(hv_vcpu_exit_t, exception), 8);
}

#[test]
fn test_vector_layout() {
    assert_eq!(size_of::<hv_simd_fp_uchar16_t>(), 16);
    assert_eq!(align_of::<hv_simd_fp_uchar16_t>(), 16);
    assert_eq!(size_of::<hv_sme_zt0_uchar64_t>(), 64);
    assert_eq!(align_of::<hv_sme_zt0_uchar64_t>(), 16);
}

#[test]
fn test_sme_state_layout() {
    assert_eq!(size_of::<hv_vcpu_sme_state_t>(), 2);
    assert_eq!(align_of::<hv_vcpu_sme_state_t>(), 1);
    assert_eq!(
        offset_of!(hv_vcpu_sme_state_t, streaming_sve_mode_enabled),
        0
    );
    assert_eq!(offset_of!(hv_vcpu_sme_state_t, za_storage_enabled), 1);
}

#[test]
fn test_opaque_objects() {
    // Opaque structs are zero sized and only used behind thin pointers.
    assert_eq!(size_of::<hv_vm_config_s>(), 0);
    assert_eq!(size_of::<hv_vcpu_config_s>(), 0);
    assert_eq!(size_of::<hv_gic_config_s>(), 0);
    assert_eq!(size_of::<hv_gic_state_s>(), 0);
    assert_eq!(size_of::<hv_vm_config_t>(), size_of::<usize>());
    assert_eq!(size_of::<hv_vcpu_config_t>(), size_of::<usize>());
    assert_eq!(size_of::<hv_gic_config_t>(), size_of::<usize>());
    assert_eq!(size_of::<hv_gic_state_t>(), size_of::<usize>());
}

#[test]
fn test_scalar_types() {
    macro_rules! check {
        ($($t:ty => $size:literal, $signed:literal;)*) => {
            $(
                assert_eq!(size_of::<$t>(), $size, stringify!($t));
                assert_eq!(<$t>::MIN != 0, $signed, stringify!($t));
            )*
        };
    }
    check! {
        hv_return_t => 4, true;
        hv_ipa_t => 8, false;
        hv_vcpu_t => 8, false;
        hv_exception_syndrome_t => 8, false;
        hv_exception_address_t => 8, false;
        hv_memory_flags_t => 8, false;
        hv_allocate_flags_t => 8, false;
        hv_exit_reason_t => 4, false;
        hv_interrupt_type_t => 4, false;
        hv_ipa_granule_t => 4, false;
        hv_cache_type_t => 4, false;
        hv_feature_reg_t => 4, false;
        hv_tlbi_op_t => 2, false;
        hv_reg_t => 4, false;
        hv_simd_fp_reg_t => 4, false;
        hv_sys_reg_t => 2, false;
        hv_sme_z_reg_t => 4, false;
        hv_sme_p_reg_t => 4, false;
        hv_gic_intid_t => 2, false;
        hv_gic_distributor_reg_t => 2, false;
        hv_gic_redistributor_reg_t => 4, false;
        hv_gic_icc_reg_t => 2, false;
        hv_gic_ich_reg_t => 2, false;
        hv_gic_icv_reg_t => 2, false;
        hv_gic_msi_reg_t => 2, false;
    }
}

fn all_consts() -> impl Iterator<Item = &'static ConstEntry> {
    __ALL_CONSTS.iter().flat_map(|t| t.iter())
}

#[test]
fn test_const_tables() {
    let mut names = HashSet::new();
    let mut count = 0;
    for &(name, ty, _, since) in all_consts() {
        assert!(name.starts_with("HV_"), "{name}");
        assert!(ty.starts_with("hv_") && ty.ends_with("_t"), "{name}: {ty}");
        assert!(since >= (11, 0), "{name}: {since:?}");
        assert!(names.insert(name), "{name} declared twice");
        count += 1;
    }
    // Everything in the arm64 SDK 27.0 headers.
    assert_eq!(count, 1926);
}

#[test]
fn test_const_values() {
    let find = |name: &str| {
        all_consts()
            .find(|e| e.0 == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    };
    // A few spot checks of the table against the constants themselves.
    assert_eq!(find("HV_BUSY").2, HV_BUSY as i128);
    assert_eq!(HV_BUSY as u32, 0xfae9_4002);
    assert_eq!(find("HV_SYS_REG_MPIDR_EL1").2, 0xc005);
    assert_eq!(find("HV_SYS_REG_ACTLR_EL1").3, (15, 0));
    assert_eq!(find("HV_FEATURE_REG_ID_AA64MMFR4_EL1").3, (27, 0));
    assert_eq!(find("HV_GIC_DISTRIBUTOR_REG_GICD_IROUTER1019").2, 0x7fd8);
    assert_eq!(
        find("HV_GIC_REDISTRIBUTOR_REG_GICR_ICFGR1").1,
        "hv_gic_redistributor_reg_t"
    );
}

#[test]
fn test_sys_reg_encoding() {
    // Every system register value is its MRS/MSR encoding, so op0 is 2 (debug)
    // or 3, and CRn is at most 15 by construction.
    for &(name, ty, value, _) in all_consts() {
        if ty == "hv_sys_reg_t" {
            let op0 = value >> 14;
            assert!(op0 == 2 || op0 == 3, "{name}: {value:#x}");
        }
    }
    let enc = |op0: u16, op1: u16, crn: u16, crm: u16, op2: u16| {
        (op0 << 14) | (op1 << 11) | (crn << 7) | (crm << 3) | op2
    };
    assert_eq!(HV_SYS_REG_MIDR_EL1, enc(3, 0, 0, 0, 0));
    assert_eq!(HV_SYS_REG_MPIDR_EL1, enc(3, 0, 0, 0, 5));
    assert_eq!(HV_SYS_REG_SCTLR_EL1, enc(3, 0, 1, 0, 0));
    assert_eq!(HV_SYS_REG_TTBR1_EL1, enc(3, 0, 2, 0, 1));
    assert_eq!(HV_SYS_REG_CNTV_CTL_EL0, enc(3, 3, 14, 3, 1));
    assert_eq!(HV_SYS_REG_SP_EL1, enc(3, 4, 4, 1, 0));
    assert_eq!(HV_SYS_REG_DBGWCR15_EL1, enc(2, 0, 0, 15, 7));
    assert_eq!(HV_GIC_ICC_REG_PMR_EL1, enc(3, 0, 4, 6, 0));
    assert_eq!(HV_GIC_ICH_REG_LR15_EL2, enc(3, 4, 12, 13, 7));
}

#[test]
fn test_gic_register_arrays() {
    for n in 0..32u16 {
        let name = format!("HV_GIC_DISTRIBUTOR_REG_GICD_ISENABLER{n}");
        let value = all_consts().find(|e| e.0 == name).unwrap().2;
        assert_eq!(value, 0x100 + 4 * n as i128, "{name}");
    }
    for n in 32..1020u16 {
        let name = format!("HV_GIC_DISTRIBUTOR_REG_GICD_IROUTER{n}");
        let value = all_consts().find(|e| e.0 == name).unwrap().2;
        assert_eq!(value, 0x6000 + 8 * n as i128, "{name}");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn test_fn_table() {
    let mut names = HashSet::new();
    for f in __FNS {
        assert!(f.name.starts_with("hv_"), "{}", f.name);
        assert!(f.since >= (11, 0), "{}", f.name);
        assert!(names.insert(f.name), "{} declared twice", f.name);
        assert!(!f.ret.is_empty(), "{}", f.name);
        // The availability table agrees with the function table.
        assert!(
            f.since == (11, 0) || format!("{:?}", DynApi::default()).contains(f.name),
            "{} missing from DynApi",
            f.name
        );
    }
    assert_eq!(names.len(), 92);
    assert_eq!(since::hv_vm_create, (11, 0));
    assert_eq!(since::hv_vcpu_invalidate_tlb, (27, 0));
}
