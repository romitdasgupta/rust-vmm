// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! The in-kernel GICv3 (macOS 15.0).
//!
//! Ordering rules enforced by the framework and documented here: the GIC
//! must be created before the first vCPU; [`Gic::save_state`] is only
//! meaningful while no vCPU runs, and [`Gic::restore_state`] must happen
//! after every vCPU is created and before the first run.

use std::sync::Arc;

use crate::error::Result;
use crate::gic_types::GicConfig;
use crate::vcpu::Vcpu;
use crate::vm::{Vm, VmInner};

/// A handle to the in-kernel GIC of the VM. VM-wide GIC operations work from
/// any thread.
#[derive(Debug, Clone)]
pub struct Gic {
    #[allow(dead_code)]
    pub(crate) vm: Arc<VmInner>,
    pub(crate) cfg: GicConfig,
}

impl Vm {
    /// Creates the in-kernel GIC with the layout `cfg`.
    ///
    /// Fails with [`Error::GicAfterVcpu`](crate::Error::GicAfterVcpu) once a
    /// vCPU exists and with [`Error::GicExists`](crate::Error::GicExists) if
    /// the GIC already exists.
    pub fn create_gic(&self, _cfg: &GicConfig) -> Result<Gic> {
        todo!()
    }
}

impl Gic {
    /// The layout the GIC was created with.
    pub fn config(&self) -> &GicConfig {
        &self.cfg
    }

    /// Sets the level of SPI `intid` (for an edge-triggered SPI, `true`
    /// signals an edge).
    pub fn set_spi(&self, _intid: u32, _level: bool) -> Result<()> {
        todo!()
    }

    /// Signals an MSI: `addr` is the doorbell address, normally
    /// `msi.base + 0x40` (`GICM_SET_SPI_NSR`).
    pub fn send_msi(&self, _addr: u64, _intid: u32) -> Result<()> {
        todo!()
    }

    /// Resets the GIC to its initial state.
    pub fn reset(&self) -> Result<()> {
        todo!()
    }

    /// Reads a distributor register (its MMIO offset).
    pub fn distributor_reg(&self, _reg: u16) -> Result<u64> {
        todo!()
    }

    /// Writes a distributor register (its MMIO offset).
    pub fn set_distributor_reg(&self, _reg: u16, _value: u64) -> Result<()> {
        todo!()
    }

    /// Reads an MSI frame register (its MMIO offset).
    pub fn msi_reg(&self, _reg: u16) -> Result<u64> {
        todo!()
    }

    /// Writes an MSI frame register (its MMIO offset).
    pub fn set_msi_reg(&self, _reg: u16, _value: u64) -> Result<()> {
        todo!()
    }

    /// Saves the GIC state as an opaque blob. The blob excludes the per-vCPU
    /// ICC/ICH registers, which are saved through the [`Vcpu`] accessors.
    pub fn save_state(&self) -> Result<Vec<u8>> {
        todo!()
    }

    /// Restores a state blob produced by [`save_state`](Self::save_state).
    pub fn restore_state(&self, _state: &[u8]) -> Result<()> {
        todo!()
    }
}

impl Vcpu {
    /// The base address of this vCPU's redistributor. Set `MPIDR_EL1` first.
    pub fn gic_redistributor_base(&self) -> Result<u64> {
        todo!()
    }

    /// Reads a redistributor register of this vCPU (its MMIO offset).
    pub fn gic_redistributor_reg(&self, _reg: u32) -> Result<u64> {
        todo!()
    }

    /// Writes a redistributor register of this vCPU (its MMIO offset).
    pub fn set_gic_redistributor_reg(&mut self, _reg: u32, _value: u64) -> Result<()> {
        todo!()
    }

    /// Reads a GIC CPU interface (`ICC_*`) register of this vCPU.
    pub fn gic_icc_reg(&self, _reg: u16) -> Result<u64> {
        todo!()
    }

    /// Writes a GIC CPU interface (`ICC_*`) register of this vCPU.
    pub fn set_gic_icc_reg(&mut self, _reg: u16, _value: u64) -> Result<()> {
        todo!()
    }

    /// Reads a GIC hypervisor control (`ICH_*`) register of this vCPU. Needs
    /// EL2 enabled.
    pub fn gic_ich_reg(&self, _reg: u16) -> Result<u64> {
        todo!()
    }

    /// Writes a GIC hypervisor control (`ICH_*`) register of this vCPU. Needs
    /// EL2 enabled.
    pub fn set_gic_ich_reg(&mut self, _reg: u16, _value: u64) -> Result<()> {
        todo!()
    }

    /// Reads a GIC virtual CPU interface (`ICV_*`) register of this vCPU.
    /// Needs EL2 enabled.
    pub fn gic_icv_reg(&self, _reg: u16) -> Result<u64> {
        todo!()
    }

    /// Writes a GIC virtual CPU interface (`ICV_*`) register of this vCPU.
    /// Needs EL2 enabled.
    pub fn set_gic_icv_reg(&mut self, _reg: u16, _value: u64) -> Result<()> {
        todo!()
    }
}
