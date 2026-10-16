// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Virtual CPUs.

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use hvf_bindings::{hv_vcpu_exit_t, hv_vcpu_t};

use crate::error::Result;
use crate::exit::{DataAbortSyndrome, VcpuExit};
use crate::regs::{InterruptType, Reg, SimdReg, SysReg, TlbiOp};
use crate::vm::VmInner;

/// A vCPU, bound to the thread that created it.
///
/// Every framework vCPU function must be called on the owning thread, so
/// `Vcpu` is neither `Send` nor `Sync`. Dropping it destroys the vCPU.
///
/// The initial `MPIDR_EL1` and `CPSR` are 0; set `CPSR` explicitly (for
/// example `0x3c5`, EL1h with interrupts masked) and `MPIDR_EL1` if the guest
/// or the GIC needs distinct affinities.
#[derive(Debug)]
pub struct Vcpu {
    pub(crate) id: hv_vcpu_t,
    #[allow(dead_code)]
    pub(crate) exit: NonNull<hv_vcpu_exit_t>,
    #[allow(dead_code)]
    pub(crate) shared: Arc<VcpuShared>,
    #[allow(dead_code)]
    pub(crate) vm: Arc<VmInner>,
    pub(crate) _not_send_sync: PhantomData<*mut ()>,
}

/// State shared between a [`Vcpu`] and its [`VcpuKicker`]s.
#[derive(Debug)]
pub(crate) struct VcpuShared {
    /// The vCPU id.
    #[allow(dead_code)]
    pub(crate) id: hv_vcpu_t,
    /// `false` once the vCPU is being destroyed. Held across
    /// `hv_vcpu_destroy` and `hv_vcpus_exit`.
    #[allow(dead_code)]
    pub(crate) alive: Mutex<bool>,
}

/// A handle that forces a [`Vcpu`] out of the guest, usable from any thread.
///
/// A kick while the vCPU is not running is remembered: the next
/// [`Vcpu::run`] returns [`VcpuExit::Canceled`] immediately.
#[derive(Debug, Clone)]
pub struct VcpuKicker {
    #[allow(dead_code)]
    pub(crate) shared: Arc<VcpuShared>,
}

impl VcpuKicker {
    /// The id of the vCPU.
    pub fn id(&self) -> u64 {
        todo!()
    }

    /// Kicks the vCPU (`hv_vcpus_exit`). Returns `Ok(false)` if the vCPU was
    /// already destroyed.
    pub fn kick(&self) -> Result<bool> {
        todo!()
    }
}

impl Vcpu {
    /// The vCPU id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// A handle that kicks this vCPU from any thread.
    pub fn kicker(&self) -> VcpuKicker {
        todo!()
    }

    /// Runs the guest until the next exit.
    pub fn run(&mut self) -> Result<VcpuExit> {
        todo!()
    }

    /// Reads a general purpose or special register.
    pub fn get_reg(&self, _reg: Reg) -> Result<u64> {
        todo!()
    }

    /// Writes a general purpose or special register.
    pub fn set_reg(&mut self, _reg: Reg, _value: u64) -> Result<()> {
        todo!()
    }

    /// Reads a system register.
    pub fn get_sys_reg(&self, _reg: SysReg) -> Result<u64> {
        todo!()
    }

    /// Writes a system register.
    pub fn set_sys_reg(&mut self, _reg: SysReg, _value: u64) -> Result<()> {
        todo!()
    }

    /// Reads a SIMD/FP register (little-endian lanes).
    pub fn get_simd_fp_reg(&self, _reg: SimdReg) -> Result<u128> {
        todo!()
    }

    /// Writes a SIMD/FP register (little-endian lanes).
    pub fn set_simd_fp_reg(&mut self, _reg: SimdReg, _value: u128) -> Result<()> {
        todo!()
    }

    /// Whether an interrupt of type `irq` is pending.
    pub fn pending_interrupt(&self, _irq: InterruptType) -> Result<bool> {
        todo!()
    }

    /// Sets the pending state of an interrupt line.
    ///
    /// The framework clears the pending state after every [`run`](Self::run),
    /// so a level-triggered line must be set again before each run. Fails
    /// with `HV_UNSUPPORTED` if the VM has an in-kernel GIC.
    pub fn set_pending_interrupt(&mut self, _irq: InterruptType, _pending: bool) -> Result<()> {
        todo!()
    }

    /// Whether the virtual timer is masked.
    pub fn vtimer_mask(&self) -> Result<bool> {
        todo!()
    }

    /// Masks or unmasks the virtual timer. The framework masks it on every
    /// [`VcpuExit::VtimerActivated`] exit.
    pub fn set_vtimer_mask(&mut self, _masked: bool) -> Result<()> {
        todo!()
    }

    /// The virtual timer offset (`CNTVCT_EL0 = mach_absolute_time() - offset`).
    pub fn vtimer_offset(&self) -> Result<u64> {
        todo!()
    }

    /// Sets the virtual timer offset.
    pub fn set_vtimer_offset(&mut self, _offset: u64) -> Result<()> {
        todo!()
    }

    /// Whether debug exceptions exit to the VMM.
    pub fn trap_debug_exceptions(&self) -> Result<bool> {
        todo!()
    }

    /// Sets whether debug exceptions exit to the VMM.
    pub fn set_trap_debug_exceptions(&mut self, _trap: bool) -> Result<()> {
        todo!()
    }

    /// Whether debug register accesses exit to the VMM.
    pub fn trap_debug_reg_accesses(&self) -> Result<bool> {
        todo!()
    }

    /// Sets whether debug register accesses exit to the VMM.
    pub fn set_trap_debug_reg_accesses(&mut self, _trap: bool) -> Result<()> {
        todo!()
    }

    /// The accumulated guest execution time, in Mach absolute time units.
    pub fn exec_time(&self) -> Result<u64> {
        todo!()
    }

    /// The accumulated time spent waiting for interrupts, in Mach absolute
    /// time units (macOS 27.0).
    pub fn wfi_time(&self) -> Result<u64> {
        todo!()
    }

    /// Whether an SError is pending (macOS 27.0).
    pub fn serror_pending(&self) -> Result<bool> {
        todo!()
    }

    /// Sets the pending state of an SError (macOS 27.0).
    pub fn set_serror(&mut self, _pending: bool) -> Result<()> {
        todo!()
    }

    /// Performs a guest TLB invalidation operation (macOS 27.0).
    pub fn invalidate_tlb(&mut self, _op: TlbiOp, _param: u64) -> Result<()> {
        todo!()
    }

    /// Advances the PC by one instruction (4 bytes), as needed after
    /// emulating any trapped instruction except `HVC`.
    pub fn advance_pc(&mut self) -> Result<()> {
        todo!()
    }

    /// The value an MMIO store described by `syndrome` writes: the transfer
    /// register (0 for `XZR`) masked to the access size.
    pub fn mmio_write_value(&self, _syndrome: &DataAbortSyndrome) -> Result<u64> {
        todo!()
    }

    /// Completes an MMIO load described by `syndrome` with the device value
    /// `raw`: writes the transfer register (unless it is `XZR`) and advances
    /// the PC.
    pub fn complete_mmio_read(&mut self, _syndrome: &DataAbortSyndrome, _raw: u64) -> Result<()> {
        todo!()
    }
}
