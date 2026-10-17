// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

#![deny(missing_docs)]
#![deny(missing_copy_implementations)]
#![deny(missing_debug_implementations)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![warn(clippy::assertions_on_result_states)]
// The crate documentation links to items that only exist on macOS/aarch64,
// the only target the documentation is published for.
#![cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(rustdoc::broken_intra_doc_links)
)]

//! Safe wrappers over the macOS Hypervisor.framework on Apple silicon.
//!
//! The crate is a thin layer, in the spirit of `kvm-ioctls`:
//!
//! - [`Hvf`] probes the host and creates the (single, per-process) [`Vm`].
//! - [`Vm`] maps guest memory, creates [`Vcpu`]s and the optional in-kernel
//!   GICv3 ([`Gic`]).
//! - [`Vcpu`] runs guest code and accesses registers. It is bound to the
//!   thread that created it and is therefore neither `Send` nor `Sync`; a
//!   [`VcpuKicker`] forces a running vCPU out of the guest from any thread.
//! - [`VcpuExit`] and [`ExceptionClass`] decode exits, including the MMIO
//!   data aborts a VMM has to emulate.
//!
//! VMM policy (memory layout, boot protocol, device models, interrupt
//! routing) is left to the caller.
//!
//! # Platform support
//!
//! - `aarch64-apple-darwin`, macOS 11.0 or newer. Functions introduced by
//!   later releases are resolved at run time and return
//!   [`Error::Unavailable`] on older systems.
//!
//! The portable parts (errors, register identifiers, exit decoding, MMIO and
//! alignment arithmetic) also build on other targets, so they can be tested
//! and reused anywhere.
//!
//! Binaries using this crate must be signed with the
//! `com.apple.security.hypervisor` entitlement. Without it, VM creation fails
//! with `HV_DENIED`.
//!
//! # Example - running an MMIO guest
//!
//! The guest below performs a few MMIO accesses to an unmapped region, issues
//! an `hvc #0`, and then waits for an interrupt with `wfi`.
//!
//! ```
//! # #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use hvf::{ExceptionClass, Hvf, MemoryFlags, Reg, VcpuExit, VmConfig};
//!
//! // mov x1, #0x09000000; mov w0, #0x42; str w0, [x1]; ldr w2, [x1, #4];
//! // strb w0, [x1, #8]; str x2, [x1, #16]; hvc #0; wfi; b .
//! const GUEST: [u8; 36] = [
//!     0x01, 0x20, 0xa1, 0xd2, 0x40, 0x08, 0x80, 0x52, 0x20, 0x00, 0x00, 0xb9, 0x22, 0x04, 0x40,
//!     0xb9, 0x20, 0x20, 0x00, 0x39, 0x22, 0x08, 0x00, 0xf9, 0x02, 0x00, 0x00, 0xd4, 0x7f, 0x20,
//!     0x03, 0xd5, 0x00, 0x00, 0x00, 0x14,
//! ];
//! const GUEST_IPA: u64 = 0x4000_0000;
//! const MMIO_BASE: u64 = 0x0900_0000;
//!
//! // 1. Create the VM.
//! let vm = match Hvf::create_vm(&VmConfig::default()) {
//!     Ok(vm) => vm,
//! #   // Not signed, not supported, or running in a VM without nested
//! #   // virtualization. HVF_REQUIRE=1 makes this a failure, as in the tests.
//! #   Err(hvf::Error::Hv { err, .. })
//! #       if err.is_unavailable() && std::env::var_os("HVF_REQUIRE").is_none_or(|v| v != "1") =>
//! #   {
//! #       return Ok(())
//! #   }
//!     Err(e) => return Err(e.into()),
//! };
//!
//! // 2. Allocate one IPA granule of guest memory and copy the guest code in.
//! let size = vm.ipa_granule() as usize;
//! // SAFETY: anonymous private mapping, checked below.
//! let host = unsafe {
//!     libc::mmap(
//!         std::ptr::null_mut(),
//!         size,
//!         libc::PROT_READ | libc::PROT_WRITE,
//!         libc::MAP_ANON | libc::MAP_PRIVATE,
//!         -1,
//!         0,
//!     )
//! };
//! assert_ne!(host, libc::MAP_FAILED);
//! // SAFETY: `host` is a fresh mapping of `size >= GUEST.len()` bytes.
//! unsafe { std::ptr::copy_nonoverlapping(GUEST.as_ptr(), host.cast(), GUEST.len()) };
//!
//! // 3. Map it into the guest.
//! // SAFETY: the mapping stays valid until it is unmapped below, after the
//! // VM is gone, and the host does not access it while the guest runs.
//! unsafe {
//!     vm.map(
//!         host.cast(),
//!         GUEST_IPA,
//!         size,
//!         MemoryFlags::READ | MemoryFlags::EXEC,
//!     )?
//! };
//!
//! // 4. Create a vCPU on this thread and point it at the guest code.
//! let mut vcpu = vm.create_vcpu(None)?;
//! vcpu.set_reg(Reg::PC, GUEST_IPA)?;
//! vcpu.set_reg(Reg::CPSR, 0x3c5)?; // EL1h, interrupts masked.
//!
//! // 5. Run it and emulate a small MMIO device.
//! let mut device = [0u8; 0x20];
//! let mut hvc_seen = false;
//! loop {
//!     match vcpu.run()? {
//!         VcpuExit::Exception(exception) => match exception.decode() {
//!             ExceptionClass::DataAbort(abort) => {
//!                 let access = abort.syndrome.expect("instruction syndrome");
//!                 let offset = (abort.ipa - MMIO_BASE) as usize;
//!                 let len = usize::from(access.len);
//!                 if abort.write {
//!                     let value = vcpu.mmio_write_value(&access)?;
//!                     device[offset..offset + len].copy_from_slice(&value.to_le_bytes()[..len]);
//!                     vcpu.advance_pc()?;
//!                 } else {
//!                     vcpu.complete_mmio_read(&access, 0xdead_beef)?;
//!                 }
//!             }
//!             // HVF has already moved the PC past the `hvc`.
//!             ExceptionClass::Hvc { imm: 0 } => hvc_seen = true,
//!             ExceptionClass::Wfx { wfe: false } => break,
//!             other => panic!("unexpected exception: {other:?}"),
//!         },
//!         other => panic!("unexpected exit: {other:?}"),
//!     }
//! }
//! assert!(hvc_seen);
//! assert_eq!(device[0], 0x42);
//! assert_eq!(&device[0x10..0x18], &0xdead_beef_u64.to_le_bytes());
//! assert_eq!(vcpu.get_reg(Reg::x(2).unwrap())?, 0xdead_beef);
//!
//! // 6. Tear down: the VM is destroyed when the last handle is dropped.
//! drop(vcpu);
//! drop(vm);
//! // SAFETY: nothing references the mapping any more.
//! unsafe { libc::munmap(host, size) };
//! # Ok(())
//! # }
//! # #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
//! # fn main() {}
//! ```

mod align;
mod error;
mod exit;
mod gic_types;
mod mmio;
mod regs;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[macro_use]
mod avail;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod gic;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod os_object;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod system;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod vcpu;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod vm;
#[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "vm-memory"))]
mod vm_memory;

pub use error::{Error, HvError, Result};
pub use exit::{DataAbort, DataAbortSyndrome, Exception, ExceptionClass, SysRegAccess, VcpuExit};
pub use gic_types::{GicConfig, GicIntid, GicParams, MsiConfig};
pub use mmio::{mmio_load_value, mmio_store_value};
pub use regs::{CacheType, FeatureReg, InterruptType, Reg, SimdReg, SysReg, TlbiOp};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use gic::Gic;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use system::{Hvf, IpaGranule, VcpuConfig, VmConfig};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use vcpu::{Vcpu, VcpuKicker};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use vm::{MemoryFlags, Vm};
