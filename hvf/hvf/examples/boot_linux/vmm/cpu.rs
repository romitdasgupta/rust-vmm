// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! vCPU threads and the exit loop.

use std::sync::{Arc, Barrier, Condvar, Mutex, PoisonError};
use std::time::Duration;

use hvf::{DataAbort, Exception, ExceptionClass, Reg, SysReg, Vcpu, VcpuExit};
use vm_device::bus::MmioAddress;
use vm_device::device_manager::MmioManager;

use super::psci::{self, Action};
use super::{Machine, Outcome, Result, Warning, sysreg};

/// `CPSR` at entry: EL1h with D, A, I and F masked, as Linux's boot
/// protocol requires.
const CPSR_EL1H_DAIF: u64 = 0x3c5;

// `CNTV_CTL_EL0` bits.
const CNTV_ENABLE: u64 = 1 << 0;
const CNTV_IMASK: u64 = 1 << 1;
const CNTV_ISTATUS: u64 = 1 << 2;

/// Longest sleep for a `WFI` exit, so that interrupts the in-kernel GIC
/// holds for the vCPU are taken without much delay even though the VMM
/// cannot see them.
const MAX_WFI_SLEEP: Duration = Duration::from_millis(10);

/// Hands out the turns in which the vCPU threads create their vCPUs.
///
/// The framework marks the redistributor of the most recently created vCPU
/// as the last one (`GICR_TYPER.Last`), and Linux stops scanning there.
/// Creating the vCPUs in index order keeps the last frame at the end.
#[derive(Debug, Default)]
pub struct CreationOrder {
    next: Mutex<usize>,
    turn: Condvar,
}

impl CreationOrder {
    fn wait_turn(&self, index: usize) {
        let mut next = self.next.lock().unwrap_or_else(PoisonError::into_inner);
        while *next != index {
            next = self.turn.wait(next).unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn done(&self) {
        *self.next.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.turn.notify_all();
    }
}

/// `ID_AA64MMFR0_EL1.PARange` for an IPA size in bits.
fn pa_range(ipa_bits: u32) -> Option<u64> {
    Some(match ipa_bits {
        32 => 0,
        36 => 1,
        40 => 2,
        42 => 3,
        44 => 4,
        48 => 5,
        52 => 6,
        _ => return None,
    })
}

/// Creates vCPU `index` on the calling thread.
fn create(machine: &Machine, index: usize) -> Result<Vcpu> {
    let mut vcpu = machine.vm.create_vcpu(None)?;
    vcpu.set_sys_reg(SysReg::MPIDR_EL1, psci::mpidr(index))?;
    // The framework reports the host's physical address size; tell the
    // guest the (smaller) IPA size of the VM instead, as QEMU does.
    if let Some(range) = pa_range(machine.vm.ipa_bits()) {
        let mmfr0 = vcpu.get_sys_reg(SysReg::ID_AA64MMFR0_EL1)?;
        vcpu.set_sys_reg(SysReg::ID_AA64MMFR0_EL1, (mmfr0 & !0xf) | range)?;
    }
    Ok(vcpu)
}

/// The body of vCPU thread `index`.
///
/// Every thread creates its vCPU, in index order, and then waits at
/// `barrier`, so that all redistributors exist before vCPU 0 runs. vCPU 0
/// then boots the kernel; the others wait for `CPU_ON`.
pub fn thread(
    machine: Arc<Machine>,
    index: usize,
    order: Arc<CreationOrder>,
    barrier: Arc<Barrier>,
) {
    order.wait_turn(index);
    let created = create(&machine, index);
    if let Ok(vcpu) = &created {
        machine.register_kicker(index, vcpu.kicker());
    }
    order.done();
    barrier.wait();

    let result = created.and_then(|mut vcpu| {
        machine.log_vcpu(index, &vcpu);
        lifecycle(&machine, index, &mut vcpu)
    });
    if let Err(e) = result {
        machine.fail(format!("vCPU {index}: {e}"));
    }
    // The vCPU is dropped here, on its own thread.
}

/// Runs vCPU `index` every time it is turned on, until the machine stops.
fn lifecycle(machine: &Machine, index: usize, vcpu: &mut Vcpu) -> Result<()> {
    let mut first = true;
    while let Some((entry, context)) = machine.cpus.wait_for_on(index) {
        machine.debug(format_args!(
            "vCPU {index}: on at {entry:#x}, x0 = {context:#x}"
        ));
        if !first {
            // A vCPU turned on again after CPU_OFF starts with the MMU off.
            vcpu.set_sys_reg(SysReg::SCTLR_EL1, 0)?;
        }
        first = false;
        vcpu.set_reg(Reg::PC, entry)?;
        vcpu.set_reg(Reg::CPSR, CPSR_EL1H_DAIF)?;
        for (n, value) in [context, 0, 0, 0].into_iter().enumerate() {
            vcpu.set_reg(Reg::x(n as u8).expect("x0-x3"), value)?;
        }
        match run(machine, index, vcpu)? {
            Stop::CpuOff => {
                machine.debug(format_args!("vCPU {index}: off"));
                machine.cpus.set_off(index);
            }
            Stop::Machine => break,
        }
    }
    Ok(())
}

/// Why [`run`] returned.
enum Stop {
    /// The vCPU called `CPU_OFF`.
    CpuOff,
    /// The machine is shutting down.
    Machine,
}

/// The exit loop of a vCPU that is on.
fn run(machine: &Machine, index: usize, vcpu: &mut Vcpu) -> Result<Stop> {
    // Set after a `VtimerActivated` exit, until the guest has handled the
    // timer interrupt (see the crate README).
    let mut vtimer_masked = false;
    loop {
        if machine.cpus.is_stopped() {
            return Ok(Stop::Machine);
        }
        if vtimer_masked {
            let ctl = vcpu.get_sys_reg(SysReg::CNTV_CTL_EL0)?;
            let firing = CNTV_ENABLE | CNTV_ISTATUS;
            if ctl & (firing | CNTV_IMASK) != firing {
                vcpu.set_vtimer_mask(false)?;
                vtimer_masked = false;
            }
        }

        let exception = match vcpu.run()? {
            // A kick: shutdown, checked above.
            VcpuExit::Canceled => continue,
            VcpuExit::Exception(exception) => exception,
            VcpuExit::VtimerActivated => {
                // Only without the in-kernel GIC, which delivers the timer
                // interrupt (PPI 27) itself.
                machine.warn_once(Warning::Vtimer, || {
                    "unexpected VtimerActivated exit".to_string()
                });
                vtimer_masked = true;
                continue;
            }
            other => return Err(format!("unexpected exit {other:?}").into()),
        };

        match exception.decode() {
            ExceptionClass::Hvc { .. } => {
                // The PC is already past the `hvc`.
                if let Some(stop) = psci_call(machine, index, vcpu)? {
                    return Ok(stop);
                }
            }
            ExceptionClass::Smc { .. } => {
                let stop = psci_call(machine, index, vcpu)?;
                vcpu.advance_pc()?;
                if let Some(stop) = stop {
                    return Ok(stop);
                }
            }
            ExceptionClass::SysReg(access) => sysreg::emulate(machine, vcpu, &access)?,
            ExceptionClass::DataAbort(abort) => mmio(machine, vcpu, &abort, &exception)?,
            ExceptionClass::Wfx { wfe } => {
                // With the in-kernel GIC the framework waits for interrupts
                // itself and `WFI` does not exit; this is a fallback.
                vcpu.advance_pc()?;
                if !wfe {
                    wait_for_interrupt(machine, index, vcpu)?;
                }
            }
            other => {
                return Err(
                    format!("unhandled exception {other:?}\n{}", dump(vcpu, &exception)).into(),
                );
            }
        }
    }
}

/// Handles a PSCI call. Returns `Some` when the vCPU stops running.
fn psci_call(machine: &Machine, index: usize, vcpu: &mut Vcpu) -> Result<Option<Stop>> {
    let mut x = [0u64; 4];
    for (n, value) in x.iter_mut().enumerate() {
        *value = vcpu.get_reg(Reg::x(n as u8).expect("x0-x3"))?;
    }
    let action = psci::handle(&machine.cpus, x);
    machine.debug(format_args!(
        "vCPU {index}: PSCI {:#x}({:#x}, {:#x}, {:#x}) -> {action:x?}",
        x[0], x[1], x[2], x[3]
    ));
    Ok(match action {
        Action::Return(value) => {
            vcpu.set_reg(Reg::x(0).expect("x0"), value)?;
            None
        }
        Action::CpuOff => Some(Stop::CpuOff),
        Action::SystemOff => {
            machine.shutdown(Outcome::PowerOff);
            Some(Stop::Machine)
        }
        Action::SystemReset => {
            machine.shutdown(Outcome::Reset);
            Some(Stop::Machine)
        }
    })
}

/// Emulates an MMIO access through the device bus.
fn mmio(
    machine: &Machine,
    vcpu: &mut Vcpu,
    abort: &DataAbort,
    exception: &Exception,
) -> Result<()> {
    let Some(access) = abort.syndrome else {
        return Err(format!(
            "data abort at {:#x} without an instruction syndrome\n{}",
            abort.ipa,
            dump(vcpu, exception)
        )
        .into());
    };
    let len = usize::from(access.len);
    let addr = MmioAddress(abort.ipa);
    // An access that no device claims fails before touching `bytes`: writes
    // are dropped and reads return zero.
    if abort.write {
        let value = vcpu.mmio_write_value(&access)?;
        if machine
            .bus
            .mmio_write(addr, &value.to_le_bytes()[..len])
            .is_err()
        {
            machine.warn_once(Warning::MmioWrite(abort.ipa), || {
                format!(
                    "ignored {len}-byte MMIO write of {value:#x} to {:#x}",
                    abort.ipa
                )
            });
        }
        vcpu.advance_pc()?;
    } else {
        let mut bytes = [0u8; 8];
        if machine.bus.mmio_read(addr, &mut bytes[..len]).is_err() {
            machine.warn_once(Warning::MmioRead(abort.ipa), || {
                format!("RAZ {len}-byte MMIO read from {:#x}", abort.ipa)
            });
        }
        vcpu.complete_mmio_read(&access, u64::from_le_bytes(bytes))?;
    }
    Ok(())
}

/// Sleeps after a `WFI` exit until the virtual timer is due, the machine
/// stops, or [`MAX_WFI_SLEEP`] passes.
fn wait_for_interrupt(machine: &Machine, index: usize, vcpu: &Vcpu) -> Result<()> {
    let ctl = vcpu.get_sys_reg(SysReg::CNTV_CTL_EL0)?;
    let mut sleep = MAX_WFI_SLEEP;
    if ctl & (CNTV_ENABLE | CNTV_IMASK) == CNTV_ENABLE {
        let cval = vcpu.get_sys_reg(SysReg::CNTV_CVAL_EL0)?;
        let now = super::time::mach_ticks().wrapping_sub(vcpu.vtimer_offset()?);
        if cval <= now {
            return Ok(());
        }
        sleep = sleep.min(super::time::ticks_to_duration(cval - now));
    }
    machine.cpus.idle(index, sleep);
    Ok(())
}

/// The registers that explain an unexpected exception.
fn dump(vcpu: &Vcpu, exception: &Exception) -> String {
    let reg = |r: Reg| vcpu.get_reg(r).unwrap_or(u64::MAX);
    let sys = |r: SysReg| vcpu.get_sys_reg(r).unwrap_or(u64::MAX);
    format!(
        "  ESR_EL2 {:#x} (EC {:#x}) FAR {:#x} IPA {:#x}\n  PC {:#x} CPSR {:#x} \
         ESR_EL1 {:#x} ELR_EL1 {:#x} FAR_EL1 {:#x}",
        exception.syndrome,
        exception.ec(),
        exception.virtual_address,
        exception.physical_address,
        reg(Reg::PC),
        reg(Reg::CPSR),
        sys(SysReg::ESR_EL1),
        sys(SysReg::ELR_EL1),
        sys(SysReg::FAR_EL1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pa_range() {
        assert_eq!(pa_range(36), Some(1));
        assert_eq!(pa_range(40), Some(2));
        assert_eq!(pa_range(41), None);
    }
}
