// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Integration with `vm-memory` (feature `vm-memory`).

use vm_memory::GuestMemory;

use crate::error::Result;
use crate::vm::{MemoryFlags, Vm};

impl Vm {
    /// Maps every region of `mem` into the guest at its guest address.
    ///
    /// All regions are validated before anything is mapped. If mapping one
    /// region fails, the regions mapped so far are unmapped again.
    ///
    /// # Safety
    ///
    /// The same as [`Vm::map`] for the host memory of every region: it must
    /// stay mapped until it is unmapped from the guest or every VM handle is
    /// dropped, and host code must only access it through volatile or atomic
    /// operations.
    pub unsafe fn map_guest_memory<M: GuestMemory>(
        &self,
        _mem: &M,
        _flags: MemoryFlags,
    ) -> Result<()> {
        todo!()
    }
}
