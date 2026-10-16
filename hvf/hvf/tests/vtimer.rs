// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Virtual timer exits and masking.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.
//!
//! Without an in-kernel GIC, the framework reports a firing virtual timer as
//! a `VtimerActivated` exit and masks the timer, so that it does not fire
//! again. The VMM then raises the timer interrupt (PPI 27) in its interrupt
//! controller model and unmasks the timer once the guest has handled it.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use std::time::Duration;

use common::{GUEST_IPA, GuestRam, SPIN, boot_vcpu, hv_guard, kick_after, mach_time};
use hvf::{Hvf, SysReg, Vcpu, VcpuExit, VmConfig};

/// `CNTV_CTL_EL0.ENABLE`
const CTL_ENABLE: u64 = 1;
/// `CNTV_CTL_EL0.ISTATUS`
const CTL_ISTATUS: u64 = 1 << 2;

fn run_until_kicked(vcpu: &mut Vcpu) -> VcpuExit {
    let kicked = kick_after(vcpu.kicker(), Duration::from_millis(50));
    let exit = vcpu.run().unwrap();
    kicked.recv().unwrap().unwrap();
    exit
}

#[test]
fn test_vtimer_exit_and_mask() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    assert!(!vcpu.vtimer_mask().unwrap());

    // A compare value in the past fires as soon as the timer is enabled.
    vcpu.set_sys_reg(SysReg::CNTV_CVAL_EL0, 0).unwrap();
    vcpu.set_sys_reg(SysReg::CNTV_CTL_EL0, CTL_ENABLE).unwrap();
    assert_eq!(vcpu.run().unwrap(), VcpuExit::VtimerActivated);
    // The framework masked the timer, and the timer condition holds.
    assert!(vcpu.vtimer_mask().unwrap());
    let ctl = vcpu.get_sys_reg(SysReg::CNTV_CTL_EL0).unwrap();
    assert_eq!(ctl & (CTL_ENABLE | CTL_ISTATUS), CTL_ENABLE | CTL_ISTATUS);

    // While masked, the still-pending timer does not exit again.
    assert_eq!(run_until_kicked(&mut vcpu), VcpuExit::Canceled);

    // The guest handled the interrupt and disabled the timer; the VMM
    // unmasks it. Nothing fires any more.
    vcpu.set_sys_reg(SysReg::CNTV_CTL_EL0, 0).unwrap();
    vcpu.set_vtimer_mask(false).unwrap();
    assert!(!vcpu.vtimer_mask().unwrap());
    assert_eq!(run_until_kicked(&mut vcpu), VcpuExit::Canceled);
}

#[test]
fn test_vtimer_unmask_refires() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);

    vcpu.set_sys_reg(SysReg::CNTV_CVAL_EL0, 0).unwrap();
    vcpu.set_sys_reg(SysReg::CNTV_CTL_EL0, CTL_ENABLE).unwrap();
    assert_eq!(vcpu.run().unwrap(), VcpuExit::VtimerActivated);
    // Unmasking while the timer condition still holds fires it again.
    vcpu.set_vtimer_mask(false).unwrap();
    assert_eq!(vcpu.run().unwrap(), VcpuExit::VtimerActivated);
    assert!(vcpu.vtimer_mask().unwrap());

    // A timer masked in CNTV_CTL_EL0 (IMASK) by the guest does not exit.
    vcpu.set_sys_reg(SysReg::CNTV_CTL_EL0, CTL_ENABLE | 0b10)
        .unwrap();
    vcpu.set_vtimer_mask(false).unwrap();
    assert_eq!(run_until_kicked(&mut vcpu), VcpuExit::Canceled);
}

/// A timer programmed in the future fires once the virtual counter reaches
/// the compare value.
#[test]
fn test_vtimer_fires_in_the_future() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);

    // Start the virtual counter at 0, and fire after 10 ms.
    let start = mach_time::now();
    vcpu.set_vtimer_offset(start).unwrap();
    let ticks = mach_time::from_nanos(10_000_000);
    vcpu.set_sys_reg(SysReg::CNTV_CVAL_EL0, ticks).unwrap();
    vcpu.set_sys_reg(SysReg::CNTV_CTL_EL0, CTL_ENABLE).unwrap();

    // A watchdog kick well after the deadline; the timer must fire first.
    // If it runs after the vCPU is gone, it does nothing.
    let _watchdog = kick_after(vcpu.kicker(), Duration::from_secs(2));
    assert_eq!(vcpu.run().unwrap(), VcpuExit::VtimerActivated);
    let elapsed = mach_time::now() - start;
    assert!(elapsed >= ticks, "fired after {elapsed} < {ticks} ticks");
}
