// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! `Vm::map_guest_memory` (feature `vm-memory`): mapping, validation and
//! rollback.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.

#![cfg(all(target_os = "macos", target_arch = "aarch64", feature = "vm-memory"))]

mod common;

use common::{boot_vcpu, expect_exception, hv_guard};
use hvf::{Error, ExceptionClass, Hvf, MemoryFlags, Vm, VmConfig};
use vm_memory::{
    Bytes, GuestAddress, GuestMemoryBackend, GuestMemoryMmap, GuestMemoryRegion,
    GuestMemoryRegionBytes, GuestRegionMmap, GuestUsize, MmapRegion,
};

/// Guest address of the code region.
const CODE_IPA: u64 = 0x4000_0000;
/// Guest address of the data region.
const DATA_IPA: u64 = 0x8000_0000;

/// `movz x1, #0x8000, lsl #16; movz w0, #0x42; str w0, [x1]; hvc #0`: stores
/// 0x42 at `DATA_IPA` and exits.
const GUEST: [u32; 4] = [0xd2b0_0001, 0x5280_0840, 0xb900_0020, 0xd400_0002];

type Mem = GuestMemoryMmap<()>;

fn create_vm() -> Vm {
    Hvf::create_vm(&VmConfig::default()).unwrap()
}

/// Size of each region of [`guest_memory`]: a multiple of every IPA granule.
const REGION_SIZE: usize = 0x1_0000;

/// [`REGION_SIZE`] bytes of anonymous memory at each of `ipas`.
///
/// Create it before the VM, so that it is dropped after it: memory mapped
/// into a guest must stay alive until it is unmapped or the VM is gone.
fn guest_memory(ipas: &[u64]) -> Mem {
    let ranges: Vec<_> = ipas
        .iter()
        .map(|&ipa| (GuestAddress(ipa), REGION_SIZE))
        .collect();
    Mem::from_ranges(&ranges).unwrap()
}

/// Whether `[ipa, ipa + size)` is free in the guest: mapping fresh host
/// memory there succeeds (the mapping is removed again).
fn ipa_range_is_free(vm: &Vm, ipa: u64, size: usize) -> bool {
    let probe = MmapRegion::<()>::new(size).unwrap();
    // SAFETY: `probe` stays mapped until after the range is unmapped again
    // below, and the guest does not run meanwhile.
    let mapped = unsafe { vm.map(probe.as_ptr(), ipa, size, MemoryFlags::RW) };
    match mapped {
        Ok(()) => {
            vm.unmap(ipa, size).unwrap();
            true
        }
        Err(Error::Hv {
            call: "hv_vm_map", ..
        }) => false,
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

/// A `GuestMemoryBackend` that does not check its regions, for regions a
/// `GuestMemoryMmap` would reject.
struct RawRegions<R>(Vec<R>);

impl<R: GuestMemoryRegion> GuestMemoryBackend for RawRegions<R> {
    type R = R;

    fn iter(&self) -> impl Iterator<Item = &R> {
        self.0.iter()
    }
}

/// A region with no host mapping.
#[derive(Debug)]
struct NoHostRegion {
    start: GuestAddress,
    len: GuestUsize,
}

impl GuestMemoryRegion for NoHostRegion {
    type B = ();

    fn len(&self) -> GuestUsize {
        self.len
    }

    fn start_addr(&self) -> GuestAddress {
        self.start
    }

    fn bitmap(&self) {}
}

impl GuestMemoryRegionBytes for NoHostRegion {}

#[test]
fn test_map_guest_memory_runs_guest() {
    let Some(_g) = hv_guard() else { return };
    let mem = guest_memory(&[CODE_IPA, DATA_IPA]);
    let vm = create_vm();
    for (i, insn) in GUEST.iter().enumerate() {
        mem.write_obj(*insn, GuestAddress(CODE_IPA + 4 * i as u64))
            .unwrap();
    }
    // SAFETY: `mem` was created before the VM, so it outlives it, and it is
    // only accessed through `Bytes`.
    unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX).unwrap() };

    let mut vcpu = boot_vcpu(&vm, CODE_IPA);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(mem.read_obj::<u32>(GuestAddress(DATA_IPA)).unwrap(), 0x42);
    drop(vcpu);

    // Every region is mapped separately and can be unmapped on its own.
    for ipa in [CODE_IPA, DATA_IPA] {
        assert!(!ipa_range_is_free(&vm, ipa, REGION_SIZE));
        vm.unmap(ipa, REGION_SIZE).unwrap();
        assert!(ipa_range_is_free(&vm, ipa, REGION_SIZE));
    }
}

#[test]
fn test_map_guest_memory_empty() {
    let Some(_g) = hv_guard() else { return };
    let vm = create_vm();
    let mem = RawRegions::<GuestRegionMmap<()>>(Vec::new());
    // SAFETY: there is no memory to map.
    unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX).unwrap() };
}

#[test]
fn test_map_guest_memory_rollback() {
    let Some(_g) = hv_guard() else { return };
    let size = REGION_SIZE;
    // Both are created before the VM, so they outlive it even if the test
    // fails while they are mapped.
    let mem = guest_memory(&[CODE_IPA, DATA_IPA]);
    let blocker = MmapRegion::<()>::new(size).unwrap();
    let vm = create_vm();

    // Occupy the guest range of the second region.
    // SAFETY: `blocker` outlives the VM, and the guest never runs.
    unsafe {
        vm.map(blocker.as_ptr(), DATA_IPA, size, MemoryFlags::RW)
            .unwrap()
    };

    // SAFETY: `mem` outlives the VM, and the guest never runs.
    let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX) }.unwrap_err();
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
    // The first region, mapped before the failure, was unmapped again.
    assert!(ipa_range_is_free(&vm, CODE_IPA, size));
    assert!(!ipa_range_is_free(&vm, DATA_IPA, size));

    // Once the range is free, the same memory maps.
    vm.unmap(DATA_IPA, size).unwrap();
    // SAFETY: as above.
    unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX).unwrap() };
    assert!(!ipa_range_is_free(&vm, CODE_IPA, size));
    assert!(!ipa_range_is_free(&vm, DATA_IPA, size));

    // Mapping it again fails on the first region and leaves the existing
    // mappings alone.
    // SAFETY: as above.
    let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX) }.unwrap_err();
    assert!(matches!(e, Error::Hv { .. }), "{e:?}");
    assert!(!ipa_range_is_free(&vm, CODE_IPA, size));
    assert!(!ipa_range_is_free(&vm, DATA_IPA, size));
}

#[test]
fn test_map_guest_memory_validates_first() {
    let Some(_g) = hv_guard() else { return };
    let vm = create_vm();
    let granule = vm.ipa_granule();
    let size = granule as usize;
    let limit = 1u64 << vm.ipa_bits();

    // A region whose size is not a multiple of the granule. The valid region
    // sorts first and must not have been mapped.
    let mem = Mem::from_ranges(&[
        (GuestAddress(CODE_IPA), size),
        (GuestAddress(DATA_IPA), size + 0x1000),
    ])
    .unwrap();
    // SAFETY: nothing is mapped.
    let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX) }.unwrap_err();
    assert_eq!(
        e,
        Error::Misaligned {
            what: "size",
            value: size as u64 + 0x1000,
            align: granule,
        }
    );
    assert!(ipa_range_is_free(&vm, CODE_IPA, size));

    // A misaligned guest address.
    let mem = Mem::from_ranges(&[
        (GuestAddress(CODE_IPA), size),
        (GuestAddress(DATA_IPA + 0x1000), size),
    ])
    .unwrap();
    // SAFETY: nothing is mapped.
    let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX) }.unwrap_err();
    assert!(matches!(e, Error::Misaligned { what: "ipa", .. }), "{e:?}");
    assert!(ipa_range_is_free(&vm, CODE_IPA, size));

    // A region beyond the IPA space.
    let mem =
        Mem::from_ranges(&[(GuestAddress(CODE_IPA), size), (GuestAddress(limit), size)]).unwrap();
    // SAFETY: nothing is mapped.
    let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX) }.unwrap_err();
    assert_eq!(
        e,
        Error::OutOfRange {
            what: "ipa + size",
            value: limit + size as u64,
            limit,
        }
    );
    assert!(ipa_range_is_free(&vm, CODE_IPA, size));

    // Overlapping regions, which `GuestMemoryMmap` itself would reject.
    let region = |ipa: u64, len: usize| {
        GuestRegionMmap::new(MmapRegion::<()>::new(len).unwrap(), GuestAddress(ipa)).unwrap()
    };
    let mem = RawRegions(vec![
        region(DATA_IPA, size),
        region(CODE_IPA, 2 * size),
        region(CODE_IPA + granule, size),
    ]);
    // SAFETY: nothing is mapped.
    let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX) }.unwrap_err();
    assert!(matches!(e, Error::Overlap { .. }), "{e:?}");
    assert!(ipa_range_is_free(&vm, CODE_IPA, 2 * size));
    assert!(ipa_range_is_free(&vm, DATA_IPA, size));

    // A region without host memory.
    let mem = RawRegions(vec![NoHostRegion {
        start: GuestAddress(CODE_IPA),
        len: size as u64,
    }]);
    // SAFETY: nothing is mapped.
    let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX) }.unwrap_err();
    assert!(matches!(e, Error::NullObject(_)), "{e:?}");
    assert!(ipa_range_is_free(&vm, CODE_IPA, size));
}
