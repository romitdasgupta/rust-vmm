// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Helpers shared by the integration tests.
//!
//! Every test file is its own process, and Hypervisor.framework allows one VM
//! per process, so the tests of one file are serialized by [`hv_guard`].

#![allow(dead_code)]

use std::fs::File;
use std::mem::ManuallyDrop;
use std::os::fd::AsRawFd;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use hvf::{Error, ExceptionClass, Hvf, MemoryFlags, Reg, Vcpu, VcpuExit, VcpuKicker, Vm, VmConfig};

/// Guest physical address the test guests are loaded at.
pub const GUEST_IPA: u64 = 0x4000_0000;

/// `CPSR` of a fresh vCPU in the tests: EL1h with `DAIF` masked.
pub const CPSR_EL1H_MASKED: u64 = 0x3c5;

/// `b .`
pub const SPIN: u32 = 0x1400_0000;
/// `wfi`
pub const WFI: u32 = 0xd503_207f;
/// `eret`
pub const ERET: u32 = 0xd69f_03e0;

/// `hvc #imm`
pub const fn hvc(imm: u16) -> u32 {
    0xd400_0002 | ((imm as u32) << 5)
}

/// The MMIO guest of `mmio_exit.rs`, at [`GUEST_IPA`], accessing the
/// unmapped region at [`MMIO_BASE`]:
///
/// ```text
/// 0x00: d2a12001  mov  x1, #0x09000000
/// 0x04: 52800840  mov  w0, #0x42
/// 0x08: b9000020  str  w0, [x1]          ; 32-bit write
/// 0x0c: b9400422  ldr  w2, [x1, #4]      ; 32-bit read -> x2
/// 0x10: 39002020  strb w0, [x1, #8]      ; 8-bit write
/// 0x14: f9000822  str  x2, [x1, #16]     ; 64-bit write
/// 0x18: d4000002  hvc  #0
/// 0x1c: d503207f  wfi
/// 0x20: 14000000  b .
/// ```
pub const MMIO_GUEST: [u32; 9] = [
    0xd2a1_2001,
    0x5280_0840,
    0xb900_0020,
    0xb940_0422,
    0x3900_2020,
    0xf900_0822,
    hvc(0),
    WFI,
    SPIN,
];

/// Base of the MMIO region [`MMIO_GUEST`] accesses.
pub const MMIO_BASE: u64 = 0x0900_0000;

/// Serializes the HVF tests of one test binary (one VM per process) and skips
/// them when Hypervisor.framework is unusable.
///
/// Returns `None` after printing `SKIP: ...` when the host does not support
/// HVF or the binary lacks the entitlement; panics instead when the
/// environment variable `HVF_REQUIRE=1` is set. Bind the guard first so it is
/// dropped last: `let Some(_g) = hv_guard() else { return };`.
pub fn hv_guard() -> Option<MutexGuard<'static, ()>> {
    static LOCK: Mutex<()> = Mutex::new(());
    // A test that failed while holding the lock leaves nothing behind that
    // the next test cares about: its VM was dropped while unwinding.
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let require = std::env::var_os("HVF_REQUIRE").is_some_and(|v| v == "1");
    let reason = if !Hvf::is_supported() {
        "kern.hv_support is not 1".to_string()
    } else {
        match Hvf::create_vm(&VmConfig::default()) {
            // The probe VM is destroyed right away, before the test runs.
            Ok(_) => return Some(guard),
            Err(Error::Hv { err, .. }) if err.is_unavailable() => err.to_string(),
            Err(e) => panic!("cannot create a probe VM: {e}"),
        }
    };
    assert!(!require, "HVF_REQUIRE=1 but HVF is unusable: {reason}");
    println!("SKIP: Hypervisor.framework is unusable: {reason}");
    None
}

/// A host memory mapping (anonymous or of a file), unmapped on drop.
///
/// The host accesses it only through volatile operations, so it may be mapped
/// into a guest; the test must then remove the guest mapping before dropping
/// it, or use [`GuestRam`], which does so itself.
#[derive(Debug)]
pub struct HostMem {
    ptr: *mut u8,
    size: usize,
}

impl HostMem {
    /// Maps `size` bytes of zeroed anonymous memory.
    pub fn anon(size: usize) -> HostMem {
        // SAFETY: an anonymous private mapping has no preconditions; the
        // result is checked below.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANON | libc::MAP_PRIVATE,
                -1,
                0,
            )
        };
        assert_ne!(ptr, libc::MAP_FAILED, "mmap of {size:#x} bytes failed");
        HostMem {
            ptr: ptr.cast(),
            size,
        }
    }

    /// Maps the first `size` bytes of `file` with protection `prot` and
    /// mapping flags `flags` (for example `MAP_SHARED`).
    pub fn file(file: &File, size: usize, prot: i32, flags: i32) -> HostMem {
        // SAFETY: a file mapping at an address chosen by the kernel does not
        // affect existing memory; the result is checked below.
        let ptr =
            unsafe { libc::mmap(std::ptr::null_mut(), size, prot, flags, file.as_raw_fd(), 0) };
        assert_ne!(ptr, libc::MAP_FAILED, "mmap of {size:#x} file bytes failed");
        HostMem {
            ptr: ptr.cast(),
            size,
        }
    }

    /// The start of the mapping.
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// The size of the mapping.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Writes 32-bit words (for example instructions) at `offset`.
    ///
    /// Uses volatile accesses, so it may be called while the memory is mapped
    /// into a guest.
    pub fn write_u32s(&self, offset: usize, words: &[u32]) {
        assert!(offset.is_multiple_of(4) && offset + 4 * words.len() <= self.size);
        for (i, word) in words.iter().enumerate() {
            // SAFETY: in bounds and aligned (checked above, and the mapping is
            // page aligned); the memory is only ever accessed with volatile
            // operations by the host.
            unsafe {
                self.ptr
                    .add(offset + 4 * i)
                    .cast::<u32>()
                    .write_volatile(*word)
            };
        }
    }

    /// Reads the 32-bit word at `offset` with a volatile access.
    pub fn read_u32(&self, offset: usize) -> u32 {
        assert!(offset.is_multiple_of(4) && offset + 4 <= self.size);
        // SAFETY: in bounds and aligned; volatile access as in `write_u32s`.
        unsafe { self.ptr.add(offset).cast::<u32>().read_volatile() }
    }

    /// Reads the 64-bit word at `offset` with a volatile access.
    pub fn read_u64(&self, offset: usize) -> u64 {
        assert!(offset.is_multiple_of(8) && offset + 8 <= self.size);
        // SAFETY: in bounds and aligned; volatile access as in `write_u32s`.
        unsafe { self.ptr.add(offset).cast::<u64>().read_volatile() }
    }
}

impl Drop for HostMem {
    fn drop(&mut self) {
        // SAFETY: `ptr` is a mapping of `size` bytes owned by `self`. Users of
        // `HostMem` keep it alive until it is no longer mapped into a guest
        // (see `GuestRam`).
        unsafe { libc::munmap(self.ptr.cast(), self.size) };
    }
}

/// Host memory mapped into a VM, unmapped from the guest and then from the
/// host on drop.
///
/// It holds a [`Vm`] handle, so it can be dropped in any order relative to
/// the test's own `Vm` and `Vcpu`s: the guest mapping is always removed before
/// the host memory is freed. Drop it before checking that the VM is gone.
#[derive(Debug)]
pub struct GuestRam {
    vm: Vm,
    ipa: u64,
    mapped: bool,
    mem: ManuallyDrop<HostMem>,
}

impl GuestRam {
    /// Maps `size` bytes of fresh anonymous memory at `ipa` with `flags`.
    pub fn new(vm: &Vm, ipa: u64, size: usize, flags: MemoryFlags) -> GuestRam {
        let mem = HostMem::anon(size);
        // SAFETY: `mem` stays mapped until `drop`, which first removes the
        // guest mapping, and the host only accesses it with volatile
        // operations.
        unsafe { vm.map(mem.as_ptr(), ipa, size, flags) }.unwrap();
        GuestRam {
            vm: vm.clone(),
            ipa,
            mapped: true,
            mem: ManuallyDrop::new(mem),
        }
    }

    /// Maps memory holding `code` at [`GUEST_IPA`], readable and executable.
    /// The size is the smallest multiple of the IPA granule that fits.
    pub fn with_code(vm: &Vm, code: &[u32]) -> GuestRam {
        let granule = vm.ipa_granule() as usize;
        let size = (4 * code.len()).next_multiple_of(granule).max(granule);
        let ram = GuestRam::new(vm, GUEST_IPA, size, MemoryFlags::READ | MemoryFlags::EXEC);
        ram.write_u32s(0, code);
        ram
    }

    /// The guest physical address of the memory.
    pub fn ipa(&self) -> u64 {
        self.ipa
    }

    /// The size of the memory.
    pub fn len(&self) -> usize {
        self.mem.len()
    }

    /// Writes 32-bit words at `offset`, see [`HostMem::write_u32s`].
    pub fn write_u32s(&self, offset: usize, words: &[u32]) {
        self.mem.write_u32s(offset, words);
    }

    /// Reads the 32-bit word at `offset`, see [`HostMem::read_u32`].
    pub fn read_u32(&self, offset: usize) -> u32 {
        self.mem.read_u32(offset)
    }

    /// Reads the 64-bit word at `offset`, see [`HostMem::read_u64`].
    pub fn read_u64(&self, offset: usize) -> u64 {
        self.mem.read_u64(offset)
    }

    /// Removes the guest mapping now, for tests that observe the unmap.
    pub fn unmap(&mut self) -> hvf::Result<()> {
        assert!(self.mapped, "already unmapped");
        self.vm.unmap(self.ipa, self.mem.len())?;
        self.mapped = false;
        Ok(())
    }
}

impl Drop for GuestRam {
    fn drop(&mut self) {
        if self.mapped && self.vm.unmap(self.ipa, self.mem.len()).is_err() {
            // The guest may still reach the memory: leak it rather than
            // freeing memory a guest can write.
            eprintln!("GuestRam: unmap of {:#x} failed, leaking it", self.ipa);
            return;
        }
        // SAFETY: `mem` is not used again, and the guest can no longer
        // access it.
        unsafe { ManuallyDrop::drop(&mut self.mem) };
    }
}

/// Creates a vCPU on the calling thread that starts at `pc` in EL1h with
/// interrupts masked.
pub fn boot_vcpu(vm: &Vm, pc: u64) -> Vcpu {
    let mut vcpu = vm.create_vcpu(None).unwrap();
    vcpu.set_reg(Reg::PC, pc).unwrap();
    vcpu.set_reg(Reg::CPSR, CPSR_EL1H_MASKED).unwrap();
    vcpu
}

/// Decodes an exception exit, panicking on any other exit.
pub fn expect_exception(exit: VcpuExit) -> ExceptionClass {
    match exit {
        VcpuExit::Exception(e) => e.decode(),
        other => panic!("expected an exception exit, got {other:?}"),
    }
}

/// Kicks `kicker` after `delay` from a helper thread.
///
/// Returns a channel that receives the result of the kick. Used to end runs
/// of guests that would otherwise spin forever.
pub fn kick_after(kicker: VcpuKicker, delay: Duration) -> mpsc::Receiver<hvf::Result<bool>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        thread::sleep(delay);
        // The receiver may be gone if the test already failed.
        let _ = tx.send(kicker.kick());
    });
    rx
}

/// Mach absolute time, the time base of the vCPU's virtual counter.
pub mod mach_time {
    #[repr(C)]
    struct TimebaseInfo {
        numer: u32,
        denom: u32,
    }

    // `libc` deprecated its copies of these declarations in favour of the
    // `mach2` crate; declaring the two functions here avoids a dependency.
    unsafe extern "C" {
        safe fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut TimebaseInfo) -> i32;
    }

    /// The current Mach absolute time. `CNTVCT_EL0` of a vCPU is this value
    /// minus the vCPU's vtimer offset.
    pub fn now() -> u64 {
        mach_absolute_time()
    }

    /// Converts nanoseconds to Mach absolute time units.
    pub fn from_nanos(ns: u64) -> u64 {
        let mut info = TimebaseInfo { numer: 0, denom: 0 };
        // SAFETY: `info` is valid for writes.
        assert_eq!(unsafe { mach_timebase_info(&mut info) }, 0);
        ns * u64::from(info.denom) / u64::from(info.numer)
    }
}
