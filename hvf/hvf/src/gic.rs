// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! The in-kernel GICv3 (macOS 15.0).
//!
//! Hypervisor.framework can emulate a GICv3 distributor, one redistributor
//! per vCPU and an optional GICv2m-style MSI frame in the kernel. Every
//! function used here was introduced in macOS 15.0 and is resolved at run
//! time; on older releases they fail with
//! [`Error::Unavailable`](crate::Error::Unavailable).
//!
//! Ordering rules:
//!
//! - The GIC must be created before the first vCPU. [`Vm::create_gic`]
//!   enforces this, and that at most one GIC exists.
//! - Write `MPIDR_EL1` of each vCPU, even if the value is the initial 0,
//!   before it runs or its redistributor is used: a vCPU whose `MPIDR_EL1`
//!   was never written has no redistributor. The framework hands out the
//!   redistributor frames in vCPU id order, whatever the affinity; each
//!   frame's `GICR_TYPER` reports the affinity, which is how a guest finds
//!   its frame.
//! - [`Gic::save_state`] is only meaningful while no vCPU runs, and
//!   [`Gic::restore_state`] must happen after every vCPU is created and has
//!   had its `MPIDR_EL1` written, and before the first run. The crate does
//!   not track running vCPUs, so it leaves these rules to the caller and
//!   returns whatever the framework reports.
//!
//! With an in-kernel GIC, the framework delivers interrupts to the vCPUs
//! itself; [`Vcpu::set_pending_interrupt`] then fails with `HV_UNSUPPORTED`.

use std::ptr;
use std::sync::Arc;

use hvf_bindings::{hv_gic_config_s, hv_gic_state_s};

use crate::align::check_gic_layout;
use crate::error::{Error, Result, check};
use crate::gic_types::GicConfig;
use crate::os_object::OsObject;
use crate::system::{Hvf, out_param};
use crate::vcpu::Vcpu;
use crate::vm::{Vm, VmInner};

/// A handle to the in-kernel GIC of the VM.
///
/// Created by [`Vm::create_gic`]. Cloning yields another handle to the same
/// GIC. The handle keeps the VM alive. VM-wide GIC operations work from any
/// thread, so `Gic` is `Send` and `Sync`; the per-vCPU registers are reached
/// through the [`Vcpu`] methods `gic_*` on the owning thread.
#[derive(Debug, Clone)]
pub struct Gic {
    /// Keeps the VM, and so the GIC, alive while the handle exists.
    pub(crate) _vm: Arc<VmInner>,
    pub(crate) cfg: GicConfig,
}

/// Creates a GIC configuration object with the layout of `cfg`.
fn gic_config(cfg: &GicConfig) -> Result<OsObject<hv_gic_config_s>> {
    let create = need!(hv_gic_config_create);
    // SAFETY: `hv_gic_config_create` has no preconditions and returns an
    // owned reference (or null), which `from_raw` takes over.
    let config =
        unsafe { OsObject::from_raw(create()) }.ok_or(Error::NullObject("hv_gic_config_create"))?;

    let set = need!(hv_gic_config_set_distributor_base);
    // SAFETY: `config` is a live GIC configuration.
    let ret = unsafe { set(config.as_ptr(), cfg.distributor_base) };
    check("hv_gic_config_set_distributor_base", ret)?;

    let set = need!(hv_gic_config_set_redistributor_base);
    // SAFETY: `config` is a live GIC configuration.
    let ret = unsafe { set(config.as_ptr(), cfg.redistributor_base) };
    check("hv_gic_config_set_redistributor_base", ret)?;

    if let Some(msi) = &cfg.msi {
        let set = need!(hv_gic_config_set_msi_region_base);
        // SAFETY: `config` is a live GIC configuration.
        let ret = unsafe { set(config.as_ptr(), msi.base) };
        check("hv_gic_config_set_msi_region_base", ret)?;

        let set = need!(hv_gic_config_set_msi_interrupt_range);
        // SAFETY: `config` is a live GIC configuration.
        let ret = unsafe { set(config.as_ptr(), msi.intid_base, msi.intid_count) };
        check("hv_gic_config_set_msi_interrupt_range", ret)?;
    }
    Ok(config)
}

impl Vm {
    /// Creates the in-kernel GIC with the layout `cfg` (macOS 15.0,
    /// `hv_gic_create`).
    ///
    /// The layout is checked against [`Hvf::gic_params`] first: every base
    /// must have the required alignment
    /// ([`Error::Misaligned`](crate::Error::Misaligned)), every region must
    /// fit in [`ipa_bits`](Self::ipa_bits) and the MSI interrupt range must
    /// lie in the SPI range ([`Error::OutOfRange`](crate::Error::OutOfRange)),
    /// and no two regions may overlap
    /// ([`Error::Overlap`](crate::Error::Overlap)). The regions must not
    /// overlap guest memory either, which is not checked.
    ///
    /// Fails with [`Error::GicAfterVcpu`](crate::Error::GicAfterVcpu) once a
    /// vCPU was created (even if it was destroyed since) and with
    /// [`Error::GicExists`](crate::Error::GicExists) if the GIC already
    /// exists.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use hvf::{GicConfig, Hvf, MsiConfig, VmConfig};
    /// let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    /// let gic = vm
    ///     .create_gic(&GicConfig {
    ///         distributor_base: 0x0800_0000,
    ///         redistributor_base: 0x080a_0000,
    ///         msi: Some(MsiConfig {
    ///             base: 0x0802_0000,
    ///             intid_base: 64,
    ///             intid_count: 64,
    ///         }),
    ///     })
    ///     .unwrap();
    /// // Raise SPI 32 from any thread.
    /// gic.set_spi(32, true).unwrap();
    /// ```
    pub fn create_gic(&self, cfg: &GicConfig) -> Result<Gic> {
        // Held across `hv_gic_create` so that it is ordered with
        // `hv_vcpu_create`.
        let mut topology = self.0.lock_topology();
        if topology.vcpu_created {
            return Err(Error::GicAfterVcpu);
        }
        if topology.gic {
            return Err(Error::GicExists);
        }
        check_gic_layout(cfg, &Hvf::gic_params()?, self.0.ipa_bits)?;

        let config = gic_config(cfg)?;
        let create = need!(hv_gic_create);
        // SAFETY: `config` is a live GIC configuration; the framework copies
        // the settings and does not keep the object.
        check("hv_gic_create", unsafe { create(config.as_ptr()) })?;
        topology.gic = true;
        drop(topology);

        Ok(Gic {
            _vm: self.0.clone(),
            cfg: *cfg,
        })
    }
}

impl Gic {
    /// The layout the GIC was created with.
    pub fn config(&self) -> &GicConfig {
        &self.cfg
    }

    /// Sets the level of SPI `intid` (`hv_gic_set_spi`).
    ///
    /// For a level-sensitive SPI the line stays at `level` until changed; for
    /// an edge-triggered one, `true` signals an edge. `intid` must be an SPI
    /// (see [`GicParams`](crate::GicParams)) outside the MSI range, otherwise
    /// the framework fails with `HV_BAD_ARGUMENT`. Works from any thread.
    pub fn set_spi(&self, intid: u32, level: bool) -> Result<()> {
        let f = need!(hv_gic_set_spi);
        // SAFETY: the function has no memory-safety preconditions; the GIC
        // exists while `self` does.
        check("hv_gic_set_spi", unsafe { f(intid, level) })
    }

    /// Signals the MSI `intid` as if a device wrote it to the doorbell `addr`
    /// (`hv_gic_send_msi`).
    ///
    /// `addr` is the guest physical doorbell address, normally
    /// `msi.base + 0x40` (`GICM_SET_SPI_NSR`), and `intid` must lie in the
    /// MSI interrupt range. Works from any thread.
    pub fn send_msi(&self, addr: u64, intid: u32) -> Result<()> {
        let f = need!(hv_gic_send_msi);
        // SAFETY: the function has no memory-safety preconditions; `addr` is
        // a guest address, never dereferenced by the host.
        check("hv_gic_send_msi", unsafe { f(addr, intid) })
    }

    /// Resets the GIC distributor, the redistributor registers and the
    /// internal state of the device (`hv_gic_reset`).
    ///
    /// The per-vCPU CPU interface registers are **not** reset: `ICC_*`
    /// (and, with EL2 enabled, `ICH_*`) keep their values, for example
    /// `ICC_PMR_EL1`, `ICC_BPR*_EL1` and `ICC_IGRPEN*_EL1`. To reset a guest
    /// fully, for example on reboot, also restore each vCPU's CPU interface
    /// state from its owning thread with
    /// [`Vcpu::set_gic_icc_reg`](crate::Vcpu::set_gic_icc_reg) and
    /// [`Vcpu::set_gic_ich_reg`](crate::Vcpu::set_gic_ich_reg).
    pub fn reset(&self) -> Result<()> {
        let f = need!(hv_gic_reset);
        // SAFETY: the function has no memory-safety preconditions.
        check("hv_gic_reset", unsafe { f() })
    }

    /// Reads a distributor register (`hv_gic_get_distributor_reg`). `reg` is
    /// the register's MMIO offset, for example `0x0000` for `GICD_CTLR`.
    pub fn distributor_reg(&self, reg: u16) -> Result<u64> {
        let f = need!(hv_gic_get_distributor_reg);
        // SAFETY: the pointer is valid for writing a `u64`.
        out_param("hv_gic_get_distributor_reg", |p| unsafe { f(reg, p) })
    }

    /// Writes a distributor register (`hv_gic_set_distributor_reg`). `reg` is
    /// the register's MMIO offset.
    pub fn set_distributor_reg(&self, reg: u16, value: u64) -> Result<()> {
        let f = need!(hv_gic_set_distributor_reg);
        // SAFETY: the function has no memory-safety preconditions.
        check("hv_gic_set_distributor_reg", unsafe { f(reg, value) })
    }

    /// Reads an MSI frame register (`hv_gic_get_msi_reg`). `reg` is the
    /// register's MMIO offset, for example `0x0008` for `GICM_TYPER`.
    pub fn msi_reg(&self, reg: u16) -> Result<u64> {
        let f = need!(hv_gic_get_msi_reg);
        // SAFETY: the pointer is valid for writing a `u64`.
        out_param("hv_gic_get_msi_reg", |p| unsafe { f(reg, p) })
    }

    /// Writes an MSI frame register (`hv_gic_set_msi_reg`). `reg` is the
    /// register's MMIO offset.
    pub fn set_msi_reg(&self, reg: u16, value: u64) -> Result<()> {
        let f = need!(hv_gic_set_msi_reg);
        // SAFETY: the function has no memory-safety preconditions.
        check("hv_gic_set_msi_reg", unsafe { f(reg, value) })
    }

    /// Saves the GIC state as an opaque, versioned blob
    /// (`hv_gic_state_create`, `hv_gic_state_get_size`,
    /// `hv_gic_state_get_data`).
    ///
    /// Call it only while no vCPU runs. The blob excludes the per-vCPU CPU
    /// interface registers (`ICC_*`, and `ICH_*` with EL2), which must be
    /// saved separately with [`Vcpu::gic_icc_reg`] and
    /// [`Vcpu::gic_ich_reg`].
    pub fn save_state(&self) -> Result<Vec<u8>> {
        let create = need!(hv_gic_state_create);
        let get_size = need!(hv_gic_state_get_size);
        let get_data = need!(hv_gic_state_get_data);

        // SAFETY: `hv_gic_state_create` has no preconditions and returns an
        // owned reference (or null, if there is no GIC), which `from_raw`
        // takes over.
        let state: OsObject<hv_gic_state_s> = unsafe { OsObject::from_raw(create()) }
            .ok_or(Error::NullObject("hv_gic_state_create"))?;

        // SAFETY: `state` is a live GIC state and the pointer is valid for
        // writing a `usize`.
        let size = out_param("hv_gic_state_get_size", |p| unsafe {
            get_size(state.as_ptr(), p)
        })?;
        let mut data = vec![0u8; size];
        if size > 0 {
            // SAFETY: `state` is a live GIC state, and `data` is valid for
            // writing the `size` bytes `hv_gic_state_get_size` reported.
            let ret = unsafe { get_data(state.as_ptr(), data.as_mut_ptr().cast()) };
            check("hv_gic_state_get_data", ret)?;
        }
        Ok(data)
    }

    /// Restores a state blob produced by [`save_state`](Self::save_state)
    /// (`hv_gic_set_state`).
    ///
    /// Call it after every vCPU is created and has had its `MPIDR_EL1`
    /// written, and before the first run; then restore the per-vCPU CPU
    /// interface registers. The framework skips the redistributor state of
    /// any vCPU whose `MPIDR_EL1` has not been written yet and still returns
    /// `Ok`, so restore `MPIDR_EL1` before calling this, not afterwards with
    /// the other per-vCPU registers. A blob saved on another macOS release
    /// may be rejected.
    pub fn restore_state(&self, state: &[u8]) -> Result<()> {
        let f = need!(hv_gic_set_state);
        let data = if state.is_empty() {
            ptr::null()
        } else {
            state.as_ptr().cast()
        };
        // SAFETY: `data` is null for an empty slice, or valid for reading
        // `state.len()` bytes; the framework copies the state.
        check("hv_gic_set_state", unsafe { f(data, state.len()) })
    }
}

impl Vcpu {
    /// The guest physical base address of this vCPU's redistributor
    /// (`hv_gic_get_redistributor_base`, macOS 15.0).
    ///
    /// Write `MPIDR_EL1` first, even if it stays 0: until then the vCPU has
    /// no redistributor and this fails. The frame goes by vCPU id, not by
    /// affinity: the vCPU with id `n` gets
    /// `redistributor_base + n * redistributor_size`, and that frame's
    /// `GICR_TYPER` reports the vCPU's `MPIDR_EL1` affinity.
    pub fn gic_redistributor_base(&self) -> Result<u64> {
        let f = need!(hv_gic_get_redistributor_base);
        // SAFETY: owning thread, live vCPU, pointer valid for a `hv_ipa_t`.
        out_param("hv_gic_get_redistributor_base", |p| unsafe {
            f(self.id, p)
        })
    }

    /// Reads a redistributor register of this vCPU
    /// (`hv_gic_get_redistributor_reg`, macOS 15.0). `reg` is the MMIO offset
    /// within the redistributor, for example `0x10100` for
    /// `GICR_ISENABLER0`.
    pub fn gic_redistributor_reg(&self, reg: u32) -> Result<u64> {
        let f = need!(hv_gic_get_redistributor_reg);
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_gic_get_redistributor_reg", |p| unsafe {
            f(self.id, reg, p)
        })
    }

    /// Writes a redistributor register of this vCPU
    /// (`hv_gic_set_redistributor_reg`, macOS 15.0). `reg` is the MMIO offset
    /// within the redistributor.
    pub fn set_gic_redistributor_reg(&mut self, reg: u32, value: u64) -> Result<()> {
        let f = need!(hv_gic_set_redistributor_reg);
        // SAFETY: owning thread, live vCPU.
        check("hv_gic_set_redistributor_reg", unsafe {
            f(self.id, reg, value)
        })
    }

    /// Reads a GIC CPU interface (`ICC_*`) register of this vCPU
    /// (`hv_gic_get_icc_reg`, macOS 15.0). `reg` is the system register
    /// encoding, for example `0xc230` for `ICC_PMR_EL1`.
    pub fn gic_icc_reg(&self, reg: u16) -> Result<u64> {
        let f = need!(hv_gic_get_icc_reg);
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_gic_get_icc_reg", |p| unsafe { f(self.id, reg, p) })
    }

    /// Writes a GIC CPU interface (`ICC_*`) register of this vCPU
    /// (`hv_gic_set_icc_reg`, macOS 15.0).
    pub fn set_gic_icc_reg(&mut self, reg: u16, value: u64) -> Result<()> {
        let f = need!(hv_gic_set_icc_reg);
        // SAFETY: owning thread, live vCPU.
        check("hv_gic_set_icc_reg", unsafe { f(self.id, reg, value) })
    }

    /// Reads a GIC hypervisor control (`ICH_*`) register of this vCPU
    /// (`hv_gic_get_ich_reg`, macOS 15.0). The framework rejects the call
    /// unless the VM has EL2 enabled.
    pub fn gic_ich_reg(&self, reg: u16) -> Result<u64> {
        let f = need!(hv_gic_get_ich_reg);
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_gic_get_ich_reg", |p| unsafe { f(self.id, reg, p) })
    }

    /// Writes a GIC hypervisor control (`ICH_*`) register of this vCPU
    /// (`hv_gic_set_ich_reg`, macOS 15.0). The framework rejects the call
    /// unless the VM has EL2 enabled.
    pub fn set_gic_ich_reg(&mut self, reg: u16, value: u64) -> Result<()> {
        let f = need!(hv_gic_set_ich_reg);
        // SAFETY: owning thread, live vCPU.
        check("hv_gic_set_ich_reg", unsafe { f(self.id, reg, value) })
    }

    /// Reads a GIC virtual CPU interface (`ICV_*`) register of this vCPU
    /// (`hv_gic_get_icv_reg`, macOS 15.0). The framework rejects the call
    /// unless the VM has EL2 enabled.
    pub fn gic_icv_reg(&self, reg: u16) -> Result<u64> {
        let f = need!(hv_gic_get_icv_reg);
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_gic_get_icv_reg", |p| unsafe { f(self.id, reg, p) })
    }

    /// Writes a GIC virtual CPU interface (`ICV_*`) register of this vCPU
    /// (`hv_gic_set_icv_reg`, macOS 15.0). The framework rejects the call
    /// unless the VM has EL2 enabled.
    pub fn set_gic_icv_reg(&mut self, reg: u16, value: u64) -> Result<()> {
        let f = need!(hv_gic_set_icv_reg);
        // SAFETY: owning thread, live vCPU.
        check("hv_gic_set_icv_reg", unsafe { f(self.id, reg, value) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Gic>();
    }
}
