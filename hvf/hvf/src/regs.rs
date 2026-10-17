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
/// Each entry is `(major, minor) NAME = (op0, op1, CRn, CRm, op2),` where
/// `(major, minor)` is the first macOS release accepting the register. The
/// constant is defined through `SysReg::from_encoding`, and the unit tests
/// check its value and availability against the `HV_SYS_REG_*` constants of
/// `hvf-bindings`, which `sdk_check` verifies against the SDK header.
macro_rules! sys_regs {
    ($(
        $(#[$m:meta])*
        ($maj:literal, $min:literal) $name:ident =
            ($op0:literal, $op1:literal, $crn:literal, $crm:literal, $op2:literal),
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

// `KVM_REG_ARM64 | KVM_REG_SIZE_U64 | KVM_REG_ARM_CORE`. The low bits of a core
// register id are its offset in `struct kvm_regs` in 32-bit words.
const KVM_ARM64_CORE_ID_BASE: u64 = 0x6000_0000_0000_0000 | 0x0030_0000_0000_0000 | 0x0010 << 16;

/// The system registers KVM exposes only as `KVM_REG_ARM_CORE` registers,
/// with their core ids. `struct kvm_regs` starts with `struct user_pt_regs`
/// (`regs[31]`, `sp`, `pc`, `pstate`), followed by `sp_el1`, `elr_el1` and
/// `spsr[]`, whose element `KVM_SPSR_EL1` (0) is `SPSR_EL1`.
const KVM_CORE_SYS_REGS: [(SysReg, u64); 4] = [
    // `regs.sp`: byte offset 31 * 8.
    (SysReg::SP_EL0, KVM_ARM64_CORE_ID_BASE | 0x3e),
    // `sp_el1`: byte offset 34 * 8.
    (SysReg::SP_EL1, KVM_ARM64_CORE_ID_BASE | 0x44),
    // `elr_el1`: byte offset 35 * 8.
    (SysReg::ELR_EL1, KVM_ARM64_CORE_ID_BASE | 0x46),
    // `spsr[KVM_SPSR_EL1]`: byte offset 36 * 8.
    (SysReg::SPSR_EL1, KVM_ARM64_CORE_ID_BASE | 0x48),
];

/// Converts between a system register encoding and the low 16 bits of its
/// KVM one-reg id, which are the same except for two registers: the Linux
/// uapi defines `KVM_REG_ARM_TIMER_CVAL` as `ARM64_SYS_REG(3, 3, 14, 0, 2)`
/// (the encoding of `CNTVCT_EL0`) and `KVM_REG_ARM_TIMER_CNT` as
/// `ARM64_SYS_REG(3, 3, 14, 3, 2)` (that of `CNTV_CVAL_EL0`), an accidental
/// swap kept for compatibility. The conversion is its own inverse.
const fn kvm_swap_timer(encoding: u16) -> u16 {
    /// `CNTVCT_EL0`, `(3, 3, 14, 0, 2)`.
    const CNTVCT_EL0: u16 = 0xdf02;
    /// `CNTV_CVAL_EL0`, `(3, 3, 14, 3, 2)`.
    const CNTV_CVAL_EL0: u16 = 0xdf1a;
    match encoding {
        CNTVCT_EL0 => CNTV_CVAL_EL0,
        CNTV_CVAL_EL0 => CNTVCT_EL0,
        other => other,
    }
}

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
    ///
    /// Like KVM, this maps `KVM_REG_ARM_TIMER_CVAL` to `CNTV_CVAL_EL0` and
    /// `KVM_REG_ARM_TIMER_CNT` to `CNTVCT_EL0`: the Linux uapi defines those
    /// two ids with each other's encodings.
    ///
    /// KVM has no system register ids for `SP_EL0`, `SP_EL1`, `ELR_EL1` and
    /// `SPSR_EL1`; it exposes them as `KVM_REG_ARM_CORE` registers
    /// (`regs.sp`, `sp_el1`, `elr_el1` and `spsr[KVM_SPSR_EL1]` of
    /// `struct kvm_regs`). Those four core ids map to these registers, and
    /// their encodings in the system register space map to `None`.
    pub fn from_kvm_one_reg_id(id: u64) -> Option<SysReg> {
        if let Some(&(reg, _)) = KVM_CORE_SYS_REGS.iter().find(|&&(_, core)| core == id) {
            return Some(reg);
        }
        if id & !0xffff != KVM_ARM64_SYSREG_ID_BASE {
            return None;
        }
        let reg = SysReg(kvm_swap_timer(id as u16));
        (!KVM_CORE_SYS_REGS.iter().any(|&(r, _)| r == reg)).then_some(reg)
    }

    /// The equivalent KVM one-reg id: a `KVM_REG_ARM64_SYSREG` id, with KVM's
    /// swapped timer ids, or the `KVM_REG_ARM_CORE` id of `SP_EL0`, `SP_EL1`,
    /// `ELR_EL1` and `SPSR_EL1` (see
    /// [`from_kvm_one_reg_id`](Self::from_kvm_one_reg_id)).
    pub fn to_kvm_one_reg_id(self) -> u64 {
        match KVM_CORE_SYS_REGS.iter().find(|&&(reg, _)| reg == self) {
            Some(&(_, core)) => core,
            None => KVM_ARM64_SYSREG_ID_BASE | u64::from(kvm_swap_timer(self.0)),
        }
    }
}

sys_regs! {
    (11, 0) DBGBVR0_EL1 = (2, 0, 0, 0, 4),
    (11, 0) DBGBCR0_EL1 = (2, 0, 0, 0, 5),
    (11, 0) DBGWVR0_EL1 = (2, 0, 0, 0, 6),
    (11, 0) DBGWCR0_EL1 = (2, 0, 0, 0, 7),
    (11, 0) DBGBVR1_EL1 = (2, 0, 0, 1, 4),
    (11, 0) DBGBCR1_EL1 = (2, 0, 0, 1, 5),
    (11, 0) DBGWVR1_EL1 = (2, 0, 0, 1, 6),
    (11, 0) DBGWCR1_EL1 = (2, 0, 0, 1, 7),
    (11, 0) MDCCINT_EL1 = (2, 0, 0, 2, 0),
    (11, 0) MDSCR_EL1 = (2, 0, 0, 2, 2),
    (11, 0) DBGBVR2_EL1 = (2, 0, 0, 2, 4),
    (11, 0) DBGBCR2_EL1 = (2, 0, 0, 2, 5),
    (11, 0) DBGWVR2_EL1 = (2, 0, 0, 2, 6),
    (11, 0) DBGWCR2_EL1 = (2, 0, 0, 2, 7),
    (11, 0) DBGBVR3_EL1 = (2, 0, 0, 3, 4),
    (11, 0) DBGBCR3_EL1 = (2, 0, 0, 3, 5),
    (11, 0) DBGWVR3_EL1 = (2, 0, 0, 3, 6),
    (11, 0) DBGWCR3_EL1 = (2, 0, 0, 3, 7),
    (11, 0) DBGBVR4_EL1 = (2, 0, 0, 4, 4),
    (11, 0) DBGBCR4_EL1 = (2, 0, 0, 4, 5),
    (11, 0) DBGWVR4_EL1 = (2, 0, 0, 4, 6),
    (11, 0) DBGWCR4_EL1 = (2, 0, 0, 4, 7),
    (11, 0) DBGBVR5_EL1 = (2, 0, 0, 5, 4),
    (11, 0) DBGBCR5_EL1 = (2, 0, 0, 5, 5),
    (11, 0) DBGWVR5_EL1 = (2, 0, 0, 5, 6),
    (11, 0) DBGWCR5_EL1 = (2, 0, 0, 5, 7),
    (11, 0) DBGBVR6_EL1 = (2, 0, 0, 6, 4),
    (11, 0) DBGBCR6_EL1 = (2, 0, 0, 6, 5),
    (11, 0) DBGWVR6_EL1 = (2, 0, 0, 6, 6),
    (11, 0) DBGWCR6_EL1 = (2, 0, 0, 6, 7),
    (11, 0) DBGBVR7_EL1 = (2, 0, 0, 7, 4),
    (11, 0) DBGBCR7_EL1 = (2, 0, 0, 7, 5),
    (11, 0) DBGWVR7_EL1 = (2, 0, 0, 7, 6),
    (11, 0) DBGWCR7_EL1 = (2, 0, 0, 7, 7),
    (11, 0) DBGBVR8_EL1 = (2, 0, 0, 8, 4),
    (11, 0) DBGBCR8_EL1 = (2, 0, 0, 8, 5),
    (11, 0) DBGWVR8_EL1 = (2, 0, 0, 8, 6),
    (11, 0) DBGWCR8_EL1 = (2, 0, 0, 8, 7),
    (11, 0) DBGBVR9_EL1 = (2, 0, 0, 9, 4),
    (11, 0) DBGBCR9_EL1 = (2, 0, 0, 9, 5),
    (11, 0) DBGWVR9_EL1 = (2, 0, 0, 9, 6),
    (11, 0) DBGWCR9_EL1 = (2, 0, 0, 9, 7),
    (11, 0) DBGBVR10_EL1 = (2, 0, 0, 10, 4),
    (11, 0) DBGBCR10_EL1 = (2, 0, 0, 10, 5),
    (11, 0) DBGWVR10_EL1 = (2, 0, 0, 10, 6),
    (11, 0) DBGWCR10_EL1 = (2, 0, 0, 10, 7),
    (11, 0) DBGBVR11_EL1 = (2, 0, 0, 11, 4),
    (11, 0) DBGBCR11_EL1 = (2, 0, 0, 11, 5),
    (11, 0) DBGWVR11_EL1 = (2, 0, 0, 11, 6),
    (11, 0) DBGWCR11_EL1 = (2, 0, 0, 11, 7),
    (11, 0) DBGBVR12_EL1 = (2, 0, 0, 12, 4),
    (11, 0) DBGBCR12_EL1 = (2, 0, 0, 12, 5),
    (11, 0) DBGWVR12_EL1 = (2, 0, 0, 12, 6),
    (11, 0) DBGWCR12_EL1 = (2, 0, 0, 12, 7),
    (11, 0) DBGBVR13_EL1 = (2, 0, 0, 13, 4),
    (11, 0) DBGBCR13_EL1 = (2, 0, 0, 13, 5),
    (11, 0) DBGWVR13_EL1 = (2, 0, 0, 13, 6),
    (11, 0) DBGWCR13_EL1 = (2, 0, 0, 13, 7),
    (11, 0) DBGBVR14_EL1 = (2, 0, 0, 14, 4),
    (11, 0) DBGBCR14_EL1 = (2, 0, 0, 14, 5),
    (11, 0) DBGWVR14_EL1 = (2, 0, 0, 14, 6),
    (11, 0) DBGWCR14_EL1 = (2, 0, 0, 14, 7),
    (11, 0) DBGBVR15_EL1 = (2, 0, 0, 15, 4),
    (11, 0) DBGBCR15_EL1 = (2, 0, 0, 15, 5),
    (11, 0) DBGWVR15_EL1 = (2, 0, 0, 15, 6),
    (11, 0) DBGWCR15_EL1 = (2, 0, 0, 15, 7),
    (11, 0) MIDR_EL1 = (3, 0, 0, 0, 0),
    (11, 0) MPIDR_EL1 = (3, 0, 0, 0, 5),
    (11, 0) ID_AA64PFR0_EL1 = (3, 0, 0, 4, 0),
    (11, 0) ID_AA64PFR1_EL1 = (3, 0, 0, 4, 1),
    (27, 0) ID_AA64PFR2_EL1 = (3, 0, 0, 4, 2),
    (15, 2) ID_AA64ZFR0_EL1 = (3, 0, 0, 4, 4),
    (15, 2) ID_AA64SMFR0_EL1 = (3, 0, 0, 4, 5),
    (11, 0) ID_AA64DFR0_EL1 = (3, 0, 0, 5, 0),
    (11, 0) ID_AA64DFR1_EL1 = (3, 0, 0, 5, 1),
    (11, 0) ID_AA64ISAR0_EL1 = (3, 0, 0, 6, 0),
    (11, 0) ID_AA64ISAR1_EL1 = (3, 0, 0, 6, 1),
    (27, 0) ID_AA64ISAR2_EL1 = (3, 0, 0, 6, 2),
    (11, 0) ID_AA64MMFR0_EL1 = (3, 0, 0, 7, 0),
    (11, 0) ID_AA64MMFR1_EL1 = (3, 0, 0, 7, 1),
    (11, 0) ID_AA64MMFR2_EL1 = (3, 0, 0, 7, 2),
    (27, 0) ID_AA64MMFR3_EL1 = (3, 0, 0, 7, 3),
    (27, 0) ID_AA64MMFR4_EL1 = (3, 0, 0, 7, 4),
    (11, 0) SCTLR_EL1 = (3, 0, 1, 0, 0),
    /// Only bit 1 (`EnTSO`, total store ordering) is accessible.
    (15, 0) ACTLR_EL1 = (3, 0, 1, 0, 1),
    (11, 0) CPACR_EL1 = (3, 0, 1, 0, 2),
    (15, 2) SMPRI_EL1 = (3, 0, 1, 2, 4),
    (15, 2) SMCR_EL1 = (3, 0, 1, 2, 6),
    (11, 0) TTBR0_EL1 = (3, 0, 2, 0, 0),
    (11, 0) TTBR1_EL1 = (3, 0, 2, 0, 1),
    (11, 0) TCR_EL1 = (3, 0, 2, 0, 2),
    (11, 0) APIAKEYLO_EL1 = (3, 0, 2, 1, 0),
    (11, 0) APIAKEYHI_EL1 = (3, 0, 2, 1, 1),
    (11, 0) APIBKEYLO_EL1 = (3, 0, 2, 1, 2),
    (11, 0) APIBKEYHI_EL1 = (3, 0, 2, 1, 3),
    (11, 0) APDAKEYLO_EL1 = (3, 0, 2, 2, 0),
    (11, 0) APDAKEYHI_EL1 = (3, 0, 2, 2, 1),
    (11, 0) APDBKEYLO_EL1 = (3, 0, 2, 2, 2),
    (11, 0) APDBKEYHI_EL1 = (3, 0, 2, 2, 3),
    (11, 0) APGAKEYLO_EL1 = (3, 0, 2, 3, 0),
    (11, 0) APGAKEYHI_EL1 = (3, 0, 2, 3, 1),
    (11, 0) SPSR_EL1 = (3, 0, 4, 0, 0),
    (11, 0) ELR_EL1 = (3, 0, 4, 0, 1),
    (11, 0) SP_EL0 = (3, 0, 4, 1, 0),
    (11, 0) AFSR0_EL1 = (3, 0, 5, 1, 0),
    (11, 0) AFSR1_EL1 = (3, 0, 5, 1, 1),
    (11, 0) ESR_EL1 = (3, 0, 5, 2, 0),
    (11, 0) FAR_EL1 = (3, 0, 6, 0, 0),
    (11, 0) PAR_EL1 = (3, 0, 7, 4, 0),
    (11, 0) MAIR_EL1 = (3, 0, 10, 2, 0),
    (11, 0) AMAIR_EL1 = (3, 0, 10, 3, 0),
    (11, 0) VBAR_EL1 = (3, 0, 12, 0, 0),
    (11, 0) CONTEXTIDR_EL1 = (3, 0, 13, 0, 1),
    (11, 0) TPIDR_EL1 = (3, 0, 13, 0, 4),
    (15, 2) SCXTNUM_EL1 = (3, 0, 13, 0, 7),
    (11, 0) CNTKCTL_EL1 = (3, 0, 14, 1, 0),
    (11, 0) CSSELR_EL1 = (3, 2, 0, 0, 0),
    (11, 0) TPIDR_EL0 = (3, 3, 13, 0, 2),
    (11, 0) TPIDRRO_EL0 = (3, 3, 13, 0, 3),
    (15, 2) TPIDR2_EL0 = (3, 3, 13, 0, 5),
    (15, 2) SCXTNUM_EL0 = (3, 3, 13, 0, 7),
    (11, 0) CNTV_CTL_EL0 = (3, 3, 14, 3, 1),
    (11, 0) CNTV_CVAL_EL0 = (3, 3, 14, 3, 2),
    (11, 0) SP_EL1 = (3, 4, 4, 1, 0),

    // Physical timer registers: only accessible when the VM has a GIC.
    /// Only accessible when the VM has a GIC.
    (15, 0) CNTP_CTL_EL0 = (3, 3, 14, 2, 1),
    /// Only accessible when the VM has a GIC.
    (15, 0) CNTP_CVAL_EL0 = (3, 3, 14, 2, 2),
    /// Only accessible when the VM has a GIC.
    (15, 0) CNTP_TVAL_EL0 = (3, 3, 14, 2, 0),

    // EL2 registers: only accessible when EL2 is enabled in the VM configuration.
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTHCTL_EL2 = (3, 4, 14, 1, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTHP_CTL_EL2 = (3, 4, 14, 2, 1),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTHP_CVAL_EL2 = (3, 4, 14, 2, 2),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTHP_TVAL_EL2 = (3, 4, 14, 2, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CNTVOFF_EL2 = (3, 4, 14, 0, 3),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) CPTR_EL2 = (3, 4, 1, 1, 2),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) ELR_EL2 = (3, 4, 4, 0, 1),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) ESR_EL2 = (3, 4, 5, 2, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) FAR_EL2 = (3, 4, 6, 0, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) HCR_EL2 = (3, 4, 1, 1, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) HPFAR_EL2 = (3, 4, 6, 0, 4),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) MAIR_EL2 = (3, 4, 10, 2, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) MDCR_EL2 = (3, 4, 1, 1, 1),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) SCTLR_EL2 = (3, 4, 1, 0, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) SPSR_EL2 = (3, 4, 4, 0, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) SP_EL2 = (3, 6, 4, 1, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) TCR_EL2 = (3, 4, 2, 0, 2),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) TPIDR_EL2 = (3, 4, 13, 0, 2),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) TTBR0_EL2 = (3, 4, 2, 0, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) TTBR1_EL2 = (3, 4, 2, 0, 1),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VBAR_EL2 = (3, 4, 12, 0, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VMPIDR_EL2 = (3, 4, 0, 0, 5),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VPIDR_EL2 = (3, 4, 0, 0, 0),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VTCR_EL2 = (3, 4, 2, 1, 2),
    /// Only accessible when EL2 is enabled in the VM configuration.
    (15, 0) VTTBR_EL2 = (3, 4, 2, 1, 0),
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
            // `test_sys_reg_matches_bindings` checks the value built by
            // `from_encoding` against the SDK constant.
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
            assert!(values.insert(info.reg.0), "duplicate value {}", info.name);
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

        // KVM_REG_ARM_TIMER_CVAL and KVM_REG_ARM_TIMER_CNT from the Linux
        // uapi headers, which swap the encodings of the two registers.
        const KVM_REG_ARM_TIMER_CVAL: u64 = 0x6030_0000_0013_df02;
        const KVM_REG_ARM_TIMER_CNT: u64 = 0x6030_0000_0013_df1a;
        let cntvct_el0 = SysReg::from_encoding(3, 3, 14, 0, 2);
        assert_eq!(
            SysReg::CNTV_CVAL_EL0.to_kvm_one_reg_id(),
            KVM_REG_ARM_TIMER_CVAL
        );
        assert_eq!(
            SysReg::from_kvm_one_reg_id(KVM_REG_ARM_TIMER_CVAL),
            Some(SysReg::CNTV_CVAL_EL0)
        );
        assert_eq!(cntvct_el0.to_kvm_one_reg_id(), KVM_REG_ARM_TIMER_CNT);
        assert_eq!(
            SysReg::from_kvm_one_reg_id(KVM_REG_ARM_TIMER_CNT),
            Some(cntvct_el0)
        );

        for info in SYS_REGS {
            let id = info.reg.to_kvm_one_reg_id();
            assert_eq!(SysReg::from_kvm_one_reg_id(id), Some(info.reg));
        }

        // KVM exposes these only as KVM_REG_ARM_CORE registers:
        // KVM_REG_ARM_CORE_REG(regs.sp), (sp_el1), (elr_el1) and
        // (spsr[KVM_SPSR_EL1]) from the Linux uapi headers.
        for (reg, core, sysreg) in [
            (SysReg::SP_EL0, 0x6030_0000_0010_003e, 0x6030_0000_0013_c208),
            (SysReg::SP_EL1, 0x6030_0000_0010_0044, 0x6030_0000_0013_e208),
            (
                SysReg::ELR_EL1,
                0x6030_0000_0010_0046,
                0x6030_0000_0013_c201,
            ),
            (
                SysReg::SPSR_EL1,
                0x6030_0000_0010_0048,
                0x6030_0000_0013_c200,
            ),
        ] {
            assert_eq!(reg.to_kvm_one_reg_id(), core, "{reg:?}");
            assert_eq!(SysReg::from_kvm_one_reg_id(core), Some(reg));
            // KVM rejects these system register ids with ENOENT.
            assert_eq!(SysReg::from_kvm_one_reg_id(sysreg), None, "{reg:?}");
        }

        // Other KVM_REG_ARM_CORE registers (regs.regs[0], regs.pc).
        assert_eq!(SysReg::from_kvm_one_reg_id(0x6030_0000_0010_0000), None);
        assert_eq!(SysReg::from_kvm_one_reg_id(0x6030_0000_0010_0040), None);
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
