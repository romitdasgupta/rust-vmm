// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Virtual timer exits and masking.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
