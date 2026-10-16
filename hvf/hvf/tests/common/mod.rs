// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::sync::MutexGuard;

/// Serializes the HVF tests of one test binary (one VM per process) and skips
/// them when Hypervisor.framework is unusable.
///
/// Returns `None` after printing `SKIP: ...` when the host does not support
/// HVF or the binary lacks the entitlement; panics instead when the
/// environment variable `HVF_REQUIRE=1` is set. Bind the guard first so it is
/// dropped last: `let Some(_g) = hv_guard() else { return };`.
pub fn hv_guard() -> Option<MutexGuard<'static, ()>> {
    todo!()
}
