// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! SME register identifiers (`hv_vcpu_types.h`, macOS 15.2).

hv_consts! {
    /// SME `Z` vector register of a vCPU.
    pub type hv_sme_z_reg_t = u32 {
        /// SME `Z0` register.
        @since(15, 2) HV_SME_Z_REG_0 = 0,
        /// SME `Z1` register.
        @since(15, 2) HV_SME_Z_REG_1 = 1,
        /// SME `Z2` register.
        @since(15, 2) HV_SME_Z_REG_2 = 2,
        /// SME `Z3` register.
        @since(15, 2) HV_SME_Z_REG_3 = 3,
        /// SME `Z4` register.
        @since(15, 2) HV_SME_Z_REG_4 = 4,
        /// SME `Z5` register.
        @since(15, 2) HV_SME_Z_REG_5 = 5,
        /// SME `Z6` register.
        @since(15, 2) HV_SME_Z_REG_6 = 6,
        /// SME `Z7` register.
        @since(15, 2) HV_SME_Z_REG_7 = 7,
        /// SME `Z8` register.
        @since(15, 2) HV_SME_Z_REG_8 = 8,
        /// SME `Z9` register.
        @since(15, 2) HV_SME_Z_REG_9 = 9,
        /// SME `Z10` register.
        @since(15, 2) HV_SME_Z_REG_10 = 10,
        /// SME `Z11` register.
        @since(15, 2) HV_SME_Z_REG_11 = 11,
        /// SME `Z12` register.
        @since(15, 2) HV_SME_Z_REG_12 = 12,
        /// SME `Z13` register.
        @since(15, 2) HV_SME_Z_REG_13 = 13,
        /// SME `Z14` register.
        @since(15, 2) HV_SME_Z_REG_14 = 14,
        /// SME `Z15` register.
        @since(15, 2) HV_SME_Z_REG_15 = 15,
        /// SME `Z16` register.
        @since(15, 2) HV_SME_Z_REG_16 = 16,
        /// SME `Z17` register.
        @since(15, 2) HV_SME_Z_REG_17 = 17,
        /// SME `Z18` register.
        @since(15, 2) HV_SME_Z_REG_18 = 18,
        /// SME `Z19` register.
        @since(15, 2) HV_SME_Z_REG_19 = 19,
        /// SME `Z20` register.
        @since(15, 2) HV_SME_Z_REG_20 = 20,
        /// SME `Z21` register.
        @since(15, 2) HV_SME_Z_REG_21 = 21,
        /// SME `Z22` register.
        @since(15, 2) HV_SME_Z_REG_22 = 22,
        /// SME `Z23` register.
        @since(15, 2) HV_SME_Z_REG_23 = 23,
        /// SME `Z24` register.
        @since(15, 2) HV_SME_Z_REG_24 = 24,
        /// SME `Z25` register.
        @since(15, 2) HV_SME_Z_REG_25 = 25,
        /// SME `Z26` register.
        @since(15, 2) HV_SME_Z_REG_26 = 26,
        /// SME `Z27` register.
        @since(15, 2) HV_SME_Z_REG_27 = 27,
        /// SME `Z28` register.
        @since(15, 2) HV_SME_Z_REG_28 = 28,
        /// SME `Z29` register.
        @since(15, 2) HV_SME_Z_REG_29 = 29,
        /// SME `Z30` register.
        @since(15, 2) HV_SME_Z_REG_30 = 30,
        /// SME `Z31` register.
        @since(15, 2) HV_SME_Z_REG_31 = 31,
    }

    /// SME `P` predicate register of a vCPU.
    pub type hv_sme_p_reg_t = u32 {
        /// SME `P0` register.
        @since(15, 2) HV_SME_P_REG_0 = 0,
        /// SME `P1` register.
        @since(15, 2) HV_SME_P_REG_1 = 1,
        /// SME `P2` register.
        @since(15, 2) HV_SME_P_REG_2 = 2,
        /// SME `P3` register.
        @since(15, 2) HV_SME_P_REG_3 = 3,
        /// SME `P4` register.
        @since(15, 2) HV_SME_P_REG_4 = 4,
        /// SME `P5` register.
        @since(15, 2) HV_SME_P_REG_5 = 5,
        /// SME `P6` register.
        @since(15, 2) HV_SME_P_REG_6 = 6,
        /// SME `P7` register.
        @since(15, 2) HV_SME_P_REG_7 = 7,
        /// SME `P8` register.
        @since(15, 2) HV_SME_P_REG_8 = 8,
        /// SME `P9` register.
        @since(15, 2) HV_SME_P_REG_9 = 9,
        /// SME `P10` register.
        @since(15, 2) HV_SME_P_REG_10 = 10,
        /// SME `P11` register.
        @since(15, 2) HV_SME_P_REG_11 = 11,
        /// SME `P12` register.
        @since(15, 2) HV_SME_P_REG_12 = 12,
        /// SME `P13` register.
        @since(15, 2) HV_SME_P_REG_13 = 13,
        /// SME `P14` register.
        @since(15, 2) HV_SME_P_REG_14 = 14,
        /// SME `P15` register.
        @since(15, 2) HV_SME_P_REG_15 = 15,
    }
}
