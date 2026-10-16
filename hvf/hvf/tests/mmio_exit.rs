// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! End-to-end MMIO exits through a `vm-device` bus.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use std::sync::{Arc, Mutex};

use common::{GUEST_IPA, MMIO_BASE, MMIO_GUEST, SPIN, boot_vcpu, expect_exception, hv_guard};
use hvf::{DataAbort, Exception, ExceptionClass, Hvf, MemoryFlags, Reg, Vcpu, VcpuExit, VmConfig};
use vm_device::DeviceMmio;
use vm_device::bus::{Bus, BusRange, MmioAddress, MmioAddressOffset};
use vm_memory::{Bytes, GuestAddress, GuestMemoryBackend, GuestMemoryMmap, GuestMemoryRegion};

/// Value the recorder returns for every read.
const READ_VALUE: u64 = 0xdead_beef;

/// One access seen by the [`Recorder`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum Access {
    /// A read of `len` bytes at `offset`.
    Read { offset: u64, len: usize },
    /// A write of `value` (little endian, `len` bytes) at `offset`.
    Write { offset: u64, len: usize, value: u64 },
}

/// An MMIO device that records every access and reads as [`READ_VALUE`].
#[derive(Debug, Default)]
struct Recorder {
    log: Mutex<Vec<Access>>,
}

impl DeviceMmio for Recorder {
    fn mmio_read(&self, _base: MmioAddress, offset: MmioAddressOffset, data: &mut [u8]) {
        let len = data.len();
        data.copy_from_slice(&READ_VALUE.to_le_bytes()[..len]);
        self.log.lock().unwrap().push(Access::Read { offset, len });
    }

    fn mmio_write(&self, _base: MmioAddress, offset: MmioAddressOffset, data: &[u8]) {
        let mut bytes = [0u8; 8];
        bytes[..data.len()].copy_from_slice(data);
        self.log.lock().unwrap().push(Access::Write {
            offset,
            len: data.len(),
            value: u64::from_le_bytes(bytes),
        });
    }
}

type MmioBus = Bus<MmioAddress, Arc<dyn DeviceMmio + Send + Sync>>;

/// Emulates the MMIO access described by `abort` on `bus`, completing the
/// instruction (loaded register and PC) on `vcpu`.
fn handle_mmio(vcpu: &mut Vcpu, bus: &MmioBus, abort: &DataAbort) {
    assert!(abort.is_translation_fault(), "dfsc {:#x}", abort.dfsc);
    let access = abort
        .syndrome
        .expect("the guest uses instructions with a valid syndrome");
    let len = usize::from(access.len);
    let (range, device) = bus
        .check_access(MmioAddress(abort.ipa), len)
        .unwrap_or_else(|e| panic!("no device for {:#x}+{len}: {e}", abort.ipa));
    let offset = abort.ipa - range.base().0;
    if abort.write {
        let value = vcpu.mmio_write_value(&access).unwrap();
        device.mmio_write(range.base(), offset, &value.to_le_bytes()[..len]);
        vcpu.advance_pc().unwrap();
    } else {
        let mut bytes = [0u8; 8];
        device.mmio_read(range.base(), offset, &mut bytes[..len]);
        vcpu.complete_mmio_read(&access, u64::from_le_bytes(bytes))
            .unwrap();
    }
}

#[test]
fn test_mmio_guest() {
    let Some(_g) = hv_guard() else { return };

    // Guest memory comes from vm-memory. It is created before the VM, so it
    // is dropped after it even if the test fails. 64 KiB is a multiple of
    // every IPA granule.
    let mem = GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(GUEST_IPA), 0x1_0000)]).unwrap();
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let code: Vec<u8> = MMIO_GUEST.iter().flat_map(|w| w.to_le_bytes()).collect();
    mem.write_slice(&code, GuestAddress(GUEST_IPA)).unwrap();
    let region = mem.iter().next().unwrap();
    let host = mem.get_host_address(GuestAddress(GUEST_IPA)).unwrap();
    // SAFETY: `mem` stays alive until after the region is unmapped below, and
    // the host only accesses it through `Bytes`.
    unsafe { vm.map(host, GUEST_IPA, region.len() as usize, MemoryFlags::RWX) }.unwrap();

    let recorder = Arc::new(Recorder::default());
    let mut bus = MmioBus::new();
    bus.register(
        BusRange::new(MmioAddress(MMIO_BASE), 0x1000).unwrap(),
        recorder.clone(),
    )
    .unwrap();

    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    let mut syndromes = Vec::new();
    let mut hvc_pc = None;
    let wfi_pc = loop {
        let exit = vcpu.run().unwrap();
        let VcpuExit::Exception(exception) = exit else {
            panic!("unexpected exit {exit:?}");
        };
        syndromes.push(exception.syndrome);
        match exception.decode() {
            ExceptionClass::DataAbort(abort) => {
                assert_eq!(abort.ipa, exception.physical_address);
                handle_mmio(&mut vcpu, &bus, &abort);
            }
            class @ ExceptionClass::Hvc { imm: 0 } => {
                // HVF already moved the PC past the `hvc`; it must not be
                // advanced again.
                assert!(class.pc_advanced_by_hw());
                hvc_pc = Some(vcpu.get_reg(Reg::PC).unwrap());
            }
            ExceptionClass::Wfx { wfe: false } => break vcpu.get_reg(Reg::PC).unwrap(),
            other => panic!("unexpected exception {other:?} ({exception:x?})"),
        }
    };

    assert_eq!(
        *recorder.log.lock().unwrap(),
        [
            Access::Write {
                offset: 0x0,
                len: 4,
                value: 0x42
            },
            Access::Read {
                offset: 0x4,
                len: 4
            },
            Access::Write {
                offset: 0x8,
                len: 1,
                value: 0x42
            },
            Access::Write {
                offset: 0x10,
                len: 8,
                value: READ_VALUE
            },
        ]
    );
    // The syndromes match the ones recorded when the guest was first run,
    // ignoring the fault status code (the translation level may vary).
    let masked: Vec<u64> = syndromes.iter().map(|esr| esr & !0x3f).collect();
    assert_eq!(
        masked,
        [
            0x9380_0040,
            0x9382_0000,
            0x9300_0040,
            0x93c2_8040,
            0x5a00_0000,
            0x07e0_0000
        ]
    );
    // The `hvc` is at offset 0x18 and the `wfi` at 0x1c: the PC after the
    // HVC exit already points at the `wfi`, which is where the WFx exit is.
    assert_eq!(hvc_pc, Some(GUEST_IPA + 0x1c));
    assert_eq!(wfi_pc, GUEST_IPA + 0x1c);
    // The value read from the device ended up in x2.
    assert_eq!(vcpu.get_reg(Reg::x(2).unwrap()).unwrap(), READ_VALUE);

    drop(vcpu);
    vm.unmap(GUEST_IPA, region.len() as usize).unwrap();
}

/// Loads and stores through `xzr`, sign-extending loads and accesses to an
/// address no device claims.
#[test]
fn test_mmio_corner_cases() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    // mov x1, #0x09000000
    // str  xzr, [x1, #0x20]   ; store of the zero register
    // ldr  wzr, [x1, #0x24]   ; load into the zero register
    // ldrsb x3, [x1, #0x28]   ; sign-extending byte load
    // ldrsh w4, [x1, #0x2a]   ; sign-extending halfword load into a W register
    // hvc #0
    // b .
    let code = common::GuestRam::with_code(
        &vm,
        &[
            0xd2a1_2001,
            0xf900_103f,
            0xb940_243f,
            0x3980_a023,
            0x79c0_5424,
            common::hvc(0),
            SPIN,
        ],
    );
    let recorder = Arc::new(Recorder::default());
    let mut bus = MmioBus::new();
    bus.register(
        BusRange::new(MmioAddress(MMIO_BASE), 0x1000).unwrap(),
        recorder.clone(),
    )
    .unwrap();

    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    vcpu.set_reg(Reg::x(3).unwrap(), 1).unwrap();
    vcpu.set_reg(Reg::x(4).unwrap(), u64::MAX).unwrap();
    loop {
        match expect_exception(vcpu.run().unwrap()) {
            ExceptionClass::DataAbort(abort) => handle_mmio(&mut vcpu, &bus, &abort),
            ExceptionClass::Hvc { imm: 0 } => break,
            other => panic!("unexpected exception {other:?}"),
        }
    }
    assert_eq!(
        *recorder.log.lock().unwrap(),
        [
            Access::Write {
                offset: 0x20,
                len: 8,
                value: 0
            },
            Access::Read {
                offset: 0x24,
                len: 4
            },
            Access::Read {
                offset: 0x28,
                len: 1
            },
            Access::Read {
                offset: 0x2a,
                len: 2
            },
        ]
    );
    // 0xef sign-extended to 64 bits; 0xbeef sign-extended to 32 bits, with
    // the upper half of x4 cleared.
    assert_eq!(
        vcpu.get_reg(Reg::x(3).unwrap()).unwrap(),
        0xffff_ffff_ffff_ffef
    );
    assert_eq!(vcpu.get_reg(Reg::x(4).unwrap()).unwrap(), 0xffff_beef);

    // An access outside the registered range is reported by the bus, and the
    // syndrome still describes it.
    let exception = Exception {
        syndrome: 0x9380_0046,
        virtual_address: MMIO_BASE + 0x1000,
        physical_address: MMIO_BASE + 0x1000,
    };
    let ExceptionClass::DataAbort(abort) = exception.decode() else {
        unreachable!()
    };
    assert!(bus.check_access(MmioAddress(abort.ipa), 4).is_err());
    drop(vcpu);
    drop(code);
}
