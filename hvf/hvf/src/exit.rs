// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! vCPU exits and exception syndrome decoding.
//!
//! Everything here is pure data and arithmetic on `ESR_EL2` values, so it
//! builds and is tested on every target.

use hvf_bindings::{
    HV_EXIT_REASON_CANCELED, HV_EXIT_REASON_EXCEPTION, HV_EXIT_REASON_VTIMER_ACTIVATED,
    hv_vcpu_exit_t,
};

use crate::regs::SysReg;

// Exception classes (`ESR_ELx.EC`) decoded by `Exception::decode`.
const EC_WFX: u8 = 0x01;
const EC_HVC64: u8 = 0x16;
const EC_SMC64: u8 = 0x17;
const EC_SYS64: u8 = 0x18;
const EC_IABT_LOW: u8 = 0x20;
const EC_DABT_LOW: u8 = 0x24;
const EC_BREAKPOINT_LOW: u8 = 0x30;
const EC_SOFTWARE_STEP_LOW: u8 = 0x32;
const EC_WATCHPOINT_LOW: u8 = 0x34;
const EC_BRK64: u8 = 0x3c;

/// Why `Vcpu::run` returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VcpuExit {
    /// The run was cancelled by a `VcpuKicker`, either
    /// while running or by a kick that happened before the run started.
    Canceled,
    /// The guest took a synchronous exception to the hypervisor.
    Exception(Exception),
    /// The virtual timer fired. The framework masked it; unmask it with
    /// `Vcpu::set_vtimer_mask(false)` once the guest has handled the
    /// interrupt.
    VtimerActivated,
    /// The framework reported an exit reason this crate does not know.
    Unknown(u32),
}

impl VcpuExit {
    /// Converts the exit information written by `hv_vcpu_run`.
    pub fn from_raw(raw: &hv_vcpu_exit_t) -> VcpuExit {
        match raw.reason {
            HV_EXIT_REASON_CANCELED => VcpuExit::Canceled,
            HV_EXIT_REASON_EXCEPTION => VcpuExit::Exception(Exception {
                syndrome: raw.exception.syndrome,
                virtual_address: raw.exception.virtual_address,
                physical_address: raw.exception.physical_address,
            }),
            HV_EXIT_REASON_VTIMER_ACTIVATED => VcpuExit::VtimerActivated,
            reason => VcpuExit::Unknown(reason),
        }
    }
}

/// A synchronous exception taken by the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exception {
    /// The exception syndrome (`ESR_EL2`).
    pub syndrome: u64,
    /// The faulting virtual address (`FAR_EL2`), when meaningful.
    pub virtual_address: u64,
    /// The faulting guest physical address, when meaningful.
    pub physical_address: u64,
}

impl Exception {
    /// The exception class, `ESR_EL2.EC` (bits 31:26).
    pub fn ec(&self) -> u8 {
        ((self.syndrome >> 26) & 0x3f) as u8
    }

    /// The instruction length bit, `ESR_EL2.IL` (bit 25): `true` for a
    /// 32-bit instruction.
    pub fn il(&self) -> bool {
        self.syndrome & (1 << 25) != 0
    }

    /// The instruction specific syndrome, `ESR_EL2.ISS` (bits 24:0).
    pub fn iss(&self) -> u32 {
        (self.syndrome & 0x1ff_ffff) as u32
    }

    /// Decodes the syndrome.
    pub fn decode(&self) -> ExceptionClass {
        let iss = self.iss();
        // The immediate of HVC, SMC and BRK is ISS[15:0].
        let imm16 = (iss & 0xffff) as u16;
        match self.ec() {
            // ISS.TI[1:0]: 00 WFI, 01 WFE, 10 WFIT, 11 WFET. For WFIT and
            // WFET, ISS.RV[2] says whether ISS.RN[9:5] names the register
            // holding the deadline.
            EC_WFX if iss & 0b10 == 0 => ExceptionClass::Wfx { wfe: iss & 1 != 0 },
            EC_WFX => ExceptionClass::WfxTimeout {
                wfe: iss & 1 != 0,
                timeout_reg: (iss & 0b100 != 0).then_some(((iss >> 5) & 0x1f) as u8),
            },
            EC_HVC64 => ExceptionClass::Hvc { imm: imm16 },
            EC_SMC64 => ExceptionClass::Smc { imm: imm16 },
            EC_SYS64 => ExceptionClass::SysReg(SysRegAccess::from_iss(iss)),
            EC_IABT_LOW => ExceptionClass::InstructionAbort {
                ipa: self.physical_address,
                va: self.virtual_address,
                ifsc: (iss & 0x3f) as u8,
            },
            EC_DABT_LOW => ExceptionClass::DataAbort(DataAbort::from_iss(
                iss,
                self.physical_address,
                self.virtual_address,
            )),
            EC_BREAKPOINT_LOW => ExceptionClass::Breakpoint,
            EC_SOFTWARE_STEP_LOW => ExceptionClass::SoftwareStep,
            EC_WATCHPOINT_LOW => ExceptionClass::Watchpoint {
                va: self.virtual_address,
            },
            EC_BRK64 => ExceptionClass::Brk { imm: imm16 },
            ec => ExceptionClass::Other { ec, iss },
        }
    }
}

/// A decoded exception.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExceptionClass {
    /// `WFI` or `WFE` (EC 0x01). The PC points at the instruction.
    Wfx {
        /// `true` for `WFE`, `false` for `WFI`.
        wfe: bool,
    },
    /// `WFIT` or `WFET` (EC 0x01, `FEAT_WFxT`): wait with a deadline. The
    /// PC points at the instruction.
    ///
    /// The guest should wake by the deadline, the value of the
    /// `timeout_reg` general purpose register compared against
    /// `CNTVCT_EL0`, even without an interrupt. The framework hides
    /// `FEAT_WFxT` in `ID_AA64ISAR2_EL1` by default, but a guest that uses
    /// the instructions anyway traps here when the VM has no in-kernel GIC.
    /// With the in-kernel GIC they do not exit, and the framework was
    /// observed to ignore the deadline, so do not advertise `FEAT_WFxT` to
    /// such a guest.
    WfxTimeout {
        /// `true` for `WFET`, `false` for `WFIT`.
        wfe: bool,
        /// The number `n` of the register `Xn` holding the deadline, or
        /// `None` if the syndrome does not report it (`ISS.RV == 0`).
        timeout_reg: Option<u8>,
    },
    /// `HVC` (EC 0x16). The PC already points at the next instruction.
    Hvc {
        /// The immediate operand.
        imm: u16,
    },
    /// `SMC` (EC 0x17). The PC points at the instruction.
    Smc {
        /// The immediate operand.
        imm: u16,
    },
    /// Trapped `MSR`, `MRS` or system instruction (EC 0x18). The PC points at
    /// the instruction.
    SysReg(SysRegAccess),
    /// Instruction abort from the guest (EC 0x20).
    InstructionAbort {
        /// The faulting guest physical address.
        ipa: u64,
        /// The faulting virtual address.
        va: u64,
        /// The instruction fault status code.
        ifsc: u8,
    },
    /// Data abort from the guest (EC 0x24), for example an MMIO access. The
    /// PC points at the instruction.
    DataAbort(DataAbort),
    /// Breakpoint from the guest (EC 0x30).
    Breakpoint,
    /// Software step from the guest (EC 0x32).
    SoftwareStep,
    /// Watchpoint from the guest (EC 0x34).
    Watchpoint {
        /// The faulting virtual address.
        va: u64,
    },
    /// `BRK` instruction (EC 0x3c).
    Brk {
        /// The immediate operand.
        imm: u16,
    },
    /// Any other exception class.
    Other {
        /// The exception class.
        ec: u8,
        /// The instruction specific syndrome.
        iss: u32,
    },
}

impl ExceptionClass {
    /// Whether the hardware already advanced the PC past the trapping
    /// instruction. Only true for [`ExceptionClass::Hvc`]; for every other
    /// class the VMM advances the PC itself after emulation.
    pub fn pc_advanced_by_hw(&self) -> bool {
        matches!(self, ExceptionClass::Hvc { .. })
    }
}

/// A decoded data abort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataAbort {
    /// The faulting guest physical address.
    pub ipa: u64,
    /// The faulting virtual address.
    pub va: u64,
    /// `true` for a write (`WnR`).
    pub write: bool,
    /// The data fault status code.
    pub dfsc: u8,
    /// The fault came from a cache maintenance instruction (`CM`).
    pub cache_maint: bool,
    /// The fault happened on a stage 1 translation table walk (`S1PTW`).
    pub s1ptw: bool,
    /// The access details, or `None` if the syndrome does not describe the
    /// instruction (`ISV == 0`), in which case the VMM must decode it.
    pub syndrome: Option<DataAbortSyndrome>,
}

impl DataAbort {
    /// Decodes the ISS of a data abort (EC 0x24): `ISV[24] SAS[23:22]
    /// SSE[21] SRT[20:16] SF[15] AR[14] CM[8] S1PTW[7] WnR[6] DFSC[5:0]`.
    fn from_iss(iss: u32, ipa: u64, va: u64) -> DataAbort {
        let bit = |n: u32| iss & (1 << n) != 0;
        // ISS[23:14] are only valid when ISV is set.
        let syndrome = bit(24).then(|| DataAbortSyndrome {
            len: 1 << ((iss >> 22) & 0x3),
            srt: ((iss >> 16) & 0x1f) as u8,
            sign_extend: bit(21),
            sf64: bit(15),
            acquire_release: bit(14),
        });
        DataAbort {
            ipa,
            va,
            write: bit(6),
            dfsc: (iss & 0x3f) as u8,
            cache_maint: bit(8),
            s1ptw: bit(7),
            syndrome,
        }
    }

    /// Whether this is a translation fault (`(dfsc & 0x3c) == 0x04`), as for
    /// an access to unmapped (MMIO) memory. A forbidden access to mapped
    /// memory the guest has not touched yet reports one too (see
    /// `Vm::protect`), so check the IPA against the guest memory map before
    /// treating the abort as MMIO.
    pub fn is_translation_fault(&self) -> bool {
        (self.dfsc & 0x3c) == 0x04
    }

    /// Whether this is a permission fault (`(dfsc & 0x3c) == 0x0c`), as for a
    /// write to read-only memory the guest has already accessed. The first
    /// access to such a page can report a translation fault instead.
    pub fn is_permission_fault(&self) -> bool {
        (self.dfsc & 0x3c) == 0x0c
    }
}

/// The instruction syndrome of a data abort (valid when `ISV == 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataAbortSyndrome {
    /// Access size in bytes: 1, 2, 4 or 8.
    pub len: u8,
    /// The transfer register number (`SRT`); 31 means `XZR`.
    pub srt: u8,
    /// A load must be sign-extended (`SSE`).
    pub sign_extend: bool,
    /// The transfer register is 64-bit (`SF`).
    pub sf64: bool,
    /// The access has acquire/release semantics (`AR`).
    pub acquire_release: bool,
}

/// A trapped system register access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysRegAccess {
    /// The accessed register.
    pub reg: SysReg,
    /// The transfer register number; 31 means `XZR`.
    pub rt: u8,
    /// `true` for a read (`MRS`), `false` for a write (`MSR`).
    pub read: bool,
}

impl SysRegAccess {
    /// Decodes the ISS of a trapped `MSR`/`MRS`/system instruction (EC 0x18):
    /// `Op0[21:20] Op2[19:17] Op1[16:14] CRn[13:10] Rt[9:5] CRm[4:1]
    /// Direction[0]`.
    fn from_iss(iss: u32) -> SysRegAccess {
        let field = |shift: u32, mask: u32| ((iss >> shift) & mask) as u8;
        SysRegAccess {
            reg: SysReg::from_encoding(
                field(20, 0x3),
                field(14, 0x7),
                field(10, 0xf),
                field(1, 0xf),
                field(17, 0x7),
            ),
            rt: field(5, 0x1f),
            read: iss & 1 != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use hvf_bindings::{HV_EXIT_REASON_UNKNOWN, hv_vcpu_exit_exception_t};

    use super::*;

    const MMIO_IPA: u64 = 0x0900_0000;

    fn exception(syndrome: u64) -> Exception {
        Exception {
            syndrome,
            virtual_address: MMIO_IPA,
            physical_address: MMIO_IPA,
        }
    }

    fn data_abort(syndrome: u64) -> DataAbort {
        match exception(syndrome).decode() {
            ExceptionClass::DataAbort(abort) => abort,
            other => panic!("{syndrome:#x} decoded as {other:?}"),
        }
    }

    fn syndrome(len: u8, srt: u8, sign_extend: bool, sf64: bool) -> DataAbortSyndrome {
        DataAbortSyndrome {
            len,
            srt,
            sign_extend,
            sf64,
            acquire_release: false,
        }
    }

    /// Builds the ESR of a trapped system register access (EC 0x18).
    fn sysreg_esr(reg: SysReg, rt: u8, read: bool) -> u64 {
        let (op0, op1, crn, crm, op2) = reg.encoding();
        let iss = (u32::from(op0) << 20)
            | (u32::from(op2) << 17)
            | (u32::from(op1) << 14)
            | (u32::from(crn) << 10)
            | (u32::from(rt) << 5)
            | (u32::from(crm) << 1)
            | u32::from(read);
        (0x18 << 26) | (1 << 25) | u64::from(iss)
    }

    #[test]
    fn test_fields() {
        let e = exception(0x93c1_8046);
        assert_eq!(e.ec(), 0x24);
        assert!(e.il());
        assert_eq!(e.iss(), 0x1c1_8046);

        // ESR_EL2 bits 63:32 (ISS2) do not leak into the fields.
        let e = exception(0xffff_ffff_0000_0000 | 0x5a00_0000);
        assert_eq!(e.ec(), 0x16);
        assert!(e.il());
        assert_eq!(e.iss(), 0);

        // A 16-bit instruction (IL = 0).
        let e = exception(0x0400_0000);
        assert_eq!(e.ec(), 0x01);
        assert!(!e.il());
    }

    #[test]
    fn test_mmio_guest_vectors() {
        // Exits of the MMIO test guest, as observed on Hypervisor.framework.

        // `str w0, [x1]`
        let a = data_abort(0x9380_0046);
        assert_eq!(
            a,
            DataAbort {
                ipa: MMIO_IPA,
                va: MMIO_IPA,
                write: true,
                dfsc: 0x06,
                cache_maint: false,
                s1ptw: false,
                syndrome: Some(syndrome(4, 0, false, false)),
            }
        );
        assert!(a.is_translation_fault());
        assert!(!a.is_permission_fault());

        // `ldr w2, [x1, #4]`
        let a = data_abort(0x9382_0006);
        assert!(!a.write);
        assert_eq!(a.dfsc, 0x06);
        assert_eq!(a.syndrome, Some(syndrome(4, 2, false, false)));
        assert!(a.is_translation_fault());

        // `strb w0, [x1, #8]`
        let a = data_abort(0x9300_0046);
        assert!(a.write);
        assert_eq!(a.syndrome, Some(syndrome(1, 0, false, false)));

        // `str x2, [x1, #16]`
        let a = data_abort(0x93c2_8046);
        assert!(a.write);
        assert_eq!(a.syndrome, Some(syndrome(8, 2, false, true)));

        // `str x1, [x0]`
        let a = data_abort(0x93c1_8046);
        assert!(a.write);
        assert_eq!(a.syndrome, Some(syndrome(8, 1, false, true)));

        // `hvc #0`: the only class where the PC has already moved on.
        let c = exception(0x5a00_0000).decode();
        assert_eq!(c, ExceptionClass::Hvc { imm: 0 });
        assert!(c.pc_advanced_by_hw());

        // `wfi`
        let c = exception(0x07e0_0000).decode();
        assert_eq!(c, ExceptionClass::Wfx { wfe: false });
        assert!(!c.pc_advanced_by_hw());
    }

    #[test]
    fn test_other_vectors() {
        // `smc #0`
        let c = exception(0x5e00_0000).decode();
        assert_eq!(c, ExceptionClass::Smc { imm: 0 });
        assert!(!c.pc_advanced_by_hw());

        // Immediates.
        assert_eq!(
            exception(0x5a00_0000 | 0xabcd).decode(),
            ExceptionClass::Hvc { imm: 0xabcd }
        );
        assert_eq!(
            exception(0x5e00_0000 | 0x1).decode(),
            ExceptionClass::Smc { imm: 1 }
        );
        assert_eq!(
            exception(0xf200_0000 | 0x3e8).decode(),
            ExceptionClass::Brk { imm: 1000 }
        );

        // `wfe`
        assert_eq!(
            exception(0x07e0_0001).decode(),
            ExceptionClass::Wfx { wfe: true }
        );

        // `wfit x5`, as trapped by Hypervisor.framework on an M5 Pro:
        // TI = 0b10, RV = 1, RN = 5. It must not decode as a plain `wfi`.
        let c = exception(0x07e0_00a6).decode();
        assert_eq!(
            c,
            ExceptionClass::WfxTimeout {
                wfe: false,
                timeout_reg: Some(5),
            }
        );
        assert!(!c.pc_advanced_by_hw());
        // `wfit x0`.
        assert_eq!(
            exception(0x07e0_0006).decode(),
            ExceptionClass::WfxTimeout {
                wfe: false,
                timeout_reg: Some(0),
            }
        );
        // `wfet x30`.
        assert_eq!(
            exception(0x07e0_03c7).decode(),
            ExceptionClass::WfxTimeout {
                wfe: true,
                timeout_reg: Some(30),
            }
        );
        // TI = WFIT without a valid register (RV = 0).
        assert_eq!(
            exception(0x07e0_00a2).decode(),
            ExceptionClass::WfxTimeout {
                wfe: false,
                timeout_reg: None,
            }
        );

        // Instruction aborts: level 2 and level 3 translation faults.
        let e = Exception {
            syndrome: 0x8200_0006,
            virtual_address: 0x4000_0000,
            physical_address: 0x4000_1000,
        };
        assert_eq!(
            e.decode(),
            ExceptionClass::InstructionAbort {
                ipa: 0x4000_1000,
                va: 0x4000_0000,
                ifsc: 0x06,
            }
        );
        assert_eq!(
            exception(0x8200_0007).decode(),
            ExceptionClass::InstructionAbort {
                ipa: MMIO_IPA,
                va: MMIO_IPA,
                ifsc: 0x07,
            }
        );
        assert!(!exception(0x8200_0007).decode().pc_advanced_by_hw());

        // Debug exceptions.
        assert_eq!(
            exception(0x30 << 26 | 1 << 25).decode(),
            ExceptionClass::Breakpoint
        );
        assert_eq!(
            exception(0x32 << 26 | 1 << 25).decode(),
            ExceptionClass::SoftwareStep
        );
        assert_eq!(
            exception(0x34 << 26 | 1 << 25).decode(),
            ExceptionClass::Watchpoint { va: MMIO_IPA }
        );

        // Unknown classes keep the raw values.
        assert_eq!(
            exception(0x0000_0000).decode(),
            ExceptionClass::Other { ec: 0, iss: 0 }
        );
        assert_eq!(
            exception(0x07 << 26 | 1 << 25 | 0x1e0_0000).decode(),
            ExceptionClass::Other {
                ec: 0x07,
                iss: 0x1e0_0000
            }
        );
    }

    #[test]
    fn test_sysreg_trap() {
        for (reg, rt, read) in [
            (SysReg::MPIDR_EL1, 3, true),
            (SysReg::CNTV_CTL_EL0, 0, false),
            (SysReg::CNTV_CVAL_EL0, 30, false),
            (SysReg::MDSCR_EL1, 31, true),
        ] {
            let esr = sysreg_esr(reg, rt, read);
            let c = exception(esr).decode();
            assert_eq!(c, ExceptionClass::SysReg(SysRegAccess { reg, rt, read }));
            assert!(!c.pc_advanced_by_hw());
        }

        // `mrs x0, mpidr_el1`, encoded by hand from the Arm ARM layout.
        assert_eq!(sysreg_esr(SysReg::MPIDR_EL1, 0, true), 0x623a_0001);
        // `msr cntv_ctl_el0, x1`
        assert_eq!(sysreg_esr(SysReg::CNTV_CTL_EL0, 1, false), 0x6232_f826);
    }

    #[test]
    fn test_data_abort_without_syndrome() {
        // ISV = 0 (for example a load/store pair): the remaining fields are
        // still decoded, the syndrome is not.
        let a = data_abort(0x9200_0046);
        assert_eq!(a.syndrome, None);
        assert!(a.write);
        assert!(a.is_translation_fault());

        // Bits 23:14 must be ignored when ISV is clear.
        let a = data_abort(0x92ff_c007);
        assert_eq!(a.syndrome, None);
        assert!(!a.write);
        assert_eq!(a.dfsc, 0x07);
    }

    #[test]
    fn test_data_abort_flags() {
        // SSE, AR, CM and S1PTW.
        let a = data_abort(0x9000_0000 | 1 << 24 | 1 << 22 | 1 << 21 | 5 << 16 | 1 << 14);
        assert_eq!(
            a.syndrome,
            Some(DataAbortSyndrome {
                len: 2,
                srt: 5,
                sign_extend: true,
                sf64: false,
                acquire_release: true,
            })
        );
        assert!(!a.cache_maint);
        assert!(!a.s1ptw);

        let a = data_abort(0x9200_0000 | 1 << 8 | 1 << 7 | 0x0f);
        assert!(a.cache_maint);
        assert!(a.s1ptw);
        assert!(a.is_permission_fault());
        assert!(!a.is_translation_fault());

        // Translation and permission faults at every level.
        for level in 0..4 {
            let a = data_abort(0x9200_0004 | level);
            assert!(a.is_translation_fault());
            assert!(!a.is_permission_fault());
            let a = data_abort(0x9200_000c | level);
            assert!(a.is_permission_fault());
            assert!(!a.is_translation_fault());
        }
        // Access flag fault.
        let a = data_abort(0x9200_000b);
        assert!(!a.is_translation_fault());
        assert!(!a.is_permission_fault());
    }

    #[test]
    fn test_from_raw() {
        let raw = hv_vcpu_exit_t {
            reason: HV_EXIT_REASON_CANCELED,
            exception: hv_vcpu_exit_exception_t::default(),
        };
        assert_eq!(VcpuExit::from_raw(&raw), VcpuExit::Canceled);

        let raw = hv_vcpu_exit_t {
            reason: HV_EXIT_REASON_VTIMER_ACTIVATED,
            exception: hv_vcpu_exit_exception_t::default(),
        };
        assert_eq!(VcpuExit::from_raw(&raw), VcpuExit::VtimerActivated);

        let raw = hv_vcpu_exit_t {
            reason: HV_EXIT_REASON_EXCEPTION,
            exception: hv_vcpu_exit_exception_t {
                syndrome: 0x9380_0046,
                virtual_address: 0x1234_5000,
                physical_address: MMIO_IPA,
            },
        };
        let exit = VcpuExit::from_raw(&raw);
        assert_eq!(
            exit,
            VcpuExit::Exception(Exception {
                syndrome: 0x9380_0046,
                virtual_address: 0x1234_5000,
                physical_address: MMIO_IPA,
            })
        );
        let VcpuExit::Exception(e) = exit else {
            unreachable!()
        };
        let ExceptionClass::DataAbort(a) = e.decode() else {
            panic!("not a data abort");
        };
        assert_eq!(a.ipa, MMIO_IPA);
        assert_eq!(a.va, 0x1234_5000);

        let raw = hv_vcpu_exit_t {
            reason: HV_EXIT_REASON_UNKNOWN,
            exception: hv_vcpu_exit_exception_t::default(),
        };
        assert_eq!(
            VcpuExit::from_raw(&raw),
            VcpuExit::Unknown(HV_EXIT_REASON_UNKNOWN)
        );

        let raw = hv_vcpu_exit_t {
            reason: 42,
            exception: hv_vcpu_exit_exception_t::default(),
        };
        assert_eq!(VcpuExit::from_raw(&raw), VcpuExit::Unknown(42));
    }
}
