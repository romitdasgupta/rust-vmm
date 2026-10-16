// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Stateless argument checks run before calling into the framework, so that
//! callers get a precise [`Error`](crate::Error) instead of `HV_BAD_ARGUMENT`.

// The callers are only compiled on macOS/aarch64; the checks are tested on
// every target.
#![cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]

use crate::error::Result;
use crate::gic_types::{GicConfig, GicParams};

/// Checks the arguments of a memory map, unmap or protect call.
///
/// `host` is the host address for `map` (`None` for `unmap`/`protect`).
/// `host`, `ipa` and `size` must be multiples of `granule`, `size` must not be
/// zero, and `[ipa, ipa + size)` must fit in an IPA space of `ipa_bits` bits.
#[allow(dead_code)]
pub(crate) fn check_map_args(
    _host: Option<usize>,
    _ipa: u64,
    _size: usize,
    _granule: u64,
    _ipa_bits: u32,
) -> Result<()> {
    todo!()
}

/// Checks a GIC layout against the host's requirements: alignment of every
/// base, no overlap between the distributor, redistributor and MSI regions,
/// every region within an IPA space of `ipa_bits` bits, and the MSI interrupt
/// range within the SPI range.
#[allow(dead_code)]
pub(crate) fn check_gic_layout(
    _cfg: &GicConfig,
    _params: &GicParams,
    _ipa_bits: u32,
) -> Result<()> {
    todo!()
}
