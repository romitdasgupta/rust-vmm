// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Register identifiers.

use hvf_bindings as hv;

/// A general purpose or special register of a vCPU (`hv_reg_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Reg(pub hv::hv_reg_t);

impl Reg {
    /// Frame pointer (`X29`).
    pub const FP: Reg = Reg(hv::HV_REG_FP);
    /// Link register (`X30`).
    pub const LR: Reg = Reg(hv::HV_REG_LR);
    /// Program counter.
    pub const PC: Reg = Reg(hv::HV_REG_PC);
    /// Floating-point control register.
    pub const FPCR: Reg = Reg(hv::HV_REG_FPCR);
    /// Floating-point status register.
    pub const FPSR: Reg = Reg(hv::HV_REG_FPSR);
    /// Current program status register.
    pub const CPSR: Reg = Reg(hv::HV_REG_CPSR);

    /// General purpose register `Xn`, or `None` if `n > 30`.
    ///
    /// Register number 31 is the zero register or the stack pointer depending
    /// on the instruction, so it has no `Reg`.
    pub fn x(n: u8) -> Option<Reg> {
        (n <= 30).then(|| Reg(hv::HV_REG_X0 + hv::hv_reg_t::from(n)))
    }
}

/// Declares the `SysReg` constants and the table describing them.
///
/// Each entry is `(major, minor) NAME = (op0, op1, CRn, CRm, op2) => raw,`
/// where `(major, minor)` is the first macOS release accepting the register
/// and `raw` is the `hv_sys_reg_t` value from the SDK header. The constant is
/// defined through `SysReg::from_encoding`, and the unit tests check it
/// against `raw`.
macro_rules! sys_regs {
    ($(
        $(#[$m:meta])*
        ($maj:literal, $min:literal) $name:ident =
            ($op0:literal, $op1:literal, $crn:literal, $crm:literal, $op2:literal) => $raw:literal,
    )*) => {
        impl SysReg {
            $(
                #[doc = concat!(
                    "`", stringify!($name), "` (`S", stringify!($op0), "_", stringify!($op1),
                    "_C", stringify!($crn), "_C", stringify!($crm), "_", stringify!($op2),
                    "`), available since macOS ", stringify!($maj), ".", stringify!($min), "."
                )]
                $(#[$m])*
                pub const $name: SysReg = SysReg::from_encoding($op0, $op1, $crn, $crm, $op2);
            )*
        }

        /// Every `HV_SYS_REG_*` register of the SDK.
        const SYS_REGS: &[SysRegInfo] = &[
            $(SysRegInfo {
                name: stringify!($name),
                reg: SysReg::$name,
                since: ($maj, $min),
                raw: $raw,
            },)*
        ];
    };
}

/// Description of one `HV_SYS_REG_*` value.
#[derive(Debug, Clone, Copy)]
struct SysRegInfo {
    /// Register name without the `HV_SYS_REG_` prefix.
    #[cfg_attr(not(test), allow(dead_code))]
    name: &'static str,
    /// The register.
    reg: SysReg,
    /// First macOS release accepting the register.
    since: (u16, u16),
    /// The value of the SDK constant.
    #[cfg_attr(not(test), allow(dead_code))]
    raw: hv::hv_sys_reg_t,
}

/// A system register of a vCPU (`hv_sys_reg_t`), encoded as
/// `(op0 << 14) | (op1 << 11) | (CRn << 7) | (CRm << 3) | op2`.
///
/// The associated constants cover every register the framework gives access
/// to. Registers marked as newer than macOS 11.0 make the register accessors
/// fail on older systems; [`SysReg::since`] reports the release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SysReg(pub hv::hv_sys_reg_t);

// `KVM_REG_ARM64 | KVM_REG_SIZE_U64 | KVM_REG_ARM64_SYSREG`, the fixed part of
// the KVM one-reg id of a 64-bit arm64 system register. The low 16 bits use the
// same `op0:op1:CRn:CRm:op2` layout as `hv_sys_reg_t`.
const KVM_ARM64_SYSREG_ID_BASE: u64 = 0x6000_0000_0000_0000 | 0x0030_0000_0000_0000 | 0x0013 << 16;

impl SysReg {
    /// Builds the identifier from the `MRS`/`MSR` operand fields.
    ///
    /// Each field is truncated to its architectural width: 2 bits for `op0`,
    /// 3 bits for `op1` and `op2`, 4 bits for `CRn` and `CRm`.
    pub const fn from_encoding(op0: u8, op1: u8, crn: u8, crm: u8, op2: u8) -> SysReg {
        SysReg(
            ((op0 as u16 & 0x3) << 14)
                | ((op1 as u16 & 0x7) << 11)
                | ((crn as u16 & 0xf) << 7)
                | ((crm as u16 & 0xf) << 3)
                | (op2 as u16 & 0x7),
        )
    }

    /// The `(op0, op1, CRn, CRm, op2)` operand fields.
    pub fn encoding(self) -> (u8, u8, u8, u8, u8) {
        let v = self.0;
        (
            ((v >> 14) & 0x3) as u8,
            ((v >> 11) & 0x7) as u8,
            ((v >> 7) & 0xf) as u8,
            ((v >> 3) & 0xf) as u8,
            (v & 0x7) as u8,
        )
    }

    /// The first macOS release `(major, minor)` that accepts this register.
    ///
    /// Registers that are not one of the associated constants report
    /// `(11, 0)`; the framework rejects them with `HV_BAD_ARGUMENT`.
    pub fn since(self) -> (u16, u16) {
        SYS_REGS
            .iter()
            .find(|info| info.reg == self)
            .map_or((11, 0), |info| info.since)
    }

    /// Converts a KVM `KVM_REG_ARM64_SYSREG` one-reg id, or returns `None` if
    /// `id` is not a 64-bit arm64 system register id.
    pub fn from_kvm_one_reg_id(id: u64) -> Option<SysReg> {
        (id & !0xffff == KVM_ARM64_SYSREG_ID_BASE).then_some(SysReg(id as u16))
    }

    /// The equivalent KVM `KVM_REG_ARM64_SYSREG` one-reg id.
    pub fn to_kvm_one_reg_id(self) -> u64 {
        KVM_ARM64_SYSREG_ID_BASE | u64::from(self.0)
    }
}

sys_regs! {
    (11, 0) DBGBVR0_EL1 = (2, 0, 0, 0, 4) => 0x8004,
    (11, 0) DBGBCR0_EL1 = (2, 0, 0, 0, 5) => 0x8005,
    (11, 0) DBGWVR0_EL1 = (2, 0, 0, 0, 6) => 0x8006,
    (11, 0) DBGWCR0_EL1 = (2, 0, 0, 0, 7) => 0x8007,
    (11, 0) DBGBVR1_EL1 = (2, 0, 0, 1, 4) => 0x800c,
    (11, 0) DBGBCR1_EL1 = (2, 0, 0, 1, 5) => 0x800d,
    (11, 0) DBGWVR1_EL1 = (2, 0, 0, 1, 6) => 0x800e,
    (11, 0) DBGWCR1_EL1 = (2, 0, 0, 1, 7) => 0x800f,
    (11, 0) MDCCINT_EL1 = (2, 0, 0, 2, 0) => 0x8010,
    (11, 0) MDSCR_EL1 = (2, 0, 0, 2, 2) => 0x8012,
    (11, 0) DBGBVR2_EL1 = (2, 0, 0, 2, 4) => 0x8014,
    (11, 0) DBGBCR2_EL1 = (2, 0, 0, 2, 5) => 0x8015,
    (11, 0) DBGWVR2_EL1 = (2, 0, 0, 2, 6) => 0x8016,
    (11, 0) DBGWCR2_EL1 = (2, 0, 0, 2, 7) => 0x8017,
    (11, 0) DBGBVR3_EL1 = (2, 0, 0, 3, 4) => 0x801c,
    (11, 0) DBGBCR3_EL1 = (2, 0, 0, 3, 5) => 0x801d,
    (11, 0) DBGWVR3_EL1 = (2, 0, 0, 3, 6) => 0x801e,
    (11, 0) DBGWCR3_EL1 = (2, 0, 0, 3, 7) => 0x801f,
    (11, 0) DBGBVR4_EL1 = (2, 0, 0, 4, 4) => 0x8024,
    (11, 0) DBGBCR4_EL1 = (2, 0, 0, 4, 5) => 0x8025,
    (11, 0) DBGWVR4_EL1 = (2, 0, 0, 4, 6) => 0x8026,
    (11, 0) DBGWCR4_EL1 = (2, 0, 0, 4, 7) => 0x8027,
    (11, 0) DBGBVR5_EL1 = (2, 0, 0, 5, 4) => 0x802c,
    (11, 0) DBGBCR5_EL1 = (2, 0, 0, 5, 5) => 0x802d,
    (11, 0) DBGWVR5_EL1 = (2, 0, 0, 5, 6) => 0x802e,
    (11, 0) DBGWCR5_EL1 = (2, 0, 0, 5, 7) => 0x802f,
    (11, 0) DBGBVR6_EL1 = (2, 0, 0, 6, 4) => 0x8034,
    (11, 0) DBGBCR6_EL1 = (2, 0, 0, 6, 5) => 0x8035,
    (11, 0) DBGWVR6_EL1 = (2, 0, 0, 6, 6) => 0x8036,
    (11, 0) DBGWCR6_EL1 = (2, 0, 0, 6, 7) => 0x8037,
    (11, 0) DBGBVR7_EL1 = (2, 0, 0, 7, 4) => 0x803c,
    (11, 0) DBGBCR7_EL1 = (2, 0, 0, 7, 5) => 0x803d,
    (11, 0) DBGWVR7_EL1 = (2, 0, 0, 7, 6) => 0x803e,
    (11, 0) DBGWCR7_EL1 = (2, 0, 0, 7, 7) => 0x803f,
    (11, 0) DBGBVR8_EL1 = (2, 0, 0, 8, 4) => 0x8044,
    (11, 0) DBGBCR8_EL1 = (2, 0, 0, 8, 5) => 0x8045,
    (11, 0) DBGWVR8_EL1 = (2, 0, 0, 8, 6) => 0x8046,
    (11, 0) DBGWCR8_EL1 = (2, 0, 0, 8, 7) => 0x8047,
    (11, 0) DBGBVR9_EL1 = (2, 0, 0, 9, 4) => 0x804c,
    (11, 0) DBGBCR9_EL1 = (2, 0, 0, 9, 5) => 0x804d,
    (11, 0) DBGWVR9_EL1 = (2, 0, 0, 9, 6) => 0x804e,
    (11, 0) DBGWCR9_EL1 = (2, 0, 0, 9, 7) => 0x804f,
    (11, 0) DBGBVR10_EL1 = (2, 0, 0, 10, 4) => 0x8054,
    (11, 0) DBGBCR10_EL1 = (2, 0, 0, 10, 5) => 0x8055,
    (11, 0) DBGWVR10_EL1 = (2, 0, 0, 10, 6) => 0x8056,
    (11, 0) DBGWCR10_EL1 = (2, 0, 0, 10, 7) => 0x8057,
    (11, 0) DBGBVR11_EL1 = (2, 0, 0, 11, 4) => 0x805c,
    (11, 0) DBGBCR11_EL1 = (2, 0, 0, 11, 5) => 0x805d,
    (11, 0) DBGWVR11_EL1 = (2, 0, 0, 11, 6) => 0x805e,
    (11, 0) DBGWCR11_EL1 = (2, 0, 0, 11, 7) => 0x805f,
    (11, 0) DBGBVR12_EL1 = (2, 0, 0, 12, 4) => 0x8064,
    (11, 0) DBGBCR12_EL1 = (2, 0, 0, 12, 5) => 0x8065,
    (11, 0) DBGWVR12_EL1 = (2, 0, 0, 12, 6) => 0x8066,
    (11, 0) DBGWCR12_EL1 = (2, 0, 0, 12, 7) => 0x8067,
    (11, 0) DBGBVR13_EL1 = (2, 0, 0, 13, 4) => 0x806c,
    (11, 0) DBGBCR13_EL1 = (2, 0, 0, 13, 5) => 0x806d,
    (11, 0) DBGWVR13_EL1 = (2, 0, 0, 13, 6) => 0x806e,
    (11, 0) DBGWCR13_EL1 = (2, 0, 0, 13, 7) => 0x806f,
    (11, 0) DBGBVR14_EL1 = (2, 0, 0, 14, 4) => 0x8074,
    (11, 0) DBGBCR14_EL1 = (2, 0, 0, 14, 5) => 0x8075,
    (11, 0) DBGWVR14_EL1 = (2, 0, 0, 14, 6) => 0x8076,
    (11, 0) DBGWCR14_EL1 = (2, 0, 0, 14, 7) => 0x8077,
    (11, 0) DBGBVR15_EL1 = (2, 0, 0, 15, 4) => 0x807c,
    (11, 0) DBGBCR15_EL1 = (2, 0, 0, 15, 5) => 0x807d,
    (11, 0) DBGWVR15_EL1 = (2, 0, 0, 15, 6) => 0x807e,
    (11, 0) DBGWCR15_EL1 = (2, 0, 0, 15, 7) => 0x807f,
    (11, 0) MIDR_EL1 = (3, 0, 0, 0, 0) => 0xc000,
    (11, 0) MPIDR_EL1 = (3, 0, 0, 0, 5) => 0xc005,
    (11, 0) ID_AA64PFR0_EL1 = (3, 0, 0, 4, 0) => 0xc020,
    (11, 0) ID_AA64PFR1_EL1 = (3, 0, 0, 4, 1) => 0xc021,
    (27, 0) ID_AA64PFR2_EL1 = (3, 0, 0, 4, 2) => 0xc022,
    (15, 2) ID_AA64ZFR0_EL1 = (3, 0, 0, 4, 4) => 0xc024,
    (15, 2) ID_AA64SMFR0_EL1 = (3, 0, 0, 4, 5) => 0xc025,
    (11, 0) ID_AA64DFR0_EL1 = (3, 0, 0, 5, 0) => 0xc028,
    (11, 0) ID_AA64DFR1_EL1 = (3, 0, 0, 5, 1) => 0xc029,
    (11, 0) ID_AA64ISAR0_EL1 = (3, 0, 0, 6, 0) => 0xc030,
    (11, 0) ID_AA64ISAR1_EL1 = (3, 0, 0, 6, 1) => 0xc031,
    (27, 0) ID_AA64ISAR2_EL1 = (3, 0, 0, 6, 2) => 0xc032,
    (11, 0) ID_AA64MMFR0_EL1 = (3, 0, 0, 7, 0) => 0xc038,
    (11, 0) ID_AA64MMFR1_EL1 = (3, 0, 0, 7, 1) => 0xc039,
    (11, 0) ID_AA64MMFR2_EL1 = (3, 0, 0, 7, 2) => 0xc03a,
    (27, 0) ID_AA64MMFR3_EL1 = (3, 0, 0, 7, 3) => 0xc03b,
    (27, 0) ID_AA64MMFR4_EL1 = (3, 0, 0, 7, 4) => 0xc03c,
    (11, 0) SCTLR_EL1 = (3, 0, 1, 0, 0) => 0xc080,
    /// Only bit 1 (`EnTSO`, total store ordering) is accessible.
    (15, 0) ACTLR_EL1 = (3, 0, 1, 0, 1) => 0xc081,
    (11, 0) CPACR_EL1 = (3, 0, 1, 0, 2) => 0xc082,
    (15, 2) SMPRI_EL1 = (3, 0, 1, 2, 4) => 0xc094,
    (15, 2) SMCR_EL1 = (3, 0, 1, 2, 6) => 0xc096,
    (11, 0) TTBR0_EL1 = (3, 0, 2, 0, 0) => 0xc100,
    (11, 0) TTBR1_EL1 = (3, 0, 2, 0, 1) => 0xc101,
    (11, 0) TCR_EL1 = (3, 0, 2, 0, 2) => 0xc102,
    (11, 0) APIAKEYLO_EL1 = (3, 0, 2, 1, 0) => 0xc108,
    (11, 0) APIAKEYHI_EL1 = (3, 0, 2, 1, 1) => 0xc109,
    (11, 0) APIBKEYLO_EL1 = (3, 0, 2, 1, 2) => 0xc10a,
    (11, 0) APIBKEYHI_EL1 = (3, 0, 2, 1, 3) => 0xc10b,
    (11, 0) APDAKEYLO_EL1 = (3, 0, 2, 2, 0) => 0xc110,
    (11, 0) APDAKEYHI_EL1 = (3, 0, 2, 2, 1) => 0xc111,
    (11, 0) APDBKEYLO_EL1 = (3, 0, 2, 2, 2) => 0xc112,
    (11, 0) APDBKEYHI_EL1 = (3, 0, 2, 2, 3) => 0xc113,
    (11, 0) APGAKEYLO_EL1 = (3, 0, 2, 3, 0) => 0xc118,
    (11, 0) APGAKEYHI_EL1 = (3, 0, 2, 3, 1) => 0xc119,
    (11, 0) SPSR_EL1 = (3, 0, 4, 0, 0) => 0xc200,
    (11, 0) ELR_EL1 = (3, 0, 4, 0, 1) => 0xc201,
    (11, 0) SP_EL0 = (3, 0, 4, 1, 0) => 0xc208,
    (11, 0) AFSR0_EL1 = (3, 0, 5, 1, 0) => 0xc288,
    (11, 0) AFSR1_EL1 = (3, 0, 5, 1, 1) => 0xc289,
    (11, 0) ESR_EL1 = (3, 0, 5, 2, 0) => 0xc290,
    (11, 0) FAR_EL1 = (3, 0, 6, 0, 0) => 0xc300,
    (11, 0) PAR_EL1 = (3, 0, 7, 4, 0) => 0xc3a0,
    (11, 0) MAIR_EL1 = (3, 0, 10, 2, 0) => 0xc510,
    (11, 0) AMAIR_EL1 = (3, 0, 10, 3, 0) => 0xc518,
    (11, 0) VBAR_EL1 = (3, 0, 12, 0, 0) => 0xc600,
    (11, 0) CONTEXTIDR_EL1 = (3, 0, 13, 0, 1) => 0xc681,
    (11, 0) TPIDR_EL1 = (3, 0, 13, 0, 4) => 0xc684,
    (15, 2) SCXTNUM_EL1 = (3, 0, 13, 0, 7) => 0xc687,
    (11, 0) CNTKCTL_EL1 = (3, 0, 14, 1, 0) => 0xc708,
    (11, 0) CSSELR_EL1 = (3, 2, 0, 0, 0) => 0xd000,
    (11, 0) TPIDR_EL0 = (3, 3, 13, 0, 2) => 0xde82,
    (11, 0) TPIDRRO_EL0 = (3, 3, 13, 0, 3) => 0xde83,
    (15, 2) TPIDR2_EL0 = (3, 3, 13, 0, 5) => 0xde85,
    (15, 2) SCXTNUM_EL0 = (3, 3, 13, 0, 7) => 0xde87,
    (11, 0) CNTV_CTL_EL0 = (3, 3, 14, 3, 1) => 0xdf19,
    (11, 0) CNTV_CVAL_EL0 = (3, 3, 14, 3, 2) => 0xdf1a,
    (11, 0) SP_EL1 = (3, 4, 4, 1, 0) => 0xe208,

    // Physical timer registers: only accessible when the VM has a GIC.
    /// Only accessible when the VM has a GIC.
    (15, 0) CNTP_CTL_EL0 = (3, 3, 14, 2, 1) => 0xdf11,
    /// Only accessible when the VM has a GIC.
    (15, 0) CNTP_CVAL_EL0 = (3, 3, 14, 2, 2) => 0xdf12,
    /// Only accessible when the VM has a GIC.
    (15, 0) CNTP_TVAL_EL0 = (3, 3, 14, 2, 0) => 0xdf10,

    // EL2 registers: only accessible when EL2 is enabled in the VM configuration.
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTHCTL_EL2 = (3, 4, 14, 1, 0) => 0xe708,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTHP_CTL_EL2 = (3, 4, 14, 2, 1) => 0xe711,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTHP_CVAL_EL2 = (3, 4, 14, 2, 2) => 0xe712,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTHP_TVAL_EL2 = (3, 4, 14, 2, 0) => 0xe710,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTVOFF_EL2 = (3, 4, 14, 0, 3) => 0xe703,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CPTR_EL2 = (3, 4, 1, 1, 2) => 0xe08a,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) ELR_EL2 = (3, 4, 4, 0, 1) => 0xe201,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) ESR_EL2 = (3, 4, 5, 2, 0) => 0xe290,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) FAR_EL2 = (3, 4, 6, 0, 0) => 0xe300,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) HCR_EL2 = (3, 4, 1, 1, 0) => 0xe088,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) HPFAR_EL2 = (3, 4, 6, 0, 4) => 0xe304,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) MAIR_EL2 = (3, 4, 10, 2, 0) => 0xe510,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) MDCR_EL2 = (3, 4, 1, 1, 1) => 0xe089,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) SCTLR_EL2 = (3, 4, 1, 0, 0) => 0xe080,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) SPSR_EL2 = (3, 4, 4, 0, 0) => 0xe200,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) SP_EL2 = (3, 6, 4, 1, 0) => 0xf208,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) TCR_EL2 = (3, 4, 2, 0, 2) => 0xe102,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) TPIDR_EL2 = (3, 4, 13, 0, 2) => 0xe682,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) TTBR0_EL2 = (3, 4, 2, 0, 0) => 0xe100,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) TTBR1_EL2 = (3, 4, 2, 0, 1) => 0xe101,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VBAR_EL2 = (3, 4, 12, 0, 0) => 0xe600,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VMPIDR_EL2 = (3, 4, 0, 0, 5) => 0xe005,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VPIDR_EL2 = (3, 4, 0, 0, 0) => 0xe000,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VTCR_EL2 = (3, 4, 2, 1, 2) => 0xe10a,
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VTTBR_EL2 = (3, 4, 2, 1, 0) => 0xe108,
}

/// A 128-bit SIMD/FP register of a vCPU (`hv_simd_fp_reg_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SimdReg(pub hv::hv_simd_fp_reg_t);

impl SimdReg {
    /// Register `Qn`, or `None` if `n > 31`.
    pub fn q(n: u8) -> Option<SimdReg> {
        (n <= 31).then(|| SimdReg(hv::HV_SIMD_FP_REG_Q0 + hv::hv_simd_fp_reg_t::from(n)))
    }
}

/// A feature register readable from a vCPU configuration
/// (`hv_feature_reg_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FeatureReg(pub hv::hv_feature_reg_t);

impl FeatureReg {
    /// `ID_AA64DFR0_EL1`.
    pub const ID_AA64DFR0_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64DFR0_EL1);
    /// `ID_AA64DFR1_EL1`.
    pub const ID_AA64DFR1_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64DFR1_EL1);
    /// `ID_AA64ISAR0_EL1`.
    pub const ID_AA64ISAR0_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64ISAR0_EL1);
    /// `ID_AA64ISAR1_EL1`.
    pub const ID_AA64ISAR1_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64ISAR1_EL1);
    /// `ID_AA64MMFR0_EL1`.
    pub const ID_AA64MMFR0_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64MMFR0_EL1);
    /// `ID_AA64MMFR1_EL1`.
    pub const ID_AA64MMFR1_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64MMFR1_EL1);
    /// `ID_AA64MMFR2_EL1`.
    pub const ID_AA64MMFR2_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64MMFR2_EL1);
    /// `ID_AA64PFR0_EL1`.
    pub const ID_AA64PFR0_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64PFR0_EL1);
    /// `ID_AA64PFR1_EL1`.
    pub const ID_AA64PFR1_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64PFR1_EL1);
    /// `CTR_EL0`.
    pub const CTR_EL0: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_CTR_EL0);
    /// `CLIDR_EL1`.
    pub const CLIDR_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_CLIDR_EL1);
    /// `DCZID_EL0`.
    pub const DCZID_EL0: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_DCZID_EL0);
    /// `ID_AA64SMFR0_EL1` (macOS 15.2).
    pub const ID_AA64SMFR0_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64SMFR0_EL1);
    /// `ID_AA64ZFR0_EL1` (macOS 15.2).
    pub const ID_AA64ZFR0_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64ZFR0_EL1);
    /// `ID_AA64ISAR2_EL1` (macOS 27.0).
    pub const ID_AA64ISAR2_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64ISAR2_EL1);
    /// `ID_AA64PFR2_EL1` (macOS 27.0).
    pub const ID_AA64PFR2_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64PFR2_EL1);
    /// `ID_AA64MMFR3_EL1` (macOS 27.0).
    pub const ID_AA64MMFR3_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64MMFR3_EL1);
    /// `ID_AA64MMFR4_EL1` (macOS 27.0).
    pub const ID_AA64MMFR4_EL1: FeatureReg = FeatureReg(hv::HV_FEATURE_REG_ID_AA64MMFR4_EL1);
}

/// Cache type for `VcpuConfig::ccsidr_el1_values`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum CacheType {
    /// Data or unified cache.
    Data = hv::HV_CACHE_TYPE_DATA,
    /// Instruction cache.
    Instruction = hv::HV_CACHE_TYPE_INSTRUCTION,
}

/// Interrupt line of a vCPU (`hv_interrupt_type_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum InterruptType {
    /// The IRQ line.
    Irq = hv::HV_INTERRUPT_TYPE_IRQ,
    /// The FIQ line.
    Fiq = hv::HV_INTERRUPT_TYPE_FIQ,
}

/// TLB invalidation operation (`hv_tlbi_op_t`, macOS 27.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TlbiOp(pub hv::hv_tlbi_op_t);

impl TlbiOp {
    /// `TLBI RVAE1IS`.
    pub const RVAE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_RVAE1IS);
    /// `TLBI RVAAE1IS`.
    pub const RVAAE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_RVAAE1IS);
    /// `TLBI RVALE1IS`.
    pub const RVALE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_RVALE1IS);
    /// `TLBI RVAALE1IS`.
    pub const RVAALE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_RVAALE1IS);
    /// `TLBI VMALLE1IS`.
    pub const VMALLE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_VMALLE1IS);
    /// `TLBI VAE1IS`.
    pub const VAE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_VAE1IS);
    /// `TLBI ASIDE1IS`.
    pub const ASIDE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_ASIDE1IS);
    /// `TLBI VAAE1IS`.
    pub const VAAE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_VAAE1IS);
    /// `TLBI VALE1IS`.
    pub const VALE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_VALE1IS);
    /// `TLBI VAALE1IS`.
    pub const VAALE1IS: TlbiOp = TlbiOp(hv::HV_TLBI_OP_VAALE1IS);
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_reg_x() {
        assert_eq!(Reg::x(0), Some(Reg(hv::HV_REG_X0)));
        assert_eq!(Reg::x(1), Some(Reg(hv::HV_REG_X1)));
        assert_eq!(Reg::x(29), Some(Reg::FP));
        assert_eq!(Reg::x(30), Some(Reg::LR));
        assert_eq!(Reg::x(31), None);
        assert_eq!(Reg::x(u8::MAX), None);
        for n in 0..=30 {
            assert_eq!(Reg::x(n).unwrap().0, u32::from(n));
        }
    }

    #[test]
    fn test_simd_reg_q() {
        assert_eq!(SimdReg::q(0), Some(SimdReg(hv::HV_SIMD_FP_REG_Q0)));
        assert_eq!(SimdReg::q(31), Some(SimdReg(hv::HV_SIMD_FP_REG_Q31)));
        assert_eq!(SimdReg::q(32), None);
        assert_eq!(SimdReg::q(u8::MAX), None);
    }

    #[test]
    fn test_sys_reg_table() {
        // Every `HV_SYS_REG_*` constant of the macOS 27 SDK.
        assert_eq!(SYS_REGS.len(), 152);

        let mut names = HashSet::new();
        let mut values = HashSet::new();
        for info in SYS_REGS {
            // The constant built by `from_encoding` is the SDK value.
            assert_eq!(info.reg.0, info.raw, "{}", info.name);
            // `encoding` inverts `from_encoding`.
            let (op0, op1, crn, crm, op2) = info.reg.encoding();
            assert_eq!(
                SysReg::from_encoding(op0, op1, crn, crm, op2),
                info.reg,
                "{}",
                info.name
            );
            // Every register the framework exposes is in the system
            // register (op0 = 3) or debug (op0 = 2) space.
            assert!(op0 == 2 || op0 == 3, "{}", info.name);
            assert_eq!(info.reg.since(), info.since, "{}", info.name);
            assert!(names.insert(info.name), "duplicate name {}", info.name);
            assert!(values.insert(info.raw), "duplicate value {}", info.name);
        }
    }

    #[test]
    fn test_sys_reg_encoding() {
        // Encodings from the Arm ARM.
        let cases = [
            (SysReg::MIDR_EL1, (3, 0, 0, 0, 0)),
            (SysReg::MPIDR_EL1, (3, 0, 0, 0, 5)),
            (SysReg::SCTLR_EL1, (3, 0, 1, 0, 0)),
            (SysReg::TTBR0_EL1, (3, 0, 2, 0, 0)),
            (SysReg::TCR_EL1, (3, 0, 2, 0, 2)),
            (SysReg::ESR_EL1, (3, 0, 5, 2, 0)),
            (SysReg::MAIR_EL1, (3, 0, 10, 2, 0)),
            (SysReg::VBAR_EL1, (3, 0, 12, 0, 0)),
            (SysReg::CNTV_CTL_EL0, (3, 3, 14, 3, 1)),
            (SysReg::CNTV_CVAL_EL0, (3, 3, 14, 3, 2)),
            (SysReg::SP_EL1, (3, 4, 4, 1, 0)),
            (SysReg::HCR_EL2, (3, 4, 1, 1, 0)),
            (SysReg::MDSCR_EL1, (2, 0, 0, 2, 2)),
            (SysReg::DBGWCR15_EL1, (2, 0, 0, 15, 7)),
        ];
        for (reg, enc) in cases {
            assert_eq!(reg.encoding(), enc);
            assert_eq!(
                SysReg::from_encoding(enc.0, enc.1, enc.2, enc.3, enc.4),
                reg
            );
        }
        assert_eq!(SysReg::MPIDR_EL1.0, 0xc005);

        // Out of range fields are truncated to their width.
        assert_eq!(
            SysReg::from_encoding(0xff, 0xff, 0xff, 0xff, 0xff),
            SysReg(0xffff)
        );
        assert_eq!(
            SysReg::from_encoding(0x7, 0x8, 0x10, 0x10, 0x8),
            SysReg::from_encoding(3, 0, 0, 0, 0)
        );
    }

    #[test]
    fn test_sys_reg_matches_bindings() {
        // Every system register declared by the bindings has a `SysReg`
        // constant with the same value and availability.
        let mut found = 0;
        for &(name, ty, value, since) in hvf_bindings::regs::__CONSTS {
            let Some(name) = name.strip_prefix("HV_SYS_REG_") else {
                continue;
            };
            assert_eq!(ty, "hv_sys_reg_t");
            let info = SYS_REGS
                .iter()
                .find(|info| info.name == name)
                .unwrap_or_else(|| panic!("no SysReg::{name}"));
            assert_eq!(i128::from(info.reg.0), value, "{name}");
            assert_eq!(info.since, since, "{name}");
            found += 1;
        }
        assert!(found > 0);
    }

    #[test]
    fn test_sys_reg_since() {
        assert_eq!(SysReg::MPIDR_EL1.since(), (11, 0));
        assert_eq!(SysReg::ACTLR_EL1.since(), (15, 0));
        assert_eq!(SysReg::CNTP_CTL_EL0.since(), (15, 0));
        assert_eq!(SysReg::HCR_EL2.since(), (15, 0));
        assert_eq!(SysReg::SMCR_EL1.since(), (15, 2));
        assert_eq!(SysReg::ID_AA64PFR2_EL1.since(), (27, 0));
        // Not a register the framework knows.
        assert_eq!(SysReg(0).since(), (11, 0));
    }

    #[test]
    fn test_sys_reg_kvm_one_reg_id() {
        // ARM64_SYS_REG(3, 0, 0, 0, 5) from the Linux uapi headers.
        const KVM_MPIDR_EL1: u64 = 0x6030_0000_0013_c005;
        // ARM64_SYS_REG(3, 3, 14, 3, 1).
        const KVM_CNTV_CTL_EL0: u64 = 0x6030_0000_0013_df19;

        assert_eq!(SysReg::MPIDR_EL1.to_kvm_one_reg_id(), KVM_MPIDR_EL1);
        assert_eq!(
            SysReg::from_kvm_one_reg_id(KVM_MPIDR_EL1),
            Some(SysReg::MPIDR_EL1)
        );
        assert_eq!(SysReg::CNTV_CTL_EL0.to_kvm_one_reg_id(), KVM_CNTV_CTL_EL0);
        assert_eq!(
            SysReg::from_kvm_one_reg_id(KVM_CNTV_CTL_EL0),
            Some(SysReg::CNTV_CTL_EL0)
        );

        for info in SYS_REGS {
            let id = info.reg.to_kvm_one_reg_id();
            assert_eq!(SysReg::from_kvm_one_reg_id(id), Some(info.reg));
        }

        // KVM_REG_ARM_CORE register (regs.regs[0]).
        assert_eq!(SysReg::from_kvm_one_reg_id(0x6030_0000_0010_0000), None);
        // 32-bit sized system register id.
        assert_eq!(SysReg::from_kvm_one_reg_id(0x6020_0000_0013_c005), None);
        // Not an arm64 id (KVM_REG_X86).
        assert_eq!(SysReg::from_kvm_one_reg_id(0x2030_0000_0013_c005), None);
        assert_eq!(SysReg::from_kvm_one_reg_id(0), None);
    }

    #[test]
    fn test_enums_match_bindings() {
        assert_eq!(CacheType::Data as u32, hv::HV_CACHE_TYPE_DATA);
        assert_eq!(CacheType::Instruction as u32, hv::HV_CACHE_TYPE_INSTRUCTION);
        assert_eq!(InterruptType::Irq as u32, hv::HV_INTERRUPT_TYPE_IRQ);
        assert_eq!(InterruptType::Fiq as u32, hv::HV_INTERRUPT_TYPE_FIQ);
        assert_eq!(FeatureReg::ID_AA64PFR0_EL1.0, 7);
        assert_eq!(TlbiOp::VMALLE1IS.0, 0x18);
    }
}
