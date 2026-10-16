// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! In-kernel GICv3 types and register identifiers (`hv_gic_types.h`,
//! macOS 15.0).

hv_consts! {
    /// Interrupt identifiers the framework reserves for architectural
    /// interrupt sources, for `hv_gic_get_intid`.
    pub type hv_gic_intid_t = u16 {
        /// Performance monitor interrupt.
        @since(15, 0) HV_GIC_INT_PERFORMANCE_MONITOR = 23,
        /// GIC maintenance interrupt (EL2 only).
        @since(15, 0) HV_GIC_INT_MAINTENANCE = 25,
        /// EL2 physical timer interrupt (EL2 only).
        @since(15, 0) HV_GIC_INT_EL2_PHYSICAL_TIMER = 26,
        /// EL1 virtual timer interrupt.
        @since(15, 0) HV_GIC_INT_EL1_VIRTUAL_TIMER = 27,
        /// EL1 physical timer interrupt.
        @since(15, 0) HV_GIC_INT_EL1_PHYSICAL_TIMER = 30,
    }

    /// GIC distributor register (the value is the MMIO offset).
    pub type hv_gic_distributor_reg_t = u16 {
        /// `GICD_CTLR`.
        @since(15, 0) HV_GIC_DISTRIBUTOR_REG_GICD_CTLR = 0x0000,
    }

    /// GIC redistributor register (the value is the MMIO offset).
    pub type hv_gic_redistributor_reg_t = u32 {
        /// `GICR_TYPER`.
        @since(15, 0) HV_GIC_REDISTRIBUTOR_REG_GICR_TYPER = 0x0008,
    }

    /// GIC CPU system register.
    pub type hv_gic_icc_reg_t = u16 {
        /// `ICC_PMR_EL1`.
        @since(15, 0) HV_GIC_ICC_REG_PMR_EL1 = 0xc230,
    }

    /// GIC virtual CPU system register (EL2 only).
    pub type hv_gic_icv_reg_t = u16 {}

    /// GIC hypervisor control register (EL2 only).
    pub type hv_gic_ich_reg_t = u16 {}

    /// GIC MSI frame register.
    pub type hv_gic_msi_reg_t = u16 {
        /// `GICM_TYPER`.
        @since(15, 0) HV_GIC_REG_GICM_TYPER = 0x0008,
        /// `GICM_SET_SPI_NSR`.
        @since(15, 0) HV_GIC_REG_GICM_SET_SPI_NSR = 0x0040,
    }
}
