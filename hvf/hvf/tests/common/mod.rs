// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard, PoisonError};

use hvf::{Error, Hvf, VmConfig};

/// Serializes the HVF tests of one test binary (one VM per process) and skips
/// them when Hypervisor.framework is unusable.
///
/// Returns `None` after printing `SKIP: ...` when the host does not support
/// HVF or the binary lacks the entitlement; panics instead when the
/// environment variable `HVF_REQUIRE=1` is set. Bind the guard first so it is
/// dropped last: `let Some(_g) = hv_guard() else { return };`.
pub fn hv_guard() -> Option<MutexGuard<'static, ()>> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let reason = if !Hvf::is_supported() {
        "kern.hv_support is not 1".to_string()
    } else {
        match Hvf::create_vm(&VmConfig::default()) {
            Ok(_) => return Some(guard),
            Err(Error::Hv { err, .. }) if err.is_unavailable() => err.to_string(),
            Err(e) => panic!("cannot create a VM: {e}"),
        }
    };
    let require = std::env::var_os("HVF_REQUIRE").is_some_and(|v| v == "1");
    assert!(!require, "HVF_REQUIRE=1 but HVF is unusable: {reason}");
    println!("SKIP: Hypervisor.framework is unusable: {reason}");
    None
}
