// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! MMIO access arithmetic.
//!
//! Hypervisor.framework has no in-kernel MMIO emulation: a guest access to
//! unmapped memory exits with a data abort, and the VMM performs the transfer
//! between the guest register and the device. These helpers compute the
//! values involved, following the architectural rules for the register
//! width, sign extension and access size of the syndrome.

use crate::exit::DataAbortSyndrome;

/// The value to write to the guest's transfer register after an MMIO load of
/// `raw` (the little-endian device value, of which only the low
/// `syndrome.len` bytes are used).
///
/// The value is truncated to the access size, sign-extended if
/// `syndrome.sign_extend` is set, and truncated to 32 bits unless
/// `syndrome.sf64` is set.
pub fn mmio_load_value(_syndrome: &DataAbortSyndrome, _raw: u64) -> u64 {
    todo!()
}

/// The value an MMIO store transfers to the device, given the content `reg`
/// of the guest's transfer register: the low `syndrome.len` bytes of `reg`.
pub fn mmio_store_value(_syndrome: &DataAbortSyndrome, _reg: u64) -> u64 {
    todo!()
}
