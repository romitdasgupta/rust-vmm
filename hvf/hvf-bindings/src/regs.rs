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

    /// System register of a vCPU, encoded as
    /// `(op0 << 14) | (op1 << 11) | (CRn << 7) | (CRm << 3) | op2`.
    pub type hv_sys_reg_t = u16 {
        /// `MIDR_EL1`.
        HV_SYS_REG_MIDR_EL1 = 0xc000,
        /// `MPIDR_EL1`.
        HV_SYS_REG_MPIDR_EL1 = 0xc005,
        /// `SCTLR_EL1`.
        HV_SYS_REG_SCTLR_EL1 = 0xc080,
        /// `VBAR_EL1`.
        HV_SYS_REG_VBAR_EL1 = 0xc600,
        /// `CNTV_CTL_EL0`.
        HV_SYS_REG_CNTV_CTL_EL0 = 0xdf19,
        /// `CNTV_CVAL_EL0`.
        HV_SYS_REG_CNTV_CVAL_EL0 = 0xdf1a,
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
