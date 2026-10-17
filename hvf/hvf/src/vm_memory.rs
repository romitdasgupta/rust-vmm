// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Integration with `vm-memory` (feature `vm-memory`).

use vm_memory::{Address, GuestMemoryBackend, GuestMemoryRegion, MemoryRegionAddress};

use crate::align::check_map_args;
use crate::error::{Error, Result};
use crate::vm::{MemoryFlags, Vm};

/// A guest memory region, validated and ready to be mapped.
#[derive(Debug, Clone, Copy)]
struct Mapping {
    host: *mut u8,
    ipa: u64,
    size: usize,
}

impl Vm {
    /// Maps every region of `mem` into the guest at its guest address, with
    /// the guest permissions `flags`.
    ///
    /// All regions are validated before anything is mapped: each one must
    /// have a host address, its host address, guest address and size must
    /// satisfy the requirements of [`Vm::map`], and no two regions may
    /// overlap in the guest physical address space. The first violation is
    /// returned:
    ///
    /// - [`Error::NoHostAddress`] for a region without a host address (one whose
    ///   [`GuestMemoryRegion::get_host_address`] fails or returns null);
    /// - [`Error::Misaligned`] or [`Error::OutOfRange`] as for [`Vm::map`];
    /// - [`Error::Overlap`] for overlapping regions.
    ///
    /// The regions are then mapped in ascending guest address order. If
    /// mapping one fails (for example with `HV_ERROR` from `hv_vm_map`
    /// because part of its range is already mapped), the regions mapped by
    /// this call are unmapped again and the error is returned, so the call
    /// either maps every region or none.
    ///
    /// Regions are not merged or split: each one is a separate `hv_vm_map`,
    /// and can later be removed with [`Vm::unmap`] of its range.
    ///
    /// # Safety
    ///
    /// The same as [`Vm::map`] for the host memory of every region: it must
    /// stay mapped (in particular, `mem` or whatever owns its mappings must
    /// not be dropped) until its guest range is unmapped with [`Vm::unmap`]
    /// or every `Vm`/`Vcpu`/`Gic` handle is dropped. The guest may access it
    /// at any time, so host code must only access it through volatile or
    /// atomic operations, such as the [`vm_memory::Bytes`] interface of `mem`,
    /// never through `&` or `&mut` references.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use hvf::{Hvf, MemoryFlags, VmConfig};
    /// use vm_memory::{GuestAddress, GuestMemoryMmap};
    ///
    /// let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    /// let size = vm.ipa_granule() as usize;
    /// let mem = GuestMemoryMmap::<()>::from_ranges(&[
    ///     (GuestAddress(0x4000_0000), 16 * size),
    ///     (GuestAddress(0x8000_0000), 16 * size),
    /// ])
    /// .unwrap();
    /// // SAFETY: `mem` outlives the VM, which is dropped first.
    /// unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX).unwrap() };
    /// # drop(vm);
    /// ```
    pub unsafe fn map_guest_memory<M>(&self, mem: &M, flags: MemoryFlags) -> Result<()>
    where
        M: GuestMemoryBackend + ?Sized,
    {
        let mappings = self.guest_memory_mappings(mem)?;
        for (index, mapping) in mappings.iter().enumerate() {
            // SAFETY: `mapping` describes a host mapping owned by `mem`, and
            // the caller guarantees that it stays mapped and is only accessed
            // through volatile or atomic operations for as long as the guest
            // can access it.
            let mapped = unsafe { self.map(mapping.host, mapping.ipa, mapping.size, flags) };
            if let Err(e) = mapped {
                self.unmap_all(&mappings[..index]);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Validates every region of `mem` and returns them sorted by guest
    /// address.
    fn guest_memory_mappings<M>(&self, mem: &M) -> Result<Vec<Mapping>>
    where
        M: GuestMemoryBackend + ?Sized,
    {
        let mut mappings = Vec::with_capacity(mem.num_regions());
        for region in mem.iter() {
            let mapping = region_mapping(region)?;
            check_map_args(
                Some(mapping.host as usize),
                mapping.ipa,
                mapping.size,
                self.ipa_granule(),
                self.ipa_bits(),
            )?;
            mappings.push(mapping);
        }
        mappings.sort_unstable_by_key(|m| m.ipa);
        check_no_overlap(&mappings)?;
        Ok(mappings)
    }

    /// Unmaps `mappings`, which this crate has just mapped, on a best-effort
    /// basis.
    fn unmap_all(&self, mappings: &[Mapping]) {
        for mapping in mappings.iter().rev() {
            let res = self.unmap(mapping.ipa, mapping.size);
            // The ranges were mapped by us and passed the same checks, so
            // unmapping them cannot fail short of a framework bug. There is no
            // better error to report than the one that caused the rollback.
            debug_assert!(res.is_ok(), "rollback of {mapping:?} failed: {res:?}");
        }
    }
}

/// Returns the host address, guest address and size of `region`.
fn region_mapping<R: GuestMemoryRegion>(region: &R) -> Result<Mapping> {
    let no_host_address = Error::NoHostAddress {
        ipa: region.start_addr().raw_value(),
    };
    let host = region
        .get_host_address(MemoryRegionAddress(0))
        .map_err(|_| no_host_address.clone())?;
    if host.is_null() {
        return Err(no_host_address);
    }
    let len = region.len();
    let size = usize::try_from(len).map_err(|_| Error::OutOfRange {
        what: "region size",
        value: len,
        limit: usize::MAX as u64,
    })?;
    Ok(Mapping {
        host,
        ipa: region.start_addr().raw_value(),
        size,
    })
}

/// Checks that no two of the `mappings`, sorted by guest address, overlap.
fn check_no_overlap(mappings: &[Mapping]) -> Result<()> {
    for pair in mappings.windows(2) {
        // `check_map_args` already rejected ranges that overflow.
        if pair[0].ipa + pair[0].size as u64 > pair[1].ipa {
            return Err(Error::Overlap {
                what: "guest memory region",
                other: "guest memory region",
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(ipa: u64, size: usize) -> Mapping {
        Mapping {
            host: std::ptr::null_mut(),
            ipa,
            size,
        }
    }

    #[test]
    fn test_check_no_overlap() {
        assert_eq!(check_no_overlap(&[]), Ok(()));
        assert_eq!(check_no_overlap(&[mapping(0, 0x4000)]), Ok(()));
        // Adjacent regions do not overlap.
        assert_eq!(
            check_no_overlap(&[mapping(0, 0x4000), mapping(0x4000, 0x4000)]),
            Ok(())
        );
        assert_eq!(
            check_no_overlap(&[mapping(0, 0x8000), mapping(0x4000, 0x4000)]),
            Err(Error::Overlap {
                what: "guest memory region",
                other: "guest memory region",
            })
        );
        // Only neighbours are compared, which suffices for sorted input.
        assert!(
            check_no_overlap(&[
                mapping(0, 0x4000),
                mapping(0x8000, 0x8000),
                mapping(0xc000, 0x4000)
            ])
            .is_err()
        );
    }
}
