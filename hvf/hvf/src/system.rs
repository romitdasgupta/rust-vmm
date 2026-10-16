// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Host capability probes, VM creation and vCPU configuration.

use hvf_bindings::hv_vcpu_config_s;

use crate::error::Result;
use crate::gic_types::{GicIntid, GicParams};
use crate::os_object::OsObject;
use crate::regs::{CacheType, FeatureReg};
use crate::vm::Vm;

/// Entry point of the crate: host probes and VM creation.
///
/// Every method states the macOS release that introduced the underlying
/// framework function. On older releases, methods using newer functions return
/// [`Error::Unavailable`](crate::Error::Unavailable).
#[derive(Debug, Clone, Copy)]
pub struct Hvf;

impl Hvf {
    /// Whether the host supports Hypervisor.framework (`kern.hv_support`).
    pub fn is_supported() -> bool {
        todo!()
    }

    /// The host page size in bytes (16 KiB on Apple silicon).
    pub fn host_page_size() -> usize {
        todo!()
    }

    /// The maximum number of vCPUs per VM (macOS 11.0).
    pub fn max_vcpu_count() -> Result<u32> {
        todo!()
    }

    /// The maximum supported IPA size in bits (macOS 13.0).
    pub fn max_ipa_bits() -> Result<u32> {
        todo!()
    }

    /// The default IPA size in bits (macOS 13.0).
    pub fn default_ipa_bits() -> Result<u32> {
        todo!()
    }

    /// Whether VMs can enable EL2, that is nested virtualization (macOS 15.0).
    pub fn el2_supported() -> Result<bool> {
        todo!()
    }

    /// The default stage-2 translation granule (macOS 26.0).
    pub fn default_ipa_granule() -> Result<IpaGranule> {
        todo!()
    }

    /// The maximum SME streaming vector length in bytes, or `None` if the
    /// host has no SME (macOS 15.2).
    pub fn sme_max_svl_bytes() -> Result<Option<usize>> {
        todo!()
    }

    /// Sizes, alignments and interrupt ranges of the in-kernel GIC
    /// (macOS 15.0).
    pub fn gic_params() -> Result<GicParams> {
        todo!()
    }

    /// The interrupt id the in-kernel GIC uses for `intid` (macOS 15.0).
    pub fn gic_intid(_intid: GicIntid) -> Result<u32> {
        todo!()
    }

    /// Creates the VM of this process.
    ///
    /// Hypervisor.framework supports a single VM per process: this fails with
    /// [`Error::VmExists`](crate::Error::VmExists) while another [`Vm`] (or
    /// any vCPU or GIC handle keeping it alive) exists.
    ///
    /// A default `cfg` uses `hv_vm_create(NULL)` (macOS 11.0). Any other
    /// setting needs `hv_vm_config_create` (macOS 13.0), EL2 needs macOS 15.0
    /// and the IPA granule macOS 26.0.
    pub fn create_vm(_cfg: &VmConfig) -> Result<Vm> {
        todo!()
    }
}

/// VM creation options. The default leaves every setting to the framework.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VmConfig {
    /// IPA size in bits, at most [`Hvf::max_ipa_bits`]. `None` keeps the
    /// default ([`Hvf::default_ipa_bits`]).
    pub ipa_bits: Option<u32>,
    /// Enables EL2 (nested virtualization) in the guest.
    pub el2: bool,
    /// Stage-2 translation granule. `None` keeps the default (16 KiB).
    pub ipa_granule: Option<IpaGranule>,
}

/// Stage-2 translation granule of a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpaGranule {
    /// 4 KiB pages (macOS 26.0).
    K4,
    /// 16 KiB pages, the default.
    K16,
}

impl IpaGranule {
    /// The granule size in bytes.
    pub fn bytes(self) -> u64 {
        todo!()
    }
}

/// A vCPU configuration, used to read the feature registers a vCPU will
/// report and to create vCPUs with [`Vm::create_vcpu`].
#[derive(Debug)]
pub struct VcpuConfig(#[allow(dead_code)] pub(crate) OsObject<hv_vcpu_config_s>);

impl VcpuConfig {
    /// Creates a default vCPU configuration (macOS 11.0).
    pub fn new() -> Result<Self> {
        todo!()
    }

    /// The value of a feature register for vCPUs created with this
    /// configuration (macOS 11.0; some registers need newer releases).
    pub fn feature_reg(&self, _reg: FeatureReg) -> Result<u64> {
        todo!()
    }

    /// The eight `CCSIDR_EL1` values for cache type `cache` (macOS 11.0).
    pub fn ccsidr_el1_values(&self, _cache: CacheType) -> Result<[u64; 8]> {
        todo!()
    }
}
