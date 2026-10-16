// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! The VM of the process and its guest memory.

use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};

use bitflags::bitflags;

use crate::error::Result;
use crate::system::VcpuConfig;
use crate::vcpu::Vcpu;

/// No VM exists.
#[allow(dead_code)]
pub(crate) const VM_FREE: u8 = 0;
/// A VM exists.
#[allow(dead_code)]
pub(crate) const VM_LIVE: u8 = 1;
/// The last VM was dropped but `hv_vm_destroy` failed; the next `create_vm`
/// retries the destroy.
#[allow(dead_code)]
pub(crate) const VM_ZOMBIE: u8 = 2;

/// State of the process-wide VM slot: one of `VM_FREE`, `VM_LIVE` or
/// `VM_ZOMBIE`.
#[allow(dead_code)]
pub(crate) static VM_STATE: AtomicU8 = AtomicU8::new(VM_FREE);

bitflags! {
    /// Guest access permissions of a memory mapping (`hv_memory_flags_t`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct MemoryFlags: u64 {
        /// The guest can read.
        const READ = hvf_bindings::HV_MEMORY_READ;
        /// The guest can write.
        const WRITE = hvf_bindings::HV_MEMORY_WRITE;
        /// The guest can execute.
        const EXEC = hvf_bindings::HV_MEMORY_EXEC;
        /// The guest can read and write.
        const RW = Self::READ.bits() | Self::WRITE.bits();
        /// The guest can read, write and execute.
        const RWX = Self::RW.bits() | Self::EXEC.bits();
    }
}

/// The VM of this process.
///
/// Created by [`Hvf::create_vm`](crate::Hvf::create_vm). Cloning yields
/// another handle to the same VM; the VM is destroyed once every `Vm`,
/// [`Vcpu`] and [`Gic`](crate::Gic) handle is dropped. VM-level operations
/// work from any thread, so `Vm` is `Send` and `Sync`.
#[derive(Debug, Clone)]
pub struct Vm(pub(crate) Arc<VmInner>);

/// Shared state of the VM.
#[derive(Debug)]
pub(crate) struct VmInner {
    /// Stage-2 granule in bytes.
    pub(crate) granule: u64,
    /// IPA size in bits.
    pub(crate) ipa_bits: u32,
    /// Whether EL2 is enabled.
    pub(crate) el2: bool,
    /// Held across `hv_vcpu_create` and `hv_gic_create` to order them.
    #[allow(dead_code)]
    pub(crate) topology: Mutex<Topology>,
}

/// What has been created in the VM so far.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub(crate) struct Topology {
    /// At least one vCPU was created.
    pub(crate) vcpu_created: bool,
    /// The in-kernel GIC was created.
    pub(crate) gic: bool,
}

impl Vm {
    /// The stage-2 translation granule in bytes. Every address and size
    /// passed to [`map`](Self::map), [`unmap`](Self::unmap) and
    /// [`protect`](Self::protect) must be a multiple of it.
    pub fn ipa_granule(&self) -> u64 {
        self.0.granule
    }

    /// The IPA size in bits.
    pub fn ipa_bits(&self) -> u32 {
        self.0.ipa_bits
    }

    /// Whether EL2 (nested virtualization) is enabled.
    pub fn el2_enabled(&self) -> bool {
        self.0.el2
    }

    /// Maps `size` bytes of host memory at `host` into the guest at `ipa`.
    ///
    /// An existing mapping in the range is not replaced: unmap before
    /// remapping.
    ///
    /// # Safety
    ///
    /// `[host, host + size)` is a valid host mapping that stays mapped (no
    /// `munmap`, `mremap` or `MAP_FIXED` replacement) until `unmap` of this
    /// range returns `Ok` or every `Vm`/`Vcpu`/`Gic` handle is dropped. The
    /// guest may read and write it at any time: host code must access it only
    /// through volatile or atomic operations (for example vm-memory's
    /// `VolatileSlice`), never through `&` or `&mut` references.
    pub unsafe fn map(
        &self,
        _host: *mut u8,
        _ipa: u64,
        _size: usize,
        _flags: MemoryFlags,
    ) -> Result<()> {
        todo!()
    }

    /// Removes the guest mapping of `[ipa, ipa + size)`. When this returns,
    /// no vCPU can access the old host memory any more.
    pub fn unmap(&self, _ipa: u64, _size: usize) -> Result<()> {
        todo!()
    }

    /// Changes the guest permissions of the mapped range `[ipa, ipa + size)`.
    pub fn protect(&self, _ipa: u64, _size: usize, _flags: MemoryFlags) -> Result<()> {
        todo!()
    }

    /// Creates a vCPU bound to the calling thread.
    ///
    /// A thread can own a single vCPU at a time; a second one fails with
    /// `HV_BUSY`.
    pub fn create_vcpu(&self, _cfg: Option<&VcpuConfig>) -> Result<Vcpu> {
        todo!()
    }
}
