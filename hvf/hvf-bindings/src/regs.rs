// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Register identifiers (`hv_vcpu_types.h`, `hv_vcpu_config.h`).

use crate::types::hv_feature_reg_t;

hv_consts! {
    /// General purpose and special register of a vCPU.
    pub type hv_reg_t = u32 {
        /// General purpose register `X0`.
        HV_REG_X0 = 0,
        /// General purpose register `X1`.
        HV_REG_X1 = 1,
        /// General purpose register `X2`.
        HV_REG_X2 = 2,
        /// General purpose register `X3`.
        HV_REG_X3 = 3,
        /// General purpose register `X4`.
        HV_REG_X4 = 4,
        /// General purpose register `X5`.
        HV_REG_X5 = 5,
        /// General purpose register `X6`.
        HV_REG_X6 = 6,
        /// General purpose register `X7`.
        HV_REG_X7 = 7,
        /// General purpose register `X8`.
        HV_REG_X8 = 8,
        /// General purpose register `X9`.
        HV_REG_X9 = 9,
        /// General purpose register `X10`.
        HV_REG_X10 = 10,
        /// General purpose register `X11`.
        HV_REG_X11 = 11,
        /// General purpose register `X12`.
        HV_REG_X12 = 12,
        /// General purpose register `X13`.
        HV_REG_X13 = 13,
        /// General purpose register `X14`.
        HV_REG_X14 = 14,
        /// General purpose register `X15`.
        HV_REG_X15 = 15,
        /// General purpose register `X16`.
        HV_REG_X16 = 16,
        /// General purpose register `X17`.
        HV_REG_X17 = 17,
        /// General purpose register `X18`.
        HV_REG_X18 = 18,
        /// General purpose register `X19`.
        HV_REG_X19 = 19,
        /// General purpose register `X20`.
        HV_REG_X20 = 20,
        /// General purpose register `X21`.
        HV_REG_X21 = 21,
        /// General purpose register `X22`.
        HV_REG_X22 = 22,
        /// General purpose register `X23`.
        HV_REG_X23 = 23,
        /// General purpose register `X24`.
        HV_REG_X24 = 24,
        /// General purpose register `X25`.
        HV_REG_X25 = 25,
        /// General purpose register `X26`.
        HV_REG_X26 = 26,
        /// General purpose register `X27`.
        HV_REG_X27 = 27,
        /// General purpose register `X28`.
        HV_REG_X28 = 28,
        /// General purpose register `X29`.
        HV_REG_X29 = 29,
        /// General purpose register `X30`.
        HV_REG_X30 = 30,
        /// Frame pointer (`X29`).
        HV_REG_FP = 29,
        /// Link register (`X30`).
        HV_REG_LR = 30,
        /// Program counter.
        HV_REG_PC = 31,
        /// Floating-point control register.
        HV_REG_FPCR = 32,
        /// Floating-point status register.
        HV_REG_FPSR = 33,
        /// Current program status register.
        HV_REG_CPSR = 34,
    }

    /// SIMD/FP register of a vCPU.
    pub type hv_simd_fp_reg_t = u32 {
        /// SIMD/FP register `Q0`.
        HV_SIMD_FP_REG_Q0 = 0,
        /// SIMD/FP register `Q1`.
        HV_SIMD_FP_REG_Q1 = 1,
        /// SIMD/FP register `Q2`.
        HV_SIMD_FP_REG_Q2 = 2,
        /// SIMD/FP register `Q3`.
        HV_SIMD_FP_REG_Q3 = 3,
        /// SIMD/FP register `Q4`.
        HV_SIMD_FP_REG_Q4 = 4,
        /// SIMD/FP register `Q5`.
        HV_SIMD_FP_REG_Q5 = 5,
        /// SIMD/FP register `Q6`.
        HV_SIMD_FP_REG_Q6 = 6,
        /// SIMD/FP register `Q7`.
        HV_SIMD_FP_REG_Q7 = 7,
        /// SIMD/FP register `Q8`.
        HV_SIMD_FP_REG_Q8 = 8,
        /// SIMD/FP register `Q9`.
        HV_SIMD_FP_REG_Q9 = 9,
        /// SIMD/FP register `Q10`.
        HV_SIMD_FP_REG_Q10 = 10,
        /// SIMD/FP register `Q11`.
        HV_SIMD_FP_REG_Q11 = 11,
        /// SIMD/FP register `Q12`.
        HV_SIMD_FP_REG_Q12 = 12,
        /// SIMD/FP register `Q13`.
        HV_SIMD_FP_REG_Q13 = 13,
        /// SIMD/FP register `Q14`.
        HV_SIMD_FP_REG_Q14 = 14,
        /// SIMD/FP register `Q15`.
        HV_SIMD_FP_REG_Q15 = 15,
        /// SIMD/FP register `Q16`.
        HV_SIMD_FP_REG_Q16 = 16,
        /// SIMD/FP register `Q17`.
        HV_SIMD_FP_REG_Q17 = 17,
        /// SIMD/FP register `Q18`.
        HV_SIMD_FP_REG_Q18 = 18,
        /// SIMD/FP register `Q19`.
        HV_SIMD_FP_REG_Q19 = 19,
        /// SIMD/FP register `Q20`.
        HV_SIMD_FP_REG_Q20 = 20,
        /// SIMD/FP register `Q21`.
        HV_SIMD_FP_REG_Q21 = 21,
        /// SIMD/FP register `Q22`.
        HV_SIMD_FP_REG_Q22 = 22,
        /// SIMD/FP register `Q23`.
        HV_SIMD_FP_REG_Q23 = 23,
        /// SIMD/FP register `Q24`.
        HV_SIMD_FP_REG_Q24 = 24,
        /// SIMD/FP register `Q25`.
        HV_SIMD_FP_REG_Q25 = 25,
        /// SIMD/FP register `Q26`.
        HV_SIMD_FP_REG_Q26 = 26,
        /// SIMD/FP register `Q27`.
        HV_SIMD_FP_REG_Q27 = 27,
        /// SIMD/FP register `Q28`.
        HV_SIMD_FP_REG_Q28 = 28,
        /// SIMD/FP register `Q29`.
        HV_SIMD_FP_REG_Q29 = 29,
        /// SIMD/FP register `Q30`.
        HV_SIMD_FP_REG_Q30 = 30,
        /// SIMD/FP register `Q31`.
        HV_SIMD_FP_REG_Q31 = 31,
    }

    /// System register of a vCPU.
    ///
    /// The value is the `MRS`/`MSR` encoding of the register:
    /// `(op0 << 14) | (op1 << 11) | (CRn << 7) | (CRm << 3) | op2`.
    ///
    /// The physical timer registers (`CNTP_*`) are only available when the
    /// VM has an in-kernel GIC, and the `*_EL2` registers only when EL2 is
    /// enabled in the VM configuration.
    pub type hv_sys_reg_t = u16 {
        /// `DBGBVR0_EL1`.
        HV_SYS_REG_DBGBVR0_EL1 = 0x8004,
        /// `DBGBCR0_EL1`.
        HV_SYS_REG_DBGBCR0_EL1 = 0x8005,
        /// `DBGWVR0_EL1`.
        HV_SYS_REG_DBGWVR0_EL1 = 0x8006,
        /// `DBGWCR0_EL1`.
        HV_SYS_REG_DBGWCR0_EL1 = 0x8007,
        /// `DBGBVR1_EL1`.
        HV_SYS_REG_DBGBVR1_EL1 = 0x800c,
        /// `DBGBCR1_EL1`.
        HV_SYS_REG_DBGBCR1_EL1 = 0x800d,
        /// `DBGWVR1_EL1`.
        HV_SYS_REG_DBGWVR1_EL1 = 0x800e,
        /// `DBGWCR1_EL1`.
        HV_SYS_REG_DBGWCR1_EL1 = 0x800f,
        /// `MDCCINT_EL1`.
        HV_SYS_REG_MDCCINT_EL1 = 0x8010,
        /// `MDSCR_EL1`.
        HV_SYS_REG_MDSCR_EL1 = 0x8012,
        /// `DBGBVR2_EL1`.
        HV_SYS_REG_DBGBVR2_EL1 = 0x8014,
        /// `DBGBCR2_EL1`.
        HV_SYS_REG_DBGBCR2_EL1 = 0x8015,
        /// `DBGWVR2_EL1`.
        HV_SYS_REG_DBGWVR2_EL1 = 0x8016,
        /// `DBGWCR2_EL1`.
        HV_SYS_REG_DBGWCR2_EL1 = 0x8017,
        /// `DBGBVR3_EL1`.
        HV_SYS_REG_DBGBVR3_EL1 = 0x801c,
        /// `DBGBCR3_EL1`.
        HV_SYS_REG_DBGBCR3_EL1 = 0x801d,
        /// `DBGWVR3_EL1`.
        HV_SYS_REG_DBGWVR3_EL1 = 0x801e,
        /// `DBGWCR3_EL1`.
        HV_SYS_REG_DBGWCR3_EL1 = 0x801f,
        /// `DBGBVR4_EL1`.
        HV_SYS_REG_DBGBVR4_EL1 = 0x8024,
        /// `DBGBCR4_EL1`.
        HV_SYS_REG_DBGBCR4_EL1 = 0x8025,
        /// `DBGWVR4_EL1`.
        HV_SYS_REG_DBGWVR4_EL1 = 0x8026,
        /// `DBGWCR4_EL1`.
        HV_SYS_REG_DBGWCR4_EL1 = 0x8027,
        /// `DBGBVR5_EL1`.
        HV_SYS_REG_DBGBVR5_EL1 = 0x802c,
        /// `DBGBCR5_EL1`.
        HV_SYS_REG_DBGBCR5_EL1 = 0x802d,
        /// `DBGWVR5_EL1`.
        HV_SYS_REG_DBGWVR5_EL1 = 0x802e,
        /// `DBGWCR5_EL1`.
        HV_SYS_REG_DBGWCR5_EL1 = 0x802f,
        /// `DBGBVR6_EL1`.
        HV_SYS_REG_DBGBVR6_EL1 = 0x8034,
        /// `DBGBCR6_EL1`.
        HV_SYS_REG_DBGBCR6_EL1 = 0x8035,
        /// `DBGWVR6_EL1`.
        HV_SYS_REG_DBGWVR6_EL1 = 0x8036,
        /// `DBGWCR6_EL1`.
        HV_SYS_REG_DBGWCR6_EL1 = 0x8037,
        /// `DBGBVR7_EL1`.
        HV_SYS_REG_DBGBVR7_EL1 = 0x803c,
        /// `DBGBCR7_EL1`.
        HV_SYS_REG_DBGBCR7_EL1 = 0x803d,
        /// `DBGWVR7_EL1`.
        HV_SYS_REG_DBGWVR7_EL1 = 0x803e,
        /// `DBGWCR7_EL1`.
        HV_SYS_REG_DBGWCR7_EL1 = 0x803f,
        /// `DBGBVR8_EL1`.
        HV_SYS_REG_DBGBVR8_EL1 = 0x8044,
        /// `DBGBCR8_EL1`.
        HV_SYS_REG_DBGBCR8_EL1 = 0x8045,
        /// `DBGWVR8_EL1`.
        HV_SYS_REG_DBGWVR8_EL1 = 0x8046,
        /// `DBGWCR8_EL1`.
        HV_SYS_REG_DBGWCR8_EL1 = 0x8047,
        /// `DBGBVR9_EL1`.
        HV_SYS_REG_DBGBVR9_EL1 = 0x804c,
        /// `DBGBCR9_EL1`.
        HV_SYS_REG_DBGBCR9_EL1 = 0x804d,
        /// `DBGWVR9_EL1`.
        HV_SYS_REG_DBGWVR9_EL1 = 0x804e,
        /// `DBGWCR9_EL1`.
        HV_SYS_REG_DBGWCR9_EL1 = 0x804f,
        /// `DBGBVR10_EL1`.
        HV_SYS_REG_DBGBVR10_EL1 = 0x8054,
        /// `DBGBCR10_EL1`.
        HV_SYS_REG_DBGBCR10_EL1 = 0x8055,
        /// `DBGWVR10_EL1`.
        HV_SYS_REG_DBGWVR10_EL1 = 0x8056,
        /// `DBGWCR10_EL1`.
        HV_SYS_REG_DBGWCR10_EL1 = 0x8057,
        /// `DBGBVR11_EL1`.
        HV_SYS_REG_DBGBVR11_EL1 = 0x805c,
        /// `DBGBCR11_EL1`.
        HV_SYS_REG_DBGBCR11_EL1 = 0x805d,
        /// `DBGWVR11_EL1`.
        HV_SYS_REG_DBGWVR11_EL1 = 0x805e,
        /// `DBGWCR11_EL1`.
        HV_SYS_REG_DBGWCR11_EL1 = 0x805f,
        /// `DBGBVR12_EL1`.
        HV_SYS_REG_DBGBVR12_EL1 = 0x8064,
        /// `DBGBCR12_EL1`.
        HV_SYS_REG_DBGBCR12_EL1 = 0x8065,
        /// `DBGWVR12_EL1`.
        HV_SYS_REG_DBGWVR12_EL1 = 0x8066,
        /// `DBGWCR12_EL1`.
        HV_SYS_REG_DBGWCR12_EL1 = 0x8067,
        /// `DBGBVR13_EL1`.
        HV_SYS_REG_DBGBVR13_EL1 = 0x806c,
        /// `DBGBCR13_EL1`.
        HV_SYS_REG_DBGBCR13_EL1 = 0x806d,
        /// `DBGWVR13_EL1`.
        HV_SYS_REG_DBGWVR13_EL1 = 0x806e,
        /// `DBGWCR13_EL1`.
        HV_SYS_REG_DBGWCR13_EL1 = 0x806f,
        /// `DBGBVR14_EL1`.
        HV_SYS_REG_DBGBVR14_EL1 = 0x8074,
        /// `DBGBCR14_EL1`.
        HV_SYS_REG_DBGBCR14_EL1 = 0x8075,
        /// `DBGWVR14_EL1`.
        HV_SYS_REG_DBGWVR14_EL1 = 0x8076,
        /// `DBGWCR14_EL1`.
        HV_SYS_REG_DBGWCR14_EL1 = 0x8077,
        /// `DBGBVR15_EL1`.
        HV_SYS_REG_DBGBVR15_EL1 = 0x807c,
        /// `DBGBCR15_EL1`.
        HV_SYS_REG_DBGBCR15_EL1 = 0x807d,
        /// `DBGWVR15_EL1`.
        HV_SYS_REG_DBGWVR15_EL1 = 0x807e,
        /// `DBGWCR15_EL1`.
        HV_SYS_REG_DBGWCR15_EL1 = 0x807f,
        /// `MIDR_EL1`.
        HV_SYS_REG_MIDR_EL1 = 0xc000,
        /// `MPIDR_EL1`.
        HV_SYS_REG_MPIDR_EL1 = 0xc005,
        /// `ID_AA64PFR0_EL1`.
        HV_SYS_REG_ID_AA64PFR0_EL1 = 0xc020,
        /// `ID_AA64PFR1_EL1`.
        HV_SYS_REG_ID_AA64PFR1_EL1 = 0xc021,
        /// `ID_AA64PFR2_EL1`.
        @since(27, 0) HV_SYS_REG_ID_AA64PFR2_EL1 = 0xc022,
        /// `ID_AA64ZFR0_EL1`.
        @since(15, 2) HV_SYS_REG_ID_AA64ZFR0_EL1 = 0xc024,
        /// `ID_AA64SMFR0_EL1`.
        @since(15, 2) HV_SYS_REG_ID_AA64SMFR0_EL1 = 0xc025,
        /// `ID_AA64DFR0_EL1`.
        HV_SYS_REG_ID_AA64DFR0_EL1 = 0xc028,
        /// `ID_AA64DFR1_EL1`.
        HV_SYS_REG_ID_AA64DFR1_EL1 = 0xc029,
        /// `ID_AA64ISAR0_EL1`.
        HV_SYS_REG_ID_AA64ISAR0_EL1 = 0xc030,
        /// `ID_AA64ISAR1_EL1`.
        HV_SYS_REG_ID_AA64ISAR1_EL1 = 0xc031,
        /// `ID_AA64ISAR2_EL1`.
        @since(27, 0) HV_SYS_REG_ID_AA64ISAR2_EL1 = 0xc032,
        /// `ID_AA64MMFR0_EL1`.
        HV_SYS_REG_ID_AA64MMFR0_EL1 = 0xc038,
        /// `ID_AA64MMFR1_EL1`.
        HV_SYS_REG_ID_AA64MMFR1_EL1 = 0xc039,
        /// `ID_AA64MMFR2_EL1`.
        HV_SYS_REG_ID_AA64MMFR2_EL1 = 0xc03a,
        /// `ID_AA64MMFR3_EL1`.
        @since(27, 0) HV_SYS_REG_ID_AA64MMFR3_EL1 = 0xc03b,
        /// `ID_AA64MMFR4_EL1`.
        @since(27, 0) HV_SYS_REG_ID_AA64MMFR4_EL1 = 0xc03c,
        /// `SCTLR_EL1`.
        HV_SYS_REG_SCTLR_EL1 = 0xc080,
        /// `ACTLR_EL1`.
        ///
        /// Only bit 1 (`EnTSO`) is accessible. Setting it makes the vCPU use
        /// the TSO memory model instead of the weakly ordered Arm model.
        @since(15, 0) HV_SYS_REG_ACTLR_EL1 = 0xc081,
        /// `CPACR_EL1`.
        HV_SYS_REG_CPACR_EL1 = 0xc082,
        /// `SMPRI_EL1`.
        @since(15, 2) HV_SYS_REG_SMPRI_EL1 = 0xc094,
        /// `SMCR_EL1`.
        @since(15, 2) HV_SYS_REG_SMCR_EL1 = 0xc096,
        /// `TTBR0_EL1`.
        HV_SYS_REG_TTBR0_EL1 = 0xc100,
        /// `TTBR1_EL1`.
        HV_SYS_REG_TTBR1_EL1 = 0xc101,
        /// `TCR_EL1`.
        HV_SYS_REG_TCR_EL1 = 0xc102,
        /// `APIAKEYLO_EL1`.
        HV_SYS_REG_APIAKEYLO_EL1 = 0xc108,
        /// `APIAKEYHI_EL1`.
        HV_SYS_REG_APIAKEYHI_EL1 = 0xc109,
        /// `APIBKEYLO_EL1`.
        HV_SYS_REG_APIBKEYLO_EL1 = 0xc10a,
        /// `APIBKEYHI_EL1`.
        HV_SYS_REG_APIBKEYHI_EL1 = 0xc10b,
        /// `APDAKEYLO_EL1`.
        HV_SYS_REG_APDAKEYLO_EL1 = 0xc110,
        /// `APDAKEYHI_EL1`.
        HV_SYS_REG_APDAKEYHI_EL1 = 0xc111,
        /// `APDBKEYLO_EL1`.
        HV_SYS_REG_APDBKEYLO_EL1 = 0xc112,
        /// `APDBKEYHI_EL1`.
        HV_SYS_REG_APDBKEYHI_EL1 = 0xc113,
        /// `APGAKEYLO_EL1`.
        HV_SYS_REG_APGAKEYLO_EL1 = 0xc118,
        /// `APGAKEYHI_EL1`.
        HV_SYS_REG_APGAKEYHI_EL1 = 0xc119,
        /// `SPSR_EL1`.
        HV_SYS_REG_SPSR_EL1 = 0xc200,
        /// `ELR_EL1`.
        HV_SYS_REG_ELR_EL1 = 0xc201,
        /// `SP_EL0`.
        HV_SYS_REG_SP_EL0 = 0xc208,
        /// `AFSR0_EL1`.
        HV_SYS_REG_AFSR0_EL1 = 0xc288,
        /// `AFSR1_EL1`.
        HV_SYS_REG_AFSR1_EL1 = 0xc289,
        /// `ESR_EL1`.
        HV_SYS_REG_ESR_EL1 = 0xc290,
        /// `FAR_EL1`.
        HV_SYS_REG_FAR_EL1 = 0xc300,
        /// `PAR_EL1`.
        HV_SYS_REG_PAR_EL1 = 0xc3a0,
        /// `MAIR_EL1`.
        HV_SYS_REG_MAIR_EL1 = 0xc510,
        /// `AMAIR_EL1`.
        HV_SYS_REG_AMAIR_EL1 = 0xc518,
        /// `VBAR_EL1`.
        HV_SYS_REG_VBAR_EL1 = 0xc600,
        /// `CONTEXTIDR_EL1`.
        HV_SYS_REG_CONTEXTIDR_EL1 = 0xc681,
        /// `TPIDR_EL1`.
        HV_SYS_REG_TPIDR_EL1 = 0xc684,
        /// `SCXTNUM_EL1`.
        @since(15, 2) HV_SYS_REG_SCXTNUM_EL1 = 0xc687,
        /// `CNTKCTL_EL1`.
        HV_SYS_REG_CNTKCTL_EL1 = 0xc708,
        /// `CSSELR_EL1`.
        HV_SYS_REG_CSSELR_EL1 = 0xd000,
        /// `TPIDR_EL0`.
        HV_SYS_REG_TPIDR_EL0 = 0xde82,
        /// `TPIDRRO_EL0`.
        HV_SYS_REG_TPIDRRO_EL0 = 0xde83,
        /// `TPIDR2_EL0`.
        @since(15, 2) HV_SYS_REG_TPIDR2_EL0 = 0xde85,
        /// `SCXTNUM_EL0`.
        @since(15, 2) HV_SYS_REG_SCXTNUM_EL0 = 0xde87,
        /// `CNTV_CTL_EL0`.
        HV_SYS_REG_CNTV_CTL_EL0 = 0xdf19,
        /// `CNTV_CVAL_EL0`.
        HV_SYS_REG_CNTV_CVAL_EL0 = 0xdf1a,
        /// `SP_EL1`.
        HV_SYS_REG_SP_EL1 = 0xe208,

        // Physical timer registers, only available if the VM was created with
        // a GIC device (`hv_gic_create`).
        /// `CNTP_CTL_EL0`.
        @since(15, 0) HV_SYS_REG_CNTP_CTL_EL0 = 0xdf11,
        /// `CNTP_CVAL_EL0`.
        @since(15, 0) HV_SYS_REG_CNTP_CVAL_EL0 = 0xdf12,
        /// `CNTP_TVAL_EL0`.
        @since(15, 0) HV_SYS_REG_CNTP_TVAL_EL0 = 0xdf10,

        // EL2 registers, only available if EL2 was enabled in the VM
        // configuration.
        /// `CNTHCTL_EL2`.
        @since(15, 0) HV_SYS_REG_CNTHCTL_EL2 = 0xe708,
        /// `CNTHP_CTL_EL2`.
        @since(15, 0) HV_SYS_REG_CNTHP_CTL_EL2 = 0xe711,
        /// `CNTHP_CVAL_EL2`.
        @since(15, 0) HV_SYS_REG_CNTHP_CVAL_EL2 = 0xe712,
        /// `CNTHP_TVAL_EL2`.
        @since(15, 0) HV_SYS_REG_CNTHP_TVAL_EL2 = 0xe710,
        /// `CNTVOFF_EL2`.
        @since(15, 0) HV_SYS_REG_CNTVOFF_EL2 = 0xe703,
        /// `CPTR_EL2`.
        @since(15, 0) HV_SYS_REG_CPTR_EL2 = 0xe08a,
        /// `ELR_EL2`.
        @since(15, 0) HV_SYS_REG_ELR_EL2 = 0xe201,
        /// `ESR_EL2`.
        @since(15, 0) HV_SYS_REG_ESR_EL2 = 0xe290,
        /// `FAR_EL2`.
        @since(15, 0) HV_SYS_REG_FAR_EL2 = 0xe300,
        /// `HCR_EL2`.
        @since(15, 0) HV_SYS_REG_HCR_EL2 = 0xe088,
        /// `HPFAR_EL2`.
        @since(15, 0) HV_SYS_REG_HPFAR_EL2 = 0xe304,
        /// `MAIR_EL2`.
        @since(15, 0) HV_SYS_REG_MAIR_EL2 = 0xe510,
        /// `MDCR_EL2`.
        @since(15, 0) HV_SYS_REG_MDCR_EL2 = 0xe089,
        /// `SCTLR_EL2`.
        @since(15, 0) HV_SYS_REG_SCTLR_EL2 = 0xe080,
        /// `SPSR_EL2`.
        @since(15, 0) HV_SYS_REG_SPSR_EL2 = 0xe200,
        /// `SP_EL2`.
        @since(15, 0) HV_SYS_REG_SP_EL2 = 0xf208,
        /// `TCR_EL2`.
        @since(15, 0) HV_SYS_REG_TCR_EL2 = 0xe102,
        /// `TPIDR_EL2`.
        @since(15, 0) HV_SYS_REG_TPIDR_EL2 = 0xe682,
        /// `TTBR0_EL2`.
        @since(15, 0) HV_SYS_REG_TTBR0_EL2 = 0xe100,
        /// `TTBR1_EL2`.
        @since(15, 0) HV_SYS_REG_TTBR1_EL2 = 0xe101,
        /// `VBAR_EL2`.
        @since(15, 0) HV_SYS_REG_VBAR_EL2 = 0xe600,
        /// `VMPIDR_EL2`.
        @since(15, 0) HV_SYS_REG_VMPIDR_EL2 = 0xe005,
        /// `VPIDR_EL2`.
        @since(15, 0) HV_SYS_REG_VPIDR_EL2 = 0xe000,
        /// `VTCR_EL2`.
        @since(15, 0) HV_SYS_REG_VTCR_EL2 = 0xe10a,
        /// `VTTBR_EL2`.
        @since(15, 0) HV_SYS_REG_VTTBR_EL2 = 0xe108,

    }

    consts hv_feature_reg_t {
        /// `ID_AA64DFR0_EL1`.
        HV_FEATURE_REG_ID_AA64DFR0_EL1 = 0,
        /// `ID_AA64DFR1_EL1`.
        HV_FEATURE_REG_ID_AA64DFR1_EL1 = 1,
        /// `ID_AA64ISAR0_EL1`.
        HV_FEATURE_REG_ID_AA64ISAR0_EL1 = 2,
        /// `ID_AA64ISAR1_EL1`.
        HV_FEATURE_REG_ID_AA64ISAR1_EL1 = 3,
        /// `ID_AA64MMFR0_EL1`.
        HV_FEATURE_REG_ID_AA64MMFR0_EL1 = 4,
        /// `ID_AA64MMFR1_EL1`.
        HV_FEATURE_REG_ID_AA64MMFR1_EL1 = 5,
        /// `ID_AA64MMFR2_EL1`.
        HV_FEATURE_REG_ID_AA64MMFR2_EL1 = 6,
        /// `ID_AA64PFR0_EL1`.
        HV_FEATURE_REG_ID_AA64PFR0_EL1 = 7,
        /// `ID_AA64PFR1_EL1`.
        HV_FEATURE_REG_ID_AA64PFR1_EL1 = 8,
        /// `CTR_EL0`.
        HV_FEATURE_REG_CTR_EL0 = 9,
        /// `CLIDR_EL1`.
        HV_FEATURE_REG_CLIDR_EL1 = 10,
        /// `DCZID_EL0`.
        HV_FEATURE_REG_DCZID_EL0 = 11,
        /// `ID_AA64SMFR0_EL1`.
        @since(15, 2) HV_FEATURE_REG_ID_AA64SMFR0_EL1 = 12,
        /// `ID_AA64ZFR0_EL1`.
        @since(15, 2) HV_FEATURE_REG_ID_AA64ZFR0_EL1 = 13,
        /// `ID_AA64ISAR2_EL1`.
        @since(27, 0) HV_FEATURE_REG_ID_AA64ISAR2_EL1 = 14,
        /// `ID_AA64PFR2_EL1`.
        @since(27, 0) HV_FEATURE_REG_ID_AA64PFR2_EL1 = 15,
        /// `ID_AA64MMFR3_EL1`.
        @since(27, 0) HV_FEATURE_REG_ID_AA64MMFR3_EL1 = 16,
        /// `ID_AA64MMFR4_EL1`.
        @since(27, 0) HV_FEATURE_REG_ID_AA64MMFR4_EL1 = 17,
    }
}
