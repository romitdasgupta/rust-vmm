// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! vCPU thread binding and kicking from other threads.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
