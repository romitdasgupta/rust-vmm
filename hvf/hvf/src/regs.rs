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
    pub fn x(_n: u8) -> Option<Reg> {
        todo!()
    }
}

/// A system register of a vCPU (`hv_sys_reg_t`), encoded as
/// `(op0 << 14) | (op1 << 11) | (CRn << 7) | (CRm << 3) | op2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SysReg(pub hv::hv_sys_reg_t);

impl SysReg {
    /// `MIDR_EL1`.
    pub const MIDR_EL1: SysReg = SysReg(hv::HV_SYS_REG_MIDR_EL1);
    /// `MPIDR_EL1`.
    pub const MPIDR_EL1: SysReg = SysReg(hv::HV_SYS_REG_MPIDR_EL1);
    /// `SCTLR_EL1`.
    pub const SCTLR_EL1: SysReg = SysReg(hv::HV_SYS_REG_SCTLR_EL1);
    /// `VBAR_EL1`.
    pub const VBAR_EL1: SysReg = SysReg(hv::HV_SYS_REG_VBAR_EL1);
    /// `CNTV_CTL_EL0`.
    pub const CNTV_CTL_EL0: SysReg = SysReg(hv::HV_SYS_REG_CNTV_CTL_EL0);
    /// `CNTV_CVAL_EL0`.
    pub const CNTV_CVAL_EL0: SysReg = SysReg(hv::HV_SYS_REG_CNTV_CVAL_EL0);

    /// Builds the identifier from the `MRS`/`MSR` operand fields.
    pub const fn from_encoding(_op0: u8, _op1: u8, _crn: u8, _crm: u8, _op2: u8) -> SysReg {
        todo!()
    }

    /// The `(op0, op1, CRn, CRm, op2)` operand fields.
    pub fn encoding(self) -> (u8, u8, u8, u8, u8) {
        todo!()
    }

    /// The first macOS release `(major, minor)` that accepts this register.
    pub fn since(self) -> (u16, u16) {
        todo!()
    }

    /// Converts a KVM `KVM_REG_ARM64_SYSREG` one-reg id, or returns `None` if
    /// `id` is not a 64-bit arm64 system register id.
    pub fn from_kvm_one_reg_id(_id: u64) -> Option<SysReg> {
        todo!()
    }

    /// The equivalent KVM `KVM_REG_ARM64_SYSREG` one-reg id.
    pub fn to_kvm_one_reg_id(self) -> u64 {
        todo!()
    }
}

/// A 128-bit SIMD/FP register of a vCPU (`hv_simd_fp_reg_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SimdReg(pub hv::hv_simd_fp_reg_t);

impl SimdReg {
    /// Register `Qn`, or `None` if `n > 31`.
    pub fn q(_n: u8) -> Option<SimdReg> {
        todo!()
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
