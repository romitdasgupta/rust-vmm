// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! IRQ line injection without the in-kernel GIC.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.
//!
//! Without an in-kernel GIC, the VMM drives the vCPU's IRQ and FIQ lines with
//! `Vcpu::set_pending_interrupt`. The framework clears the pending state after
//! every `Vcpu::run`, so a level-triggered line has to be latched by the VMM
//! and re-armed before each run. These tests document that behaviour and the
//! re-arm pattern.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use std::time::Duration;

use common::{
    ERET, GUEST_IPA, GuestRam, SPIN, boot_vcpu, expect_exception, hv_guard, hvc, kick_after,
};
use hvf::{ExceptionClass, Hvf, InterruptType, Reg, SysReg, Vcpu, VcpuExit, VmConfig};

/// Offset of the vector table in the guest image (2 KiB aligned).
const VECTORS: usize = 0x800;
/// Offset of the "IRQ, current EL with SP_ELx" vector.
const IRQ_VECTOR: usize = VECTORS + 0x280;
/// Offset of the "FIQ, current EL with SP_ELx" vector.
const FIQ_VECTOR: usize = VECTORS + 0x300;
/// Offset of the spin loop the guest idles in.
const SPIN_LOOP: u64 = 0x14;

/// Loads the guest:
///
/// ```text
/// 0x000: adr  x0, vectors       ; 0x800
///        msr  vbar_el1, x0
///        isb
///        msr  daifclr, #3       ; unmask IRQ and FIQ
///        hvc  #0                ; "ready"
/// 0x014: b    .
/// 0xa80: hvc  #1                ; IRQ handler
///        eret
/// 0xb00: hvc  #2                ; FIQ handler
///        eret
/// ```
fn load_guest(vm: &hvf::Vm) -> GuestRam {
    let ram = GuestRam::with_code(
        vm,
        &[
            0x1000_4000,
            0xd518_c000,
            0xd503_3fdf,
            0xd503_43ff,
            hvc(0),
            SPIN,
        ],
    );
    ram.write_u32s(IRQ_VECTOR, &[hvc(1), ERET]);
    ram.write_u32s(FIQ_VECTOR, &[hvc(2), ERET]);
    ram
}

/// Runs the guest up to its "ready" call, with IRQ and FIQ unmasked.
fn ready_vcpu(vm: &hvf::Vm) -> Vcpu {
    let mut vcpu = boot_vcpu(vm, GUEST_IPA);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(
        vcpu.get_sys_reg(SysReg::VBAR_EL1).unwrap(),
        GUEST_IPA + VECTORS as u64
    );
    // PSTATE.I and PSTATE.F are clear.
    assert_eq!(vcpu.get_reg(Reg::CPSR).unwrap() & 0xc0, 0);
    vcpu
}

/// Runs a vCPU that is expected to spin until kicked.
fn run_until_kicked(vcpu: &mut Vcpu) -> VcpuExit {
    let kicked = kick_after(vcpu.kicker(), Duration::from_millis(50));
    let exit = vcpu.run().unwrap();
    kicked.recv().unwrap().unwrap();
    exit
}

#[test]
fn test_pending_state_is_cleared_by_run() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = load_guest(&vm);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);

    vcpu.set_pending_interrupt(InterruptType::Irq, true)
        .unwrap();
    assert!(vcpu.pending_interrupt(InterruptType::Irq).unwrap());
    assert!(!vcpu.pending_interrupt(InterruptType::Fiq).unwrap());
    // The line stays asserted during the run: the guest starts with IRQs
    // masked (CPSR 0x3c5) and takes the interrupt as soon as it unmasks
    // them, before its "ready" call at offset 0x10.
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 1 }
    );
    assert_eq!(vcpu.get_sys_reg(SysReg::ELR_EL1).unwrap(), GUEST_IPA + 0x10);
    // The run cleared the pending state...
    assert!(!vcpu.pending_interrupt(InterruptType::Irq).unwrap());
    // ...so after the handler returns, the guest is not interrupted again.
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(run_until_kicked(&mut vcpu), VcpuExit::Canceled);
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), GUEST_IPA + SPIN_LOOP);

    // Clearing a line that is not pending is harmless.
    vcpu.set_pending_interrupt(InterruptType::Irq, false)
        .unwrap();
    assert!(!vcpu.pending_interrupt(InterruptType::Irq).unwrap());
}

#[test]
fn test_irq_and_fiq_delivery() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = load_guest(&vm);
    let mut vcpu = ready_vcpu(&vm);

    vcpu.set_pending_interrupt(InterruptType::Irq, true)
        .unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 1 }
    );
    // The guest took the IRQ while spinning, through its vector table.
    assert_eq!(
        vcpu.get_sys_reg(SysReg::ELR_EL1).unwrap(),
        GUEST_IPA + SPIN_LOOP
    );
    // The handler runs with interrupts masked.
    assert_eq!(vcpu.get_reg(Reg::CPSR).unwrap() & 0xc0, 0xc0);

    // Returning from the handler with the line low goes back to spinning.
    assert_eq!(run_until_kicked(&mut vcpu), VcpuExit::Canceled);
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), GUEST_IPA + SPIN_LOOP);

    vcpu.set_pending_interrupt(InterruptType::Fiq, true)
        .unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 2 }
    );
}

/// The pattern for a level-triggered interrupt line: the VMM latches the
/// line level and re-arms it before every run, until the device lowers it.
#[test]
fn test_level_irq_rearmed_before_each_run() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = load_guest(&vm);
    let mut vcpu = ready_vcpu(&vm);

    // The device raises its line.
    let mut irq_level = true;
    let mut irqs_taken = 0;
    loop {
        // Re-arm the latched level before every run.
        vcpu.set_pending_interrupt(InterruptType::Irq, irq_level)
            .unwrap();
        let exit = if irq_level {
            vcpu.run().unwrap()
        } else {
            run_until_kicked(&mut vcpu)
        };
        match exit {
            VcpuExit::Exception(e) => match e.decode() {
                ExceptionClass::Hvc { imm: 1 } => {
                    irqs_taken += 1;
                    // The device lowers its line after the guest handled it
                    // three times.
                    if irqs_taken == 3 {
                        irq_level = false;
                    }
                }
                other => panic!("unexpected exception {other:?}"),
            },
            VcpuExit::Canceled if !irq_level => break,
            other => panic!("unexpected exit {other:?}"),
        }
    }
    // A level held across runs interrupted the guest after every `eret`.
    assert_eq!(irqs_taken, 3);
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), GUEST_IPA + SPIN_LOOP);
}
