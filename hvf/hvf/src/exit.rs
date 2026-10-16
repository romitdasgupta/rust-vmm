// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! vCPU exits and exception syndrome decoding.
//!
//! Everything here is pure data and arithmetic on `ESR_EL2` values, so it
//! builds and is tested on every target.

use hvf_bindings::hv_vcpu_exit_t;

use crate::regs::SysReg;

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
    pub fn from_raw(_raw: &hv_vcpu_exit_t) -> VcpuExit {
        todo!()
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
        todo!()
    }

    /// The instruction length bit, `ESR_EL2.IL` (bit 25): `true` for a
    /// 32-bit instruction.
    pub fn il(&self) -> bool {
        todo!()
    }

    /// The instruction specific syndrome, `ESR_EL2.ISS` (bits 24:0).
    pub fn iss(&self) -> u32 {
        todo!()
    }

    /// Decodes the syndrome.
    pub fn decode(&self) -> ExceptionClass {
        todo!()
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
        todo!()
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
    /// Whether this is a translation fault (`(dfsc & 0x3c) == 0x04`), as for
    /// an access to unmapped (MMIO) memory.
    pub fn is_translation_fault(&self) -> bool {
        todo!()
    }

    /// Whether this is a permission fault (`(dfsc & 0x3c) == 0x0c`), as for a
    /// write to memory mapped read-only.
    pub fn is_permission_fault(&self) -> bool {
        todo!()
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
