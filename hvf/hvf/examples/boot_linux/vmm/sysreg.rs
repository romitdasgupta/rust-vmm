// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Emulation of the system registers the framework traps.
//!
//! With the in-kernel GIC, the framework handles the GIC CPU interface
//! (`ICC_*`) and the timers (`CNTP_*`, `CNTV_*`) itself. What is left for a
//! Linux guest are the OS lock and debug communication channel registers,
//! which Linux touches on every CPU. Everything else reads as zero and
//! ignores writes (RAZ/WI), logged once per register and direction.

use hvf::{Reg, SysReg, SysRegAccess, Vcpu};

use super::{Machine, Result, Warning};

/// `OSLAR_EL1`: Linux writes 0 to clear the OS lock.
const OSLAR_EL1: SysReg = SysReg::from_encoding(2, 0, 1, 0, 4);
/// `OSLSR_EL1`.
const OSLSR_EL1: SysReg = SysReg::from_encoding(2, 0, 1, 1, 4);
/// `OSDLR_EL1`.
const OSDLR_EL1: SysReg = SysReg::from_encoding(2, 0, 1, 3, 4);
/// `MDCCSR_EL0`, read by the DCC console driver (`hvc_dcc`).
const MDCCSR_EL0: SysReg = SysReg::from_encoding(2, 3, 0, 1, 0);

/// `OSLSR_EL1` value: OS lock implemented (`OSLM = 0b10`) and unlocked.
const OSLSR_UNLOCKED: u64 = 0x8;

/// Formats a register as `S<op0>_<op1>_C<n>_C<m>_<op2>`.
pub fn name(reg: SysReg) -> String {
    let (op0, op1, crn, crm, op2) = reg.encoding();
    format!("S{op0}_{op1}_C{crn}_C{crm}_{op2}")
}

/// Emulates a trapped `MRS`/`MSR` and advances the PC past it.
pub fn emulate(machine: &Machine, vcpu: &mut Vcpu, access: &SysRegAccess) -> Result<()> {
    // Rt = 31 is XZR: reads are discarded, writes store zero.
    let rt = Reg::x(access.rt);
    if access.read {
        let value = match access.reg {
            OSLSR_EL1 => OSLSR_UNLOCKED,
            OSDLR_EL1 | MDCCSR_EL0 => 0,
            SysReg::MDCCINT_EL1 => vcpu.get_sys_reg(SysReg::MDCCINT_EL1)?,
            reg => {
                machine.warn_once(Warning::SysRegRead(reg), || {
                    format!("RAZ read of {}", name(reg))
                });
                0
            }
        };
        if let Some(rt) = rt {
            vcpu.set_reg(rt, value)?;
        }
    } else {
        let value = match rt {
            Some(rt) => vcpu.get_reg(rt)?,
            None => 0,
        };
        match access.reg {
            OSLAR_EL1 | OSDLR_EL1 => {}
            SysReg::MDCCINT_EL1 => vcpu.set_sys_reg(SysReg::MDCCINT_EL1, value)?,
            reg => machine.warn_once(Warning::SysRegWrite(reg), || {
                format!("ignored write of {value:#x} to {}", name(reg))
            }),
        }
    }
    vcpu.advance_pc()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(name(OSLAR_EL1), "S2_0_C1_C0_4");
        assert_eq!(name(MDCCSR_EL0), "S2_3_C0_C1_0");
    }
}
