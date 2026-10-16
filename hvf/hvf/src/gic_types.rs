// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Plain data describing the in-kernel GICv3 (macOS 15.0).

use hvf_bindings as hv;

/// Guest physical layout of the in-kernel GICv3.
///
/// All addresses must satisfy the alignment reported by `Hvf::gic_params`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GicConfig {
    /// Base address of the distributor.
    pub distributor_base: u64,
    /// Base address of the redistributor region. The region covers the
    /// redistributors of the maximum number of vCPUs
    /// ([`GicParams::redistributor_region_size`] bytes).
    pub redistributor_base: u64,
    /// The optional GICv2m-style MSI frame.
    pub msi: Option<MsiConfig>,
}

/// Configuration of the MSI frame of the in-kernel GIC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MsiConfig {
    /// Base address of the MSI frame. Devices signal an MSI by writing the
    /// interrupt id to `base + 0x40` (`GICM_SET_SPI_NSR`).
    pub base: u64,
    /// First SPI interrupt id reserved for MSIs.
    pub intid_base: u32,
    /// Number of SPI interrupt ids reserved for MSIs. The range must lie
    /// within the SPI range of [`GicParams`].
    pub intid_count: u32,
}

/// Sizes, alignments and interrupt ranges the in-kernel GIC requires on this
/// host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GicParams {
    /// Size of the distributor region in bytes.
    pub distributor_size: u64,
    /// Required alignment of the distributor base.
    pub distributor_base_alignment: u64,
    /// Size of the whole redistributor region in bytes.
    pub redistributor_region_size: u64,
    /// Size of the redistributor of a single vCPU in bytes.
    pub redistributor_size: u64,
    /// Required alignment of the redistributor region base.
    pub redistributor_base_alignment: u64,
    /// Size of the MSI frame in bytes.
    pub msi_region_size: u64,
    /// Required alignment of the MSI frame base.
    pub msi_region_base_alignment: u64,
    /// First SPI interrupt id.
    pub spi_base: u32,
    /// Number of SPI interrupt ids.
    pub spi_count: u32,
}

/// Architectural interrupt sources whose interrupt id the framework assigns
/// (`hv_gic_intid_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GicIntid {
    /// Performance monitor interrupt.
    PerformanceMonitor,
    /// GIC maintenance interrupt (EL2 only).
    Maintenance,
    /// EL2 physical timer interrupt (EL2 only).
    El2PhysicalTimer,
    /// EL1 virtual timer interrupt.
    El1VirtualTimer,
    /// EL1 physical timer interrupt.
    El1PhysicalTimer,
}

impl GicIntid {
    /// The corresponding `hv_gic_intid_t` value.
    // Only used by the macOS/aarch64 modules.
    #[allow(dead_code)]
    pub(crate) fn raw(self) -> hv::hv_gic_intid_t {
        match self {
            GicIntid::PerformanceMonitor => hv::HV_GIC_INT_PERFORMANCE_MONITOR,
            GicIntid::Maintenance => hv::HV_GIC_INT_MAINTENANCE,
            GicIntid::El2PhysicalTimer => hv::HV_GIC_INT_EL2_PHYSICAL_TIMER,
            GicIntid::El1VirtualTimer => hv::HV_GIC_INT_EL1_VIRTUAL_TIMER,
            GicIntid::El1PhysicalTimer => hv::HV_GIC_INT_EL1_PHYSICAL_TIMER,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gic_intid_raw() {
        assert_eq!(GicIntid::PerformanceMonitor.raw(), 23);
        assert_eq!(GicIntid::Maintenance.raw(), 25);
        assert_eq!(GicIntid::El2PhysicalTimer.raw(), 26);
        assert_eq!(GicIntid::El1VirtualTimer.raw(), 27);
        assert_eq!(GicIntid::El1PhysicalTimer.raw(), 30);
    }

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<GicConfig>();
        assert_send_sync::<MsiConfig>();
        assert_send_sync::<GicParams>();
        assert_send_sync::<GicIntid>();
    }
}
