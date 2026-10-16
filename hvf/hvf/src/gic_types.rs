// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Plain data describing the in-kernel GICv3 (macOS 15.0).

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
