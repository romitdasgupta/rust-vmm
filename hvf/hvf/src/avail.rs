// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Run-time availability of functions newer than macOS 11.0.

use std::sync::OnceLock;

use hvf_bindings::DynApi;

/// The process-wide table of run-time resolved framework functions.
#[allow(dead_code)]
pub(crate) fn api() -> &'static DynApi {
    static API: OnceLock<DynApi> = OnceLock::new();
    API.get_or_init(DynApi::load)
}

/// Evaluates to the run-time resolved pointer of the framework function
/// `$name`, or returns `Err(Error::Unavailable { .. })` from the enclosing
/// function if the running macOS release does not provide it.
///
/// ```ignore
/// let create = need!(hv_vm_config_create);
/// // SAFETY: ...
/// let config = unsafe { create() };
/// ```
#[allow(unused_macros)]
macro_rules! need {
    ($name:ident) => {
        match $crate::avail::api().$name {
            Some(f) => f,
            None => {
                return Err($crate::Error::Unavailable {
                    api: stringify!($name),
                    since: hvf_bindings::since::$name,
                });
            }
        }
    };
}
