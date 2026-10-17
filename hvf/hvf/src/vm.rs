// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! The VM of the process and its guest memory.
//!
//! Hypervisor.framework has a single, implicit VM per process. Its lifetime is
//! tracked by the process-wide [`VM_STATE`] slot:
//!
//! - [`VmSlot::claim`] moves the slot from `Free` to `Live` before
//!   `hv_vm_create`, so at most one [`Vm`] exists at a time.
//! - Every `Vm`, [`Vcpu`] and [`Gic`](crate::Gic) handle holds an
//!   `Arc<VmInner>`. Dropping the last one destroys the VM and frees the
//!   slot.
//! - If `hv_vm_destroy` keeps failing (because a vCPU created through
//!   `hvf-bindings`, behind the crate's back, still exists on a live
//!   thread), the slot becomes `Zombie` and the next `create_vm` retries
//!   the destroy. A leaked [`Vcpu`] cannot cause this: it keeps its
//!   `Arc<VmInner>`, so the slot stays `Live` and `create_vm` fails with
//!   `VmExists` for the rest of the process.

use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use bitflags::bitflags;
use hvf_bindings::{
    HV_BUSY, HV_NO_DEVICE, HV_SUCCESS, hv_vcpu_create, hv_vcpu_exit_t, hv_vcpu_t, hv_vm_destroy,
    hv_vm_map, hv_vm_protect, hv_vm_unmap,
};

use crate::align::check_map_args;
use crate::error::{Error, Result, check};
use crate::system::VcpuConfig;
use crate::vcpu::Vcpu;

/// State of the process-wide VM slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotState {
    /// No VM exists.
    Free,
    /// A VM exists, or a claim is about to create one.
    Live,
    /// The last VM was dropped but `hv_vm_destroy` failed; the next
    /// `create_vm` retries the destroy.
    Zombie,
}

/// The process-wide VM slot.
static VM_STATE: Mutex<SlotState> = Mutex::new(SlotState::Free);

/// Locks the VM slot, ignoring poisoning (the state is a plain enum that is
/// always consistent).
fn slot_state() -> MutexGuard<'static, SlotState> {
    VM_STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How often `hv_vm_destroy` is attempted while it returns `HV_BUSY`.
const DESTROY_ATTEMPTS: u32 = 50;
/// Delay between two `hv_vm_destroy` attempts.
const DESTROY_RETRY_DELAY: Duration = Duration::from_millis(1);

/// Destroys the VM of the process, retrying while the framework reports
/// `HV_BUSY` (vCPUs of exited threads are torn down asynchronously).
///
/// `HV_NO_DEVICE`, which the framework returns when no VM exists, counts as
/// success.
///
/// Must only be called by the owner of the VM slot, while no `Vm`, `Vcpu` or
/// `Gic` handle exists.
fn destroy_vm() -> Result<()> {
    let mut attempt = 1;
    loop {
        // SAFETY: the function has no memory-safety preconditions. The caller
        // owns the VM slot and no handle to the VM exists, so nothing in this
        // crate uses the VM or its mappings any more.
        let ret = unsafe { hv_vm_destroy() };
        match ret {
            HV_SUCCESS | HV_NO_DEVICE => return Ok(()),
            HV_BUSY if attempt < DESTROY_ATTEMPTS => {
                attempt += 1;
                thread::sleep(DESTROY_RETRY_DELAY);
            }
            _ => return check("hv_vm_destroy", ret),
        }
    }
}

/// Exclusive ownership of the process-wide VM slot, from before
/// `hv_vm_create` until the VM is handed to a [`VmInner`].
///
/// Dropping an unused claim frees the slot again.
#[derive(Debug)]
pub(crate) struct VmSlot {
    _private: (),
}

impl VmSlot {
    /// Claims the VM slot.
    ///
    /// Fails with [`Error::VmExists`] while a VM is alive. If the previous VM
    /// could not be destroyed, the destroy is retried first; if it still
    /// fails, its error is returned and the slot stays a zombie.
    pub(crate) fn claim() -> Result<VmSlot> {
        let mut state = slot_state();
        match *state {
            SlotState::Free => {}
            SlotState::Live => return Err(Error::VmExists),
            // No handle to the old VM exists. Holding the lock across the
            // retries makes a concurrent `create_vm` wait for the outcome.
            SlotState::Zombie => destroy_vm()?,
        }
        *state = SlotState::Live;
        Ok(VmSlot { _private: () })
    }

    /// Hands the slot over to a newly created VM with the given effective
    /// configuration. The VM is destroyed when the last handle is dropped.
    pub(crate) fn into_vm(self, granule: u64, ipa_bits: u32, el2: bool) -> Vm {
        // The `VmInner` takes over the claim and releases it in its `Drop`.
        mem::forget(self);
        Vm(Arc::new(VmInner {
            granule,
            ipa_bits,
            el2,
            topology: Mutex::new(Topology::default()),
        }))
    }

    /// Destroys a VM created under this claim that will not be handed out,
    /// then frees the slot (or leaves it a zombie if the destroy fails).
    pub(crate) fn destroy_created(self) {
        if destroy_vm().is_err() {
            *slot_state() = SlotState::Zombie;
            mem::forget(self);
        }
    }
}

impl Drop for VmSlot {
    fn drop(&mut self) {
        *slot_state() = SlotState::Free;
    }
}

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
///
/// Leaking a [`Vcpu`] (for example with [`std::mem::forget`]) keeps the VM
/// alive for the rest of the process.
#[derive(Debug, Clone)]
pub struct Vm(pub(crate) Arc<VmInner>);

/// Shared state of the VM.
///
/// Exactly one `VmInner` exists while the VM slot is `Live` with a VM
/// handed out; it owns the slot and destroys the VM on drop.
#[derive(Debug)]
pub(crate) struct VmInner {
    /// Stage-2 granule in bytes.
    pub(crate) granule: u64,
    /// IPA size in bits.
    pub(crate) ipa_bits: u32,
    /// Whether EL2 is enabled.
    pub(crate) el2: bool,
    /// Held across `hv_vcpu_create` and `hv_gic_create` to order them.
    pub(crate) topology: Mutex<Topology>,
}

impl VmInner {
    /// Locks the topology, ignoring poisoning (the state is two flags that
    /// are always consistent).
    pub(crate) fn lock_topology(&self) -> std::sync::MutexGuard<'_, Topology> {
        self.topology.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for VmInner {
    fn drop(&mut self) {
        // Every `Vm`, `Vcpu` and `Gic` handle is gone, so every vCPU created
        // through this crate was destroyed by its handle (a leaked `Vcpu`
        // keeps this `VmInner` alive). Only vCPUs created through
        // `hvf-bindings` directly can still make the destroy fail.
        *slot_state() = if destroy_vm().is_ok() {
            SlotState::Free
        } else {
            SlotState::Zombie
        };
    }
}

/// What has been created in the VM so far.
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

    /// Maps `size` bytes of host memory at `host` into the guest at `ipa`
    /// (`hv_vm_map`).
    ///
    /// `host`, `ipa` and `size` must be multiples of
    /// [`ipa_granule`](Self::ipa_granule) (otherwise
    /// [`Error::Misaligned`](crate::Error::Misaligned)), `size` must not be
    /// zero, and the range must fit in [`ipa_bits`](Self::ipa_bits) (otherwise
    /// [`Error::OutOfRange`](crate::Error::OutOfRange)). With the default
    /// 16 KiB granule, the host address must also be aligned to the host page
    /// size, which it is for any `mmap` result.
    ///
    /// An existing mapping in the range is not replaced: unmap before
    /// remapping. Mapping over a mapped range fails with `HV_ERROR` from
    /// `hv_vm_map`.
    ///
    /// # Safety
    ///
    /// `[host, host + size)` is a valid host mapping that stays mapped (no
    /// `munmap`, `mremap` or `MAP_FIXED` replacement) until `unmap` of this
    /// range returns `Ok` or every `Vm`/`Vcpu`/`Gic` handle is dropped. The
    /// guest may read and write it at any time: host code must access it only
    /// through volatile or atomic operations (for example vm-memory's
    /// `VolatileSlice`), never through `&` or `&mut` references.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use hvf::{Hvf, MemoryFlags, VmConfig};
    /// let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    /// let size = vm.ipa_granule() as usize;
    /// // SAFETY: a fresh anonymous mapping.
    /// let host = unsafe {
    ///     libc::mmap(
    ///         std::ptr::null_mut(),
    ///         size,
    ///         libc::PROT_READ | libc::PROT_WRITE,
    ///         libc::MAP_ANON | libc::MAP_PRIVATE,
    ///         -1,
    ///         0,
    ///     )
    /// };
    /// assert_ne!(host, libc::MAP_FAILED);
    /// // SAFETY: the mapping outlives the VM, which is dropped first.
    /// unsafe {
    ///     vm.map(host.cast(), 0x4000_0000, size, MemoryFlags::RWX)
    ///         .unwrap()
    /// };
    /// vm.unmap(0x4000_0000, size).unwrap();
    /// ```
    pub unsafe fn map(
        &self,
        host: *mut u8,
        ipa: u64,
        size: usize,
        flags: MemoryFlags,
    ) -> Result<()> {
        check_map_args(
            Some(host as usize),
            ipa,
            size,
            self.0.granule,
            self.0.ipa_bits,
        )?;
        // SAFETY: the caller guarantees that `[host, host + size)` stays a
        // valid mapping for as long as the guest can access it, and that the
        // host never holds Rust references into it. The framework only stores
        // the address for stage-2 translation.
        let ret = unsafe { hv_vm_map(host.cast(), ipa, size, flags.bits()) };
        check("hv_vm_map", ret)
    }

    /// Removes the guest mapping of `[ipa, ipa + size)` (`hv_vm_unmap`).
    ///
    /// The arguments are checked as for [`map`](Self::map). When this returns
    /// `Ok`, no vCPU can access the old host memory any more, even one that
    /// is running at the time of the call, so the host memory may be freed.
    pub fn unmap(&self, ipa: u64, size: usize) -> Result<()> {
        check_map_args(None, ipa, size, self.0.granule, self.0.ipa_bits)?;
        // SAFETY: unmapping only removes guest access to host memory; it
        // cannot make any host memory invalid.
        let ret = unsafe { hv_vm_unmap(ipa, size) };
        check("hv_vm_unmap", ret)
    }

    /// Changes the guest permissions of the mapped range `[ipa, ipa + size)`
    /// (`hv_vm_protect`).
    ///
    /// The arguments are checked as for [`map`](Self::map). Fails with
    /// `HV_ERROR` if the range is not mapped.
    ///
    /// A guest access the new permissions forbid exits with an abort. For a
    /// page the guest has already accessed this is a permission fault, but
    /// for a page it has not touched yet the framework reports a translation
    /// fault, with the same syndrome as an access to unmapped (MMIO) memory.
    /// A dirty page tracker must therefore treat every write abort on a
    /// range it mapped as a dirtying write, whatever the fault status code,
    /// and retry the store (without advancing the PC) once the page is
    /// writable again.
    pub fn protect(&self, ipa: u64, size: usize, flags: MemoryFlags) -> Result<()> {
        check_map_args(None, ipa, size, self.0.granule, self.0.ipa_bits)?;
        // SAFETY: changing guest permissions of an existing mapping does not
        // affect host memory safety; the host memory stays mapped.
        let ret = unsafe { hv_vm_protect(ipa, size, flags.bits()) };
        check("hv_vm_protect", ret)
    }

    /// Creates a vCPU bound to the calling thread (`hv_vcpu_create`).
    ///
    /// `cfg` selects the vCPU configuration; `None` uses the default one.
    ///
    /// A thread can own a single vCPU at a time; a second one fails with
    /// [`Error::Hv`](crate::Error::Hv) for `hv_vcpu_create` with `HV_BUSY`.
    /// Once a vCPU was created, the in-kernel GIC can no longer be created.
    pub fn create_vcpu(&self, cfg: Option<&VcpuConfig>) -> Result<Vcpu> {
        // Held across `hv_vcpu_create` so that it is ordered with
        // `hv_gic_create`.
        let mut topology = self.0.lock_topology();
        let config = cfg.map_or(ptr::null_mut(), |cfg| cfg.0.as_ptr());
        let mut id: hv_vcpu_t = 0;
        let mut exit: *mut hv_vcpu_exit_t = ptr::null_mut();
        // SAFETY: `id` and `exit` are valid for writes, and `config` is null
        // or a live vCPU configuration borrowed from `cfg` for the duration
        // of the call.
        let ret = unsafe { hv_vcpu_create(&mut id, &mut exit, config) };
        check("hv_vcpu_create", ret)?;
        topology.vcpu_created = true;
        drop(topology);
        // SAFETY: `hv_vcpu_create` just created vCPU `id` on this thread and
        // returned its exit information pointer.
        unsafe { Vcpu::from_created(id, exit, self.0.clone()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::Hvf;
    use crate::system::VmConfig;
    use crate::system::tests::hv_test_lock;

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Vm>();
        assert_send_sync::<crate::Gic>();
    }

    #[test]
    fn test_memory_flags() {
        assert_eq!(MemoryFlags::READ.bits(), 1);
        assert_eq!(MemoryFlags::WRITE.bits(), 2);
        assert_eq!(MemoryFlags::EXEC.bits(), 4);
        assert_eq!(MemoryFlags::RW.bits(), 3);
        assert_eq!(MemoryFlags::RWX.bits(), 7);
    }

    /// An anonymous mapping freed on drop.
    struct AnonMem {
        ptr: *mut u8,
        size: usize,
    }

    impl AnonMem {
        fn new(size: usize) -> Self {
            // SAFETY: an anonymous private mapping has no preconditions; the
            // result is checked below.
            let ptr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_ANON | libc::MAP_PRIVATE,
                    -1,
                    0,
                )
            };
            assert_ne!(ptr, libc::MAP_FAILED);
            AnonMem {
                ptr: ptr.cast(),
                size,
            }
        }
    }

    impl Drop for AnonMem {
        fn drop(&mut self) {
            // SAFETY: `ptr` is a mapping of `size` bytes created in `new`;
            // the tests drop it only after the VM using it is gone.
            unsafe { libc::munmap(self.ptr.cast(), self.size) };
        }
    }

    #[test]
    fn test_map_unmap_protect() {
        let Some(_g) = hv_test_lock() else { return };
        let mem = AnonMem::new(4 * Hvf::host_page_size());
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        let granule = vm.ipa_granule();
        let size = granule as usize;
        let ipa = 0x4000_0000;

        // Misaligned host address, IPA and size are caught before the call.
        // SAFETY: the checks fail before anything is mapped.
        let e = unsafe { vm.map(mem.ptr.wrapping_add(1), ipa, size, MemoryFlags::RW) };
        assert!(matches!(
            e,
            Err(Error::Misaligned {
                what: "host address",
                ..
            })
        ));
        // SAFETY: as above.
        let e = unsafe { vm.map(mem.ptr, ipa + 1, size, MemoryFlags::RW) };
        assert!(matches!(e, Err(Error::Misaligned { what: "ipa", .. })));
        // SAFETY: as above.
        let e = unsafe { vm.map(mem.ptr, ipa, size + 1, MemoryFlags::RW) };
        assert!(matches!(e, Err(Error::Misaligned { what: "size", .. })));
        // SAFETY: as above.
        let e = unsafe { vm.map(mem.ptr, 1 << vm.ipa_bits(), size, MemoryFlags::RW) };
        assert!(matches!(e, Err(Error::OutOfRange { .. })));

        // SAFETY: `mem` outlives `vm`, and the host does not access it.
        unsafe { vm.map(mem.ptr, ipa, 2 * size, MemoryFlags::RW) }.unwrap();
        // Mapping over a mapping fails in the framework.
        // SAFETY: as above.
        let e = unsafe { vm.map(mem.ptr, ipa, size, MemoryFlags::RW) }.unwrap_err();
        assert!(
            matches!(
                e,
                Error::Hv {
                    call: "hv_vm_map",
                    ..
                }
            ),
            "{e:?}"
        );

        vm.protect(ipa, size, MemoryFlags::READ).unwrap();
        vm.unmap(ipa, 2 * size).unwrap();
        let e = vm.protect(ipa, size, MemoryFlags::READ).unwrap_err();
        assert!(
            matches!(
                e,
                Error::Hv {
                    call: "hv_vm_protect",
                    ..
                }
            ),
            "{e:?}"
        );
        assert!(matches!(
            vm.unmap(ipa, 0),
            Err(Error::OutOfRange { value: 0, .. })
        ));

        // Remapping after the unmap works.
        // SAFETY: as above.
        unsafe { vm.map(mem.ptr, ipa, size, MemoryFlags::RWX) }.unwrap();
        drop(vm);
    }

    #[test]
    fn test_slot_states() {
        let Some(_g) = hv_test_lock() else { return };
        assert_eq!(*slot_state(), SlotState::Free);

        // An unused claim frees the slot again.
        let slot = VmSlot::claim().unwrap();
        assert_eq!(*slot_state(), SlotState::Live);
        assert_eq!(VmSlot::claim().unwrap_err(), Error::VmExists);
        drop(slot);
        assert_eq!(*slot_state(), SlotState::Free);

        // A zombie slot without a VM (the framework already destroyed it) is
        // recovered by the next claim.
        *slot_state() = SlotState::Zombie;
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        assert_eq!(*slot_state(), SlotState::Live);
        assert_eq!(
            Hvf::create_vm(&VmConfig::default()).unwrap_err(),
            Error::VmExists
        );

        // Every handle keeps the VM alive.
        let clone = vm.clone();
        drop(vm);
        assert_eq!(*slot_state(), SlotState::Live);
        drop(clone);
        assert_eq!(*slot_state(), SlotState::Free);
    }

    #[test]
    fn test_zombie_recovery() {
        let Some(_g) = hv_test_lock() else { return };
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();

        // A vCPU created behind the crate's back keeps `hv_vm_destroy`
        // failing with `HV_BUSY` while its thread lives. `hv_vm_destroy`
        // succeeds once that thread has exited.
        let (created_tx, created_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let handle = thread::spawn(move || {
            let mut id = 0;
            let mut exit = ptr::null_mut();
            // SAFETY: `id` and `exit` are valid for writes; a null
            // configuration selects the default one.
            let ret = unsafe { hv_vcpu_create(&mut id, &mut exit, ptr::null_mut()) };
            created_tx.send(ret).unwrap();
            release_rx.recv().unwrap();
        });
        assert_eq!(created_rx.recv().unwrap(), HV_SUCCESS);

        // The destroy of the last handle fails and leaves a zombie slot.
        drop(vm);
        assert_eq!(*slot_state(), SlotState::Zombie);
        // The next creation retries the destroy, which still fails.
        let e = Hvf::create_vm(&VmConfig::default()).unwrap_err();
        assert!(
            matches!(e, Error::Hv { call: "hv_vm_destroy", err } if err.is_busy()),
            "{e:?}"
        );
        assert_eq!(*slot_state(), SlotState::Zombie);

        // Once the thread is gone, creation recovers the slot.
        release_tx.send(()).unwrap();
        handle.join().unwrap();
        let start = std::time::Instant::now();
        let vm = loop {
            match Hvf::create_vm(&VmConfig::default()) {
                Ok(vm) => break vm,
                Err(Error::Hv { call, err }) if call == "hv_vm_destroy" && err.is_busy() => {
                    assert!(start.elapsed() < Duration::from_secs(1));
                }
                Err(e) => panic!("unexpected error: {e:?}"),
            }
        };
        assert_eq!(*slot_state(), SlotState::Live);
        drop(vm);
        assert_eq!(*slot_state(), SlotState::Free);
    }

    #[test]
    fn test_vm_kept_alive_by_vcpu() {
        let Some(_g) = hv_test_lock() else { return };
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let handle = {
            let vm = vm.clone();
            thread::spawn(move || {
                let vcpu = vm.create_vcpu(None).unwrap();
                drop(vm);
                tx.send(()).unwrap();
                done_rx.recv().unwrap();
                drop(vcpu);
            })
        };
        rx.recv().unwrap();
        drop(vm);
        // The vCPU on the other thread still holds the VM.
        assert_eq!(*slot_state(), SlotState::Live);
        done_tx.send(()).unwrap();
        handle.join().unwrap();
        assert_eq!(*slot_state(), SlotState::Free);
    }
}
