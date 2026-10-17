// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Guest memory mapping, unmapping and protection.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.
//!
//! # File-backed and `MAP_SHARED` host memory
//!
//! `vm-memory` file regions are `MAP_SHARED` mappings of a file. The tests at
//! the end of this file check the in-contract behaviour of such mappings. A
//! one-off probe (macOS 27.0, Apple silicon) also looked at what the
//! framework does when the `Vm::map` safety contract is broken, which is not
//! something a test may do:
//!
//! - **In contract.** Guest stores to a `MAP_SHARED` file mapping reach the
//!   file (`pread` sees them), and host `pwrite`s to the file are visible to
//!   the guest. With `MAP_PRIVATE` the guest writes the private copy and the
//!   file is unchanged. A `MAP_SHARED` mapping of a file opened read-only
//!   cannot be mapped into the guest at all: `hv_vm_map` fails with
//!   `HV_ERROR`, even for `MemoryFlags::READ`. (An *anonymous* mapping whose
//!   protection was reduced to `PROT_READ` with `mprotect`, on the other hand,
//!   maps fine with `MemoryFlags::RW`, and guest stores to it succeed: guest
//!   permissions come only from the `MemoryFlags`.)
//! - **Truncation.** Shrinking the file under a guest mapping does not
//!   crash the host or the guest. A guest access to the truncated part exits
//!   with a stage-2 translation fault (a data abort that looks like MMIO), and
//!   the data stored before the truncation is gone.
//! - **`munmap` while guest-mapped (contract violation).** The guest keeps
//!   accessing the *old* pages: its stores still reach the file, even after
//!   the descriptor is closed and the file unlinked, while a new anonymous
//!   mapping placed at the same host address is not affected. This matches
//!   the earlier result for anonymous memory: the framework holds its own
//!   reference to the pages, so the violation leaks memory (or writes a file
//!   the VMM believes is closed) instead of corrupting unrelated host memory.
//!   The contract of `Vm::map` nevertheless stays strict, since this is
//!   observed behaviour, not a documented guarantee.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use common::{GUEST_IPA, GuestRam, HostMem, SPIN, boot_vcpu, expect_exception, hv_guard, hvc};
use hvf::{Error, ExceptionClass, Hvf, IpaGranule, MemoryFlags, Reg, VmConfig};

/// Guest physical address of the data page used by the store/load guests.
const DATA_IPA: u64 = 0x5000_0000;

/// `str w0, [x1]; hvc #0; b .`
const STORE_GUEST: [u32; 3] = [0xb900_0020, hvc(0), SPIN];
/// `ldr w0, [x1]; hvc #0; b .`
const LOAD_GUEST: [u32; 3] = [0xb940_0020, hvc(0), SPIN];

/// A unique file in the temporary directory, removed on drop.
struct TempFile {
    path: PathBuf,
    file: File,
}

impl TempFile {
    fn new(size: usize) -> TempFile {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        TempFile::create(
            std::env::temp_dir().join(format!("hvf-memory-{}-{n}", std::process::id())),
            size,
        )
    }

    /// Creates the file at `path`, which is named after this process.
    fn create(path: PathBuf, size: usize) -> TempFile {
        // A file with this live process's name can only have been left
        // behind by an earlier process with the same pid that died without
        // unwinding (SIGKILL, abort).
        let _ = std::fs::remove_file(&path);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.set_len(size as u64).unwrap();
        TempFile { path, file }
    }

    fn pread_u32(&self, offset: u64) -> u32 {
        let mut buf = [0u8; 4];
        std::os::unix::fs::FileExt::read_exact_at(&self.file, &mut buf, offset).unwrap();
        u32::from_le_bytes(buf)
    }

    fn pwrite_u32(&self, offset: u64, value: u32) {
        std::os::unix::fs::FileExt::write_all_at(&self.file, &value.to_le_bytes(), offset).unwrap();
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Regression test for A5-7: a file left behind by a killed run whose pid
/// this run reuses does not make the file-mapping tests fail.
#[test]
fn test_temp_file_replaces_stale_file() {
    let path = std::env::temp_dir().join(format!("hvf-memory-{}-stale", std::process::id()));
    std::fs::write(&path, b"stale").unwrap();
    let file = TempFile::create(path.clone(), 0x4000);
    assert_eq!(file.file.metadata().unwrap().len(), 0x4000);
    assert_eq!(file.pread_u32(0), 0);
    drop(file);
    assert!(!path.exists());
}

/// Creates a vCPU that starts at `GUEST_IPA` with `x0 = value` and
/// `x1 = addr`, the operands of `STORE_GUEST` and `LOAD_GUEST`.
fn vcpu_with(vm: &hvf::Vm, value: u64, addr: u64) -> hvf::Vcpu {
    let mut vcpu = boot_vcpu(vm, GUEST_IPA);
    vcpu.set_reg(Reg::x(0).unwrap(), value).unwrap();
    vcpu.set_reg(Reg::x(1).unwrap(), addr).unwrap();
    vcpu
}

fn assert_hv_error(r: hvf::Result<()>, call: &str) {
    match r {
        Err(Error::Hv { call: c, .. }) if c == call => {}
        other => panic!("expected an error from {call}, got {other:?}"),
    }
}

#[test]
fn test_argument_checks() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let granule = vm.ipa_granule();
    let size = granule as usize;
    let mem = HostMem::anon(2 * size);
    let host = mem.as_ptr();

    let misaligned = |r: hvf::Result<()>, expected: &str| match r {
        Err(Error::Misaligned { what, align, .. }) => {
            assert_eq!(what, expected);
            assert_eq!(align, granule);
        }
        other => panic!("expected Misaligned({expected}), got {other:?}"),
    };
    misaligned(
        // SAFETY: the arguments are rejected before anything is mapped.
        unsafe { vm.map(host.wrapping_add(0x800), GUEST_IPA, size, MemoryFlags::RW) },
        "host address",
    );
    misaligned(
        // SAFETY: as above.
        unsafe { vm.map(host, GUEST_IPA + 0x800, size, MemoryFlags::RW) },
        "ipa",
    );
    misaligned(
        // SAFETY: as above.
        unsafe { vm.map(host, GUEST_IPA, size + 0x800, MemoryFlags::RW) },
        "size",
    );
    misaligned(vm.unmap(GUEST_IPA + 0x800, size), "ipa");
    misaligned(
        vm.protect(GUEST_IPA, size - 0x800, MemoryFlags::READ),
        "size",
    );

    // Ranges beyond the IPA space, overflowing ranges and empty ranges.
    let end = 1u64 << vm.ipa_bits();
    assert!(matches!(
        // SAFETY: as above.
        unsafe { vm.map(host, end, size, MemoryFlags::RW) },
        Err(Error::OutOfRange { .. })
    ));
    assert!(matches!(
        // SAFETY: as above.
        unsafe { vm.map(host, end - granule, 2 * size, MemoryFlags::RW) },
        Err(Error::OutOfRange { .. })
    ));
    assert!(matches!(
        vm.unmap(u64::MAX - granule + 1, size),
        Err(Error::OutOfRange { .. })
    ));
    assert!(matches!(
        // SAFETY: as above.
        unsafe { vm.map(host, GUEST_IPA, 0, MemoryFlags::RW) },
        Err(Error::OutOfRange { value: 0, .. })
    ));

    // The last granule of the IPA space can be mapped.
    // SAFETY: `mem` outlives the mapping, which is removed below, and the
    // host does not access it.
    unsafe { vm.map(host, end - granule, size, MemoryFlags::RW) }.unwrap();
    vm.unmap(end - granule, size).unwrap();
}

#[test]
fn test_double_map_and_protect_unmapped() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let size = vm.ipa_granule() as usize;
    let mem = HostMem::anon(2 * size);

    // SAFETY: `mem` outlives every mapping, all of which are removed before
    // it is dropped, and the host does not access it.
    unsafe { vm.map(mem.as_ptr(), GUEST_IPA, 2 * size, MemoryFlags::RW) }.unwrap();
    // Overlapping mappings are rejected by the framework, whether they cover
    // the same or a partly overlapping range.
    // SAFETY: as above.
    assert_hv_error(
        unsafe { vm.map(mem.as_ptr(), GUEST_IPA, 2 * size, MemoryFlags::RW) },
        "hv_vm_map",
    );
    // SAFETY: as above.
    assert_hv_error(
        unsafe {
            vm.map(
                mem.as_ptr(),
                GUEST_IPA + size as u64,
                2 * size,
                MemoryFlags::RW,
            )
        },
        "hv_vm_map",
    );

    // Protecting part of a mapping works; protecting unmapped memory fails.
    vm.protect(GUEST_IPA, size, MemoryFlags::READ).unwrap();
    assert_hv_error(
        vm.protect(0x8000_0000, size, MemoryFlags::READ),
        "hv_vm_protect",
    );
    vm.unmap(GUEST_IPA, 2 * size).unwrap();
    assert_hv_error(
        vm.protect(GUEST_IPA, size, MemoryFlags::READ),
        "hv_vm_protect",
    );
}

#[test]
fn test_map_unmap_remap() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let mut first = GuestRam::with_code(&vm, &[hvc(1), SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 1 }
    );

    // Without a mapping, the instruction fetch faults.
    first.unmap().unwrap();
    drop(first);
    vcpu.set_reg(Reg::PC, GUEST_IPA).unwrap();
    match expect_exception(vcpu.run().unwrap()) {
        ExceptionClass::InstructionAbort { ipa, ifsc, .. } => {
            assert_eq!(ipa & !0xfff, GUEST_IPA);
            // A translation fault, at whatever level.
            assert_eq!(ifsc & 0x3c, 0x04, "ifsc {ifsc:#x}");
        }
        other => panic!("expected an instruction abort, got {other:?}"),
    }

    // Different host memory mapped at the same address is used from then on.
    let _second = GuestRam::with_code(&vm, &[hvc(2), SPIN]);
    vcpu.set_reg(Reg::PC, GUEST_IPA).unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 2 }
    );
}

#[test]
fn test_4k_granule() {
    let Some(_g) = hv_guard() else { return };
    let cfg = VmConfig {
        ipa_granule: Some(IpaGranule::K4),
        ..Default::default()
    };
    let vm = match Hvf::create_vm(&cfg) {
        Ok(vm) => vm,
        Err(Error::Unavailable { api, since }) => {
            println!("SKIP: {api} needs macOS {}.{}", since.0, since.1);
            return;
        }
        Err(e) => panic!("{e:?}"),
    };
    assert_eq!(vm.ipa_granule(), 4096);

    // A 4 KiB piece in the middle of a (16 KiB) host page, mapped at a 4 KiB
    // aligned address that is not 16 KiB aligned.
    let mem = HostMem::anon(Hvf::host_page_size().max(0x4000));
    mem.write_u32s(0x1000, &[hvc(7), SPIN]);
    let host = mem.as_ptr().wrapping_add(0x1000);
    // SAFETY: `mem` outlives the mapping, which is removed below, and the
    // host does not access it while the guest runs.
    unsafe { vm.map(host, 0x1000, 0x1000, MemoryFlags::READ | MemoryFlags::EXEC) }.unwrap();
    // A 2 KiB mapping is still misaligned.
    assert!(matches!(
        // SAFETY: rejected before anything is mapped.
        unsafe { vm.map(host, 0x8000, 0x800, MemoryFlags::READ) },
        Err(Error::Misaligned { what: "size", .. })
    ));

    let mut vcpu = boot_vcpu(&vm, 0x1000);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 7 }
    );
    // The neighbouring 4 KiB pages are not mapped.
    vcpu.set_reg(Reg::PC, 0x2000).unwrap();
    assert!(matches!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::InstructionAbort { ipa: 0x2000, .. }
    ));
    drop(vcpu);
    vm.unmap(0x1000, 0x1000).unwrap();
}

/// Regression test for P4: `unmap` takes effect synchronously, even for a
/// vCPU that is running at the time, so the host memory may be freed as soon
/// as it returns.
#[test]
fn test_unmap_while_running() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let mut ram = GuestRam::with_code(&vm, &[SPIN]);

    let (kicker_tx, kicker_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel();
    let vcpu_thread = {
        let vm = vm.clone();
        thread::spawn(move || {
            let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
            kicker_tx.send(vcpu.kicker()).unwrap();
            exit_tx.send(vcpu.run()).unwrap();
        })
    };
    let kicker = kicker_rx.recv().unwrap();
    // Let the guest spin for a while, then pull the memory away.
    thread::sleep(Duration::from_millis(50));
    ram.unmap().unwrap();
    // The memory is no longer reachable by the guest.
    drop(ram);

    let exit = match exit_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(exit) => exit.unwrap(),
        Err(_) => {
            kicker.kick().unwrap();
            panic!("the vCPU kept running after its code was unmapped");
        }
    };
    match expect_exception(exit) {
        ExceptionClass::InstructionAbort { ipa, ifsc, .. } => {
            assert_eq!(ipa & !0xfff, GUEST_IPA);
            assert_eq!(ifsc & 0x3c, 0x04, "ifsc {ifsc:#x}");
        }
        other => panic!("expected an instruction abort, got {other:?}"),
    }
    vcpu_thread.join().unwrap();
}

/// Write protection turns guest stores into data aborts, which is the
/// building block of dirty page tracking.
///
/// Only pages the guest has already accessed report a permission fault. A
/// store to a protected page that is not yet populated in the stage-2 tables
/// reports a translation fault instead (observed on macOS 27.0), so a dirty
/// tracker must treat any write abort on mapped memory as a dirtying write.
#[test]
fn test_protect_traps_writes() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &STORE_GUEST);
    let size = vm.ipa_granule() as usize;
    let data = GuestRam::new(&vm, DATA_IPA, 2 * size, MemoryFlags::RW);
    let second = DATA_IPA + size as u64;

    // The guest writes the first page while it is writable.
    let mut vcpu = vcpu_with(&vm, 0x0bad_cafe, DATA_IPA);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    drop(vcpu);
    assert_eq!(data.read_u32(0), 0x0bad_cafe);

    vm.protect(DATA_IPA, 2 * size, MemoryFlags::READ).unwrap();
    let mut vcpu = vcpu_with(&vm, 0x1234_5678, DATA_IPA + 0x10);
    match expect_exception(vcpu.run().unwrap()) {
        ExceptionClass::DataAbort(abort) => {
            assert!(abort.write);
            assert!(abort.is_permission_fault(), "dfsc {:#x}", abort.dfsc);
            assert_eq!(abort.ipa, DATA_IPA + 0x10);
        }
        other => panic!("expected a data abort, got {other:?}"),
    }
    // The store did not happen and the PC still points at it.
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), GUEST_IPA);
    assert_eq!(data.read_u32(0x10), 0);

    // After the VMM records the page as dirty and makes it writable again,
    // the store is simply retried.
    vm.protect(DATA_IPA, size, MemoryFlags::RW).unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(data.read_u32(0x10), 0x1234_5678);
    drop(vcpu);

    // The second page was never touched by the guest: the store still
    // exits, but not necessarily as a permission fault.
    let mut vcpu = vcpu_with(&vm, 0x5a5a_5a5a, second);
    match expect_exception(vcpu.run().unwrap()) {
        ExceptionClass::DataAbort(abort) => {
            assert!(abort.write);
            assert!(
                abort.is_translation_fault() || abort.is_permission_fault(),
                "dfsc {:#x}",
                abort.dfsc
            );
            assert_eq!(abort.ipa, second);
        }
        other => panic!("expected a data abort, got {other:?}"),
    }
    drop(vcpu);
    assert_eq!(data.read_u32(size), 0);

    // Read-only memory can still be read.
    let load = GuestRam::new(
        &vm,
        0x6000_0000,
        size,
        MemoryFlags::READ | MemoryFlags::EXEC,
    );
    load.write_u32s(0, &LOAD_GUEST);
    let mut vcpu = vcpu_with(&vm, 0, DATA_IPA + 0x10);
    vm.protect(DATA_IPA, size, MemoryFlags::READ).unwrap();
    vcpu.set_reg(Reg::PC, 0x6000_0000).unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(vcpu.get_reg(Reg::x(0).unwrap()).unwrap(), 0x1234_5678);

    // Without any permission, even loads exit.
    vm.protect(DATA_IPA, size, MemoryFlags::empty()).unwrap();
    vcpu.set_reg(Reg::PC, 0x6000_0000).unwrap();
    match expect_exception(vcpu.run().unwrap()) {
        ExceptionClass::DataAbort(abort) => {
            assert!(!abort.write);
            assert_eq!(abort.ipa, DATA_IPA + 0x10);
        }
        other => panic!("expected a data abort, got {other:?}"),
    }
}

/// Host and guest see each other's writes through a `MAP_SHARED` file
/// mapping, as with `vm-memory` file regions.
#[test]
fn test_shared_file_mapping() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &STORE_GUEST);
    let size = vm.ipa_granule() as usize;
    let file = TempFile::new(size);
    let mem = HostMem::file(
        &file.file,
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED,
    );
    // SAFETY: `mem` outlives the mapping, which is removed below, and the
    // host only accesses it with volatile operations.
    unsafe { vm.map(mem.as_ptr(), DATA_IPA, size, MemoryFlags::RW) }.unwrap();

    // A guest store reaches the file.
    let mut vcpu = vcpu_with(&vm, 0x1111_1111, DATA_IPA);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(file.pread_u32(0), 0x1111_1111);
    assert_eq!(mem.read_u32(0), 0x1111_1111);

    // A write to the file is seen by the guest.
    file.pwrite_u32(8, 0x2222_2222);
    let load = GuestRam::new(
        &vm,
        0x6000_0000,
        size,
        MemoryFlags::READ | MemoryFlags::EXEC,
    );
    load.write_u32s(0, &LOAD_GUEST);
    drop(vcpu);
    let mut vcpu = vcpu_with(&vm, 0, DATA_IPA + 8);
    vcpu.set_reg(Reg::PC, 0x6000_0000).unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(vcpu.get_reg(Reg::x(0).unwrap()).unwrap(), 0x2222_2222);
    vm.unmap(DATA_IPA, size).unwrap();
}

/// With `MAP_PRIVATE`, guest stores go to the private copy, not the file.
#[test]
fn test_private_file_mapping() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &STORE_GUEST);
    let size = vm.ipa_granule() as usize;
    let file = TempFile::new(size);
    let mem = HostMem::file(
        &file.file,
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE,
    );
    // SAFETY: as in `test_shared_file_mapping`.
    unsafe { vm.map(mem.as_ptr(), DATA_IPA, size, MemoryFlags::RW) }.unwrap();
    let mut vcpu = vcpu_with(&vm, 0x3333_3333, DATA_IPA + 4);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(mem.read_u32(4), 0x3333_3333);
    assert_eq!(file.pread_u32(4), 0);
    drop(vcpu);
    vm.unmap(DATA_IPA, size).unwrap();
}

/// A `MAP_SHARED` mapping of a file opened read-only cannot be mapped into
/// the guest at all, not even read-only.
#[test]
fn test_read_only_file_mapping() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let size = vm.ipa_granule() as usize;
    let file = TempFile::new(size);
    let ro = File::open(&file.path).unwrap();
    let mem = HostMem::file(&ro, size, libc::PROT_READ, libc::MAP_SHARED);
    for flags in [MemoryFlags::RW, MemoryFlags::READ] {
        assert_hv_error(
            // SAFETY: as in `test_shared_file_mapping`; the call fails.
            unsafe { vm.map(mem.as_ptr(), DATA_IPA, size, flags) },
            "hv_vm_map",
        );
    }
}

/// Truncating the file under a guest mapping makes guest accesses to the
/// lost part exit as stage-2 translation faults; neither side crashes.
#[test]
fn test_truncated_file_mapping() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &STORE_GUEST);
    let size = vm.ipa_granule() as usize;
    let file = TempFile::new(size);
    let mem = HostMem::file(
        &file.file,
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED,
    );
    // SAFETY: as in `test_shared_file_mapping`. The host does not touch the
    // memory after the truncation (that would raise SIGBUS).
    unsafe { vm.map(mem.as_ptr(), DATA_IPA, size, MemoryFlags::RW) }.unwrap();

    let mut vcpu = vcpu_with(&vm, 0x5555_5555, DATA_IPA);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(file.pread_u32(0), 0x5555_5555);

    file.file.set_len(0).unwrap();
    drop(vcpu);
    let mut vcpu = vcpu_with(&vm, 0x6666_6666, DATA_IPA + 16);
    match expect_exception(vcpu.run().unwrap()) {
        ExceptionClass::DataAbort(abort) => {
            assert!(abort.write);
            assert!(abort.is_translation_fault(), "dfsc {:#x}", abort.dfsc);
            assert_eq!(abort.ipa, DATA_IPA + 16);
        }
        other => panic!("expected a data abort, got {other:?}"),
    }
    drop(vcpu);
    vm.unmap(DATA_IPA, size).unwrap();

    // The truncated data is gone, and the failed store never happened.
    file.file.set_len(size as u64).unwrap();
    assert_eq!(file.pread_u32(0), 0);
    assert_eq!(file.pread_u32(16), 0);
}

#[cfg(feature = "vm-memory")]
mod guest_memory {
    use super::*;

    use vm_memory::{
        Address, Bytes, GuestAddress, GuestMemoryBackend, GuestMemoryMmap, GuestMemoryRegion,
    };

    /// Region size used by these tests: a multiple of every IPA granule, so
    /// the memory can be created before the VM (and is dropped after it).
    const REGION: usize = 0x1_0000;

    fn unmap_all(vm: &hvf::Vm, mem: &GuestMemoryMmap) {
        for region in mem.iter() {
            vm.unmap(region.start_addr().raw_value(), region.len() as usize)
                .unwrap();
        }
    }

    /// The regions of `mem` are mapped at their guest addresses, and guest
    /// code written through `vm-memory` runs.
    #[test]
    fn test_map_guest_memory() {
        let Some(_g) = hv_guard() else { return };
        let mem = GuestMemoryMmap::<()>::from_ranges(&[
            (GuestAddress(GUEST_IPA), REGION),
            (GuestAddress(DATA_IPA), 2 * REGION),
        ])
        .unwrap();
        let code: Vec<u8> = STORE_GUEST.iter().flat_map(|w| w.to_le_bytes()).collect();
        mem.write_slice(&code, GuestAddress(GUEST_IPA)).unwrap();
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();

        // SAFETY: `mem` outlives the VM, and the host only accesses it
        // through `Bytes`.
        unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX) }.unwrap();

        let target = DATA_IPA + REGION as u64 + 8;
        let mut vcpu = vcpu_with(&vm, 0xfeed_f00d, target);
        assert_eq!(
            expect_exception(vcpu.run().unwrap()),
            ExceptionClass::Hvc { imm: 0 }
        );
        assert_eq!(
            mem.read_obj::<u32>(GuestAddress(target)).unwrap(),
            0xfeed_f00d
        );
        drop(vcpu);

        // Each region is a separate mapping that can be removed on its own.
        unmap_all(&vm, &mem);
    }

    /// Nothing is mapped when one region fails validation.
    #[test]
    fn test_map_guest_memory_validates_first() {
        let Some(_g) = hv_guard() else { return };
        let mem = GuestMemoryMmap::<()>::from_ranges(&[
            (GuestAddress(GUEST_IPA), REGION),
            // Smaller than any granule.
            (GuestAddress(DATA_IPA), 0x800),
        ])
        .unwrap();
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        // SAFETY: `mem` outlives the VM; the call fails before mapping.
        let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RW) }.unwrap_err();
        assert!(matches!(e, Error::Misaligned { what: "size", .. }), "{e:?}");
        // The valid region was not mapped either.
        assert_hv_error(
            vm.protect(GUEST_IPA, REGION, MemoryFlags::READ),
            "hv_vm_protect",
        );
    }

    /// When a framework map call fails, the regions mapped by the same call
    /// are unmapped again.
    #[test]
    fn test_map_guest_memory_rolls_back() {
        let Some(_g) = hv_guard() else { return };
        let mem = GuestMemoryMmap::<()>::from_ranges(&[
            (GuestAddress(GUEST_IPA), REGION),
            (GuestAddress(DATA_IPA), REGION),
        ])
        .unwrap();
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        // Something else already occupies part of the second region.
        let other = GuestRam::new(
            &vm,
            DATA_IPA + REGION as u64 / 2,
            vm.ipa_granule() as usize,
            MemoryFlags::READ,
        );

        // SAFETY: `mem` outlives the VM, and the host does not access it.
        let e = unsafe { vm.map_guest_memory(&mem, MemoryFlags::RW) }.unwrap_err();
        assert!(
            matches!(
                e,
                Error::Hv {
                    call: "hv_vm_map",
                    ..
                }
            ),
            "{e:?}"
        );
        // The first region was rolled back...
        assert_hv_error(
            vm.protect(GUEST_IPA, REGION, MemoryFlags::READ),
            "hv_vm_protect",
        );
        // ...and the existing mapping is untouched.
        vm.protect(other.ipa(), other.len(), MemoryFlags::READ)
            .unwrap();

        // Once the conflict is gone the whole memory maps.
        drop(other);
        // SAFETY: as above.
        unsafe { vm.map_guest_memory(&mem, MemoryFlags::RW) }.unwrap();
        unmap_all(&vm, &mem);
    }
}
