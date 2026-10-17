// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Virtual CPUs.

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use hvf_bindings::{
    HV_SUCCESS, hv_simd_fp_uchar16_t, hv_vcpu_destroy, hv_vcpu_exit_t, hv_vcpu_get_exec_time,
    hv_vcpu_get_pending_interrupt, hv_vcpu_get_reg, hv_vcpu_get_simd_fp_reg, hv_vcpu_get_sys_reg,
    hv_vcpu_get_trap_debug_exceptions, hv_vcpu_get_trap_debug_reg_accesses,
    hv_vcpu_get_vtimer_mask, hv_vcpu_get_vtimer_offset, hv_vcpu_run, hv_vcpu_set_pending_interrupt,
    hv_vcpu_set_reg, hv_vcpu_set_simd_fp_reg_by_ref, hv_vcpu_set_sys_reg,
    hv_vcpu_set_trap_debug_exceptions, hv_vcpu_set_trap_debug_reg_accesses,
    hv_vcpu_set_vtimer_mask, hv_vcpu_set_vtimer_offset, hv_vcpu_t, hv_vcpus_exit,
};

use crate::error::{Error, Result, check};
use crate::exit::{DataAbortSyndrome, VcpuExit};
use crate::mmio::{mmio_load_value, mmio_store_value};
use crate::regs::{InterruptType, Reg, SimdReg, SysReg, TlbiOp};
use crate::system::out_param;
use crate::vm::VmInner;

/// Register number of the zero register in a data abort syndrome.
const XZR: u8 = 31;

/// A vCPU, bound to the thread that created it.
///
/// Every framework vCPU function must be called on the owning thread, so
/// `Vcpu` is neither `Send` nor `Sync`. Dropping it destroys the vCPU, on the
/// owning thread, and lets the VM be destroyed once no other handle remains.
///
/// The initial `MPIDR_EL1` and `CPSR` read as 0; set `CPSR` explicitly (for
/// example `0x3c5`, EL1h with interrupts masked). With the in-kernel GIC,
/// also write `MPIDR_EL1` on every vCPU, even when 0 is the wanted affinity,
/// before it runs or its redistributor is used or restored: until it is
/// written, the vCPU has no redistributor. Guest accesses to its frame then
/// exit as MMIO, [`gic_redistributor_base`](Self::gic_redistributor_base)
/// fails with `HV_BAD_ARGUMENT`, and the redistributor register accessors
/// fail with `HV_DENIED`, which here does not mean a missing entitlement.
/// Without a GIC, set `MPIDR_EL1` if the guest needs distinct affinities.
///
/// Moving a vCPU to another thread does not compile:
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<hvf::Vcpu>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<hvf::Vcpu>();
/// ```
///
/// Use a [`VcpuKicker`] to interact with a running vCPU from another thread.
#[derive(Debug)]
pub struct Vcpu {
    pub(crate) id: hv_vcpu_t,
    /// Exit information written by `hv_vcpu_run`, owned by the framework and
    /// valid until `hv_vcpu_destroy`.
    pub(crate) exit: NonNull<hv_vcpu_exit_t>,
    pub(crate) shared: Arc<VcpuShared>,
    /// Keeps the VM alive while the vCPU exists. Dropped after `Drop::drop`
    /// destroyed the vCPU.
    #[allow(dead_code)]
    pub(crate) vm: Arc<VmInner>,
    pub(crate) _not_send_sync: PhantomData<*mut ()>,
}

/// State shared between a [`Vcpu`] and its [`VcpuKicker`]s.
#[derive(Debug)]
pub(crate) struct VcpuShared {
    /// The vCPU id.
    pub(crate) id: hv_vcpu_t,
    /// `false` once the vCPU is being destroyed. Held across
    /// `hv_vcpu_destroy` and `hv_vcpus_exit`.
    pub(crate) alive: Mutex<bool>,
}

impl VcpuShared {
    /// Locks `alive`, ignoring poisoning (the flag is always consistent).
    fn lock_alive(&self) -> MutexGuard<'_, bool> {
        self.alive.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A handle that forces a [`Vcpu`] out of the guest, usable from any thread.
///
/// A kick while the vCPU is not running is remembered: the next
/// [`Vcpu::run`] returns [`VcpuExit::Canceled`] immediately, like KVM's
/// `immediate_exit`.
///
/// A kicker never affects another vCPU, even one that reuses the id of its
/// destroyed vCPU: kicks after the destroy return `Ok(false)`.
#[derive(Debug, Clone)]
pub struct VcpuKicker {
    pub(crate) shared: Arc<VcpuShared>,
}

impl VcpuKicker {
    /// The id of the vCPU.
    pub fn id(&self) -> u64 {
        self.shared.id
    }

    /// Kicks the vCPU (`hv_vcpus_exit`). Returns `Ok(false)` if the vCPU was
    /// already destroyed.
    pub fn kick(&self) -> Result<bool> {
        // The vCPU cannot be destroyed, and so its id cannot be reused, while
        // the lock is held.
        let alive = self.shared.lock_alive();
        if !*alive {
            return Ok(false);
        }
        let mut id = self.shared.id;
        // SAFETY: `hv_vcpus_exit` may be called from any thread and reads one
        // id from the valid pointer. The id names our live vCPU: its destroy
        // has not started, since `Vcpu::drop` clears `alive` under this lock
        // first.
        let ret = unsafe { hv_vcpus_exit(&mut id, 1) };
        drop(alive);
        check("hv_vcpus_exit", ret)?;
        Ok(true)
    }
}

impl Drop for Vcpu {
    fn drop(&mut self) {
        // Holding the lock across the destroy orders it after every kick that
        // saw the vCPU alive, and before every kick that sees it destroyed.
        let mut alive = self.shared.lock_alive();
        *alive = false;
        // SAFETY: `Vcpu` is neither `Send` nor `Sync`, so this runs on the
        // thread that created the vCPU, and the vCPU is destroyed only once.
        // `exit` is not used after this.
        let ret = unsafe { hv_vcpu_destroy(self.id) };
        drop(alive);
        debug_assert!(
            ret == HV_SUCCESS || std::thread::panicking(),
            "hv_vcpu_destroy({}) failed: {}",
            self.id,
            crate::HvError(ret)
        );
    }
}

impl Vcpu {
    /// Wraps a vCPU just created by `hv_vcpu_create`.
    ///
    /// # Safety
    ///
    /// `id` must have been returned by a successful `hv_vcpu_create` on the
    /// calling thread, together with `exit`, and must not be owned by anything
    /// else.
    pub(crate) unsafe fn from_created(
        id: hv_vcpu_t,
        exit: *mut hv_vcpu_exit_t,
        vm: Arc<VmInner>,
    ) -> Result<Vcpu> {
        let shared = Arc::new(VcpuShared {
            id,
            alive: Mutex::new(true),
        });
        let Some(exit) = NonNull::new(exit) else {
            // The framework never returns a null pointer here; destroy the
            // vCPU rather than handing out an unusable one.
            // SAFETY: the caller passes ownership of the vCPU, created on this
            // thread.
            unsafe { hv_vcpu_destroy(id) };
            return Err(Error::NullObject("hv_vcpu_create"));
        };
        Ok(Vcpu {
            id,
            exit,
            shared,
            vm,
            _not_send_sync: PhantomData,
        })
    }

    /// The vCPU id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// A handle that kicks this vCPU from any thread.
    pub fn kicker(&self) -> VcpuKicker {
        VcpuKicker {
            shared: self.shared.clone(),
        }
    }

    /// Runs the guest until the next exit (`hv_vcpu_run`).
    ///
    /// Exits that need emulation are returned as [`VcpuExit::Exception`];
    /// except after `HVC`, the PC still points at the trapping instruction
    /// (see [`advance_pc`](Self::advance_pc)).
    pub fn run(&mut self) -> Result<VcpuExit> {
        // SAFETY: called on the owning thread (`Vcpu` is `!Send`) with a live
        // vCPU id.
        check("hv_vcpu_run", unsafe { hv_vcpu_run(self.id) })?;
        // SAFETY: `exit` points to the framework's exit information of this
        // vCPU, valid until it is destroyed and written only by `hv_vcpu_run`
        // on this thread, which has returned.
        let raw = unsafe { self.exit.as_ptr().read() };
        Ok(VcpuExit::from_raw(&raw))
    }

    /// Reads a general purpose or special register.
    pub fn get_reg(&self, reg: Reg) -> Result<u64> {
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_vcpu_get_reg", |p| unsafe {
            hv_vcpu_get_reg(self.id, reg.0, p)
        })
    }

    /// Writes a general purpose or special register.
    pub fn set_reg(&mut self, reg: Reg, value: u64) -> Result<()> {
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_set_reg", unsafe {
            hv_vcpu_set_reg(self.id, reg.0, value)
        })
    }

    /// Reads a system register.
    pub fn get_sys_reg(&self, reg: SysReg) -> Result<u64> {
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_vcpu_get_sys_reg", |p| unsafe {
            hv_vcpu_get_sys_reg(self.id, reg.0, p)
        })
    }

    /// Writes a system register.
    pub fn set_sys_reg(&mut self, reg: SysReg, value: u64) -> Result<()> {
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_set_sys_reg", unsafe {
            hv_vcpu_set_sys_reg(self.id, reg.0, value)
        })
    }

    /// Reads a SIMD/FP register (little-endian lanes).
    pub fn get_simd_fp_reg(&self, reg: SimdReg) -> Result<u128> {
        // SAFETY: owning thread, live vCPU, pointer valid for a 16-byte
        // aligned `hv_simd_fp_uchar16_t`.
        let value = out_param("hv_vcpu_get_simd_fp_reg", |p| unsafe {
            hv_vcpu_get_simd_fp_reg(self.id, reg.0, p)
        })?;
        Ok(u128::from_le_bytes(value.0))
    }

    /// Writes a SIMD/FP register (little-endian lanes).
    pub fn set_simd_fp_reg(&mut self, reg: SimdReg, value: u128) -> Result<()> {
        let value = hv_simd_fp_uchar16_t(value.to_le_bytes());
        // SAFETY: owning thread, live vCPU; `value` is readable for 16 bytes.
        check("hv_vcpu_set_simd_fp_reg", unsafe {
            hv_vcpu_set_simd_fp_reg_by_ref(self.id, reg.0, &value)
        })
    }

    /// Whether an interrupt of type `irq` is pending.
    ///
    /// Fails with `HV_UNSUPPORTED` if the VM has an in-kernel GIC.
    pub fn pending_interrupt(&self, irq: InterruptType) -> Result<bool> {
        // SAFETY: owning thread, live vCPU, pointer valid for a `bool`.
        out_param("hv_vcpu_get_pending_interrupt", |p| unsafe {
            hv_vcpu_get_pending_interrupt(self.id, irq as u32, p)
        })
    }

    /// Sets the pending state of an interrupt line.
    ///
    /// The framework clears the pending state after every [`run`](Self::run),
    /// so a level-triggered line must be set again before each run. Fails
    /// with `HV_UNSUPPORTED` if the VM has an in-kernel GIC.
    pub fn set_pending_interrupt(&mut self, irq: InterruptType, pending: bool) -> Result<()> {
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_set_pending_interrupt", unsafe {
            hv_vcpu_set_pending_interrupt(self.id, irq as u32, pending)
        })
    }

    /// Whether the virtual timer is masked.
    pub fn vtimer_mask(&self) -> Result<bool> {
        // SAFETY: owning thread, live vCPU, pointer valid for a `bool`.
        out_param("hv_vcpu_get_vtimer_mask", |p| unsafe {
            hv_vcpu_get_vtimer_mask(self.id, p)
        })
    }

    /// Masks or unmasks the virtual timer. The framework masks it on every
    /// [`VcpuExit::VtimerActivated`] exit; unmask it once the guest has
    /// handled the timer interrupt.
    pub fn set_vtimer_mask(&mut self, masked: bool) -> Result<()> {
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_set_vtimer_mask", unsafe {
            hv_vcpu_set_vtimer_mask(self.id, masked)
        })
    }

    /// The virtual timer offset (`CNTVCT_EL0 = mach_absolute_time() - offset`).
    pub fn vtimer_offset(&self) -> Result<u64> {
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_vcpu_get_vtimer_offset", |p| unsafe {
            hv_vcpu_get_vtimer_offset(self.id, p)
        })
    }

    /// Sets the virtual timer offset.
    pub fn set_vtimer_offset(&mut self, offset: u64) -> Result<()> {
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_set_vtimer_offset", unsafe {
            hv_vcpu_set_vtimer_offset(self.id, offset)
        })
    }

    /// Whether debug exceptions exit to the VMM.
    pub fn trap_debug_exceptions(&self) -> Result<bool> {
        // SAFETY: owning thread, live vCPU, pointer valid for a `bool`.
        out_param("hv_vcpu_get_trap_debug_exceptions", |p| unsafe {
            hv_vcpu_get_trap_debug_exceptions(self.id, p)
        })
    }

    /// Sets whether debug exceptions exit to the VMM.
    pub fn set_trap_debug_exceptions(&mut self, trap: bool) -> Result<()> {
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_set_trap_debug_exceptions", unsafe {
            hv_vcpu_set_trap_debug_exceptions(self.id, trap)
        })
    }

    /// Whether debug register accesses exit to the VMM.
    pub fn trap_debug_reg_accesses(&self) -> Result<bool> {
        // SAFETY: owning thread, live vCPU, pointer valid for a `bool`.
        out_param("hv_vcpu_get_trap_debug_reg_accesses", |p| unsafe {
            hv_vcpu_get_trap_debug_reg_accesses(self.id, p)
        })
    }

    /// Sets whether debug register accesses exit to the VMM.
    pub fn set_trap_debug_reg_accesses(&mut self, trap: bool) -> Result<()> {
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_set_trap_debug_reg_accesses", unsafe {
            hv_vcpu_set_trap_debug_reg_accesses(self.id, trap)
        })
    }

    /// The accumulated guest execution time, in Mach absolute time units.
    pub fn exec_time(&self) -> Result<u64> {
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_vcpu_get_exec_time", |p| unsafe {
            hv_vcpu_get_exec_time(self.id, p)
        })
    }

    /// The accumulated time spent waiting for interrupts, in Mach absolute
    /// time units (macOS 27.0). Fails with `HV_UNSUPPORTED` without an
    /// in-kernel GIC.
    pub fn wfi_time(&self) -> Result<u64> {
        let f = need!(hv_vcpu_get_wait_for_interrupt_time);
        // SAFETY: owning thread, live vCPU, pointer valid for a `u64`.
        out_param("hv_vcpu_get_wait_for_interrupt_time", |p| unsafe {
            f(self.id, p)
        })
    }

    /// Whether an SError is pending (macOS 27.0).
    pub fn serror_pending(&self) -> Result<bool> {
        let f = need!(hv_vcpu_get_serror);
        // SAFETY: owning thread, live vCPU, pointer valid for a `bool`.
        out_param("hv_vcpu_get_serror", |p| unsafe { f(self.id, p) })
    }

    /// Sets the pending state of an SError (macOS 27.0).
    pub fn set_serror(&mut self, pending: bool) -> Result<()> {
        let f = need!(hv_vcpu_set_serror);
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_set_serror", unsafe { f(self.id, pending) })
    }

    /// Performs a guest TLB invalidation operation (macOS 27.0). `param` is
    /// the operand of the `TLBI` instruction. With EL2 enabled it acts on the
    /// guest hypervisor's translations.
    pub fn invalidate_tlb(&mut self, op: TlbiOp, param: u64) -> Result<()> {
        let f = need!(hv_vcpu_invalidate_tlb);
        // SAFETY: owning thread, live vCPU.
        check("hv_vcpu_invalidate_tlb", unsafe { f(self.id, op.0, param) })
    }

    /// Advances the PC by one instruction (4 bytes), as needed after
    /// emulating any trapped instruction except `HVC`.
    pub fn advance_pc(&mut self) -> Result<()> {
        let pc = self.get_reg(Reg::PC)?;
        self.set_reg(Reg::PC, pc.wrapping_add(4))
    }

    /// The general purpose register `n` of a syndrome.
    fn syndrome_reg(n: u8) -> Result<Reg> {
        Reg::x(n).ok_or(Error::InvalidRegister(u32::from(n)))
    }

    /// Completes an MMIO store described by `syndrome`: returns the value the
    /// guest stores (the transfer register, 0 for `XZR`, masked to the access
    /// size) and advances the PC. The caller performs the store with the
    /// returned value.
    ///
    /// This is the store counterpart of
    /// [`complete_mmio_read`](Self::complete_mmio_read): after either one,
    /// do not call [`advance_pc`](Self::advance_pc). On error the PC is
    /// left at the store.
    pub fn complete_mmio_write(&mut self, syndrome: &DataAbortSyndrome) -> Result<u64> {
        let value = self.mmio_write_value(syndrome)?;
        self.advance_pc()?;
        Ok(value)
    }

    /// The value an MMIO store described by `syndrome` writes: the transfer
    /// register (0 for `XZR`) masked to the access size.
    ///
    /// Unlike [`complete_mmio_write`](Self::complete_mmio_write), this does
    /// not advance the PC: the caller performs the store and then calls
    /// [`advance_pc`](Self::advance_pc). Forgetting to makes the guest
    /// repeat the store forever.
    pub fn mmio_write_value(&self, syndrome: &DataAbortSyndrome) -> Result<u64> {
        let reg = match syndrome.srt {
            XZR => 0,
            n => self.get_reg(Self::syndrome_reg(n)?)?,
        };
        Ok(mmio_store_value(syndrome, reg))
    }

    /// Completes an MMIO load described by `syndrome` with the device value
    /// `raw`: writes the transfer register (unless it is `XZR`) and advances
    /// the PC.
    pub fn complete_mmio_read(&mut self, syndrome: &DataAbortSyndrome, raw: u64) -> Result<()> {
        if syndrome.srt != XZR {
            let reg = Self::syndrome_reg(syndrome.srt)?;
            self.set_reg(reg, mmio_load_value(syndrome, raw))?;
        }
        self.advance_pc()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::thread;
    use std::time::Duration;

    use crate::system::tests::hv_test_lock;
    use crate::{ExceptionClass, Hvf, MemoryFlags, Vm, VmConfig};

    /// `b .`
    const SPIN: [u8; 4] = [0x00, 0x00, 0x00, 0x14];
    /// `hvc #0; b .`
    const HVC: [u8; 8] = [0x02, 0x00, 0x00, 0xd4, 0x00, 0x00, 0x00, 0x14];
    const GUEST_IPA: u64 = 0x4000_0000;

    /// A VM with one granule of guest code at `GUEST_IPA`.
    struct Guest {
        vm: Vm,
        host: *mut libc::c_void,
        size: usize,
    }

    impl Guest {
        fn new(code: &[u8]) -> Guest {
            let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
            let size = vm.ipa_granule() as usize;
            // SAFETY: an anonymous private mapping has no preconditions; the
            // result is checked below.
            let host = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_ANON | libc::MAP_PRIVATE,
                    -1,
                    0,
                )
            };
            assert_ne!(host, libc::MAP_FAILED);
            // SAFETY: `host` is a fresh mapping of `size >= code.len()` bytes.
            unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), host.cast(), code.len()) };
            // SAFETY: the mapping lives until `drop`, which runs after every
            // test dropped its vCPUs, and the host does not touch it.
            unsafe {
                vm.map(
                    host.cast(),
                    GUEST_IPA,
                    size,
                    MemoryFlags::READ | MemoryFlags::EXEC,
                )
            }
            .unwrap();
            Guest { vm, host, size }
        }

        fn vcpu(&self) -> Vcpu {
            let mut vcpu = self.vm.create_vcpu(None).unwrap();
            vcpu.set_reg(Reg::PC, GUEST_IPA).unwrap();
            vcpu.set_reg(Reg::CPSR, 0x3c5).unwrap();
            vcpu
        }
    }

    impl Drop for Guest {
        fn drop(&mut self) {
            self.vm.unmap(GUEST_IPA, self.size).unwrap();
            // SAFETY: the guest mapping was removed above.
            unsafe { libc::munmap(self.host, self.size) };
        }
    }

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VcpuKicker>();
    }

    #[test]
    fn test_regs() {
        let Some(_g) = hv_test_lock() else { return };
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        let mut vcpu = vm.create_vcpu(None).unwrap();
        assert_eq!(vcpu.get_reg(Reg::CPSR).unwrap(), 0);
        assert_eq!(
            vcpu.get_sys_reg(SysReg::MPIDR_EL1).unwrap() & 0xff_00ff_ffff,
            0
        );

        for n in 0..=30 {
            let reg = Reg::x(n).unwrap();
            vcpu.set_reg(reg, 0x1111_0000 + u64::from(n)).unwrap();
        }
        for n in 0..=30 {
            let reg = Reg::x(n).unwrap();
            assert_eq!(vcpu.get_reg(reg).unwrap(), 0x1111_0000 + u64::from(n));
        }
        vcpu.set_reg(Reg::PC, 0x1234_5678).unwrap();
        vcpu.advance_pc().unwrap();
        assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), 0x1234_567c);
        let e = vcpu.set_reg(Reg(0xffff), 0).unwrap_err();
        assert!(
            matches!(
                e,
                Error::Hv {
                    call: "hv_vcpu_set_reg",
                    ..
                }
            ),
            "{e:?}"
        );

        vcpu.set_sys_reg(SysReg::VBAR_EL1, 0x8_0000).unwrap();
        assert_eq!(vcpu.get_sys_reg(SysReg::VBAR_EL1).unwrap(), 0x8_0000);

        for n in 0..32 {
            let reg = SimdReg::q(n).unwrap();
            let value = u128::from_le_bytes([n; 16]) ^ (1u128 << 127);
            vcpu.set_simd_fp_reg(reg, value).unwrap();
        }
        for n in 0..32 {
            let reg = SimdReg::q(n).unwrap();
            let value = u128::from_le_bytes([n; 16]) ^ (1u128 << 127);
            assert_eq!(vcpu.get_simd_fp_reg(reg).unwrap(), value);
        }

        vcpu.set_vtimer_offset(42).unwrap();
        assert_eq!(vcpu.vtimer_offset().unwrap(), 42);
        vcpu.set_vtimer_mask(true).unwrap();
        assert!(vcpu.vtimer_mask().unwrap());
        vcpu.set_vtimer_mask(false).unwrap();
        assert!(!vcpu.vtimer_mask().unwrap());

        vcpu.set_trap_debug_exceptions(true).unwrap();
        assert!(vcpu.trap_debug_exceptions().unwrap());
        vcpu.set_trap_debug_reg_accesses(true).unwrap();
        assert!(vcpu.trap_debug_reg_accesses().unwrap());

        vcpu.set_pending_interrupt(InterruptType::Irq, true)
            .unwrap();
        assert!(vcpu.pending_interrupt(InterruptType::Irq).unwrap());
        assert!(!vcpu.pending_interrupt(InterruptType::Fiq).unwrap());

        vcpu.exec_time().unwrap();
        // macOS 27.0 functions.
        match vcpu.set_serror(false) {
            Ok(()) => assert!(!vcpu.serror_pending().unwrap()),
            Err(Error::Unavailable { since: (27, 0), .. }) => {}
            Err(e) => panic!("{e}"),
        }
        match vcpu.invalidate_tlb(TlbiOp::VMALLE1IS, 0) {
            Ok(()) | Err(Error::Unavailable { since: (27, 0), .. }) => {}
            Err(e) => panic!("{e}"),
        }
        match vcpu.wfi_time() {
            // Unsupported without an in-kernel GIC.
            Ok(_) | Err(Error::Unavailable { since: (27, 0), .. }) => {}
            Err(Error::Hv { err, .. }) if err.is_unsupported() => {}
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn test_mmio_helpers() {
        let Some(_g) = hv_test_lock() else { return };
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        let mut vcpu = vm.create_vcpu(None).unwrap();
        let syndrome = |len, srt, sign_extend, sf64| DataAbortSyndrome {
            len,
            srt,
            sign_extend,
            sf64,
            acquire_release: false,
        };

        vcpu.set_reg(Reg::x(3).unwrap(), 0x1122_3344_5566_7788)
            .unwrap();
        assert_eq!(
            vcpu.mmio_write_value(&syndrome(2, 3, false, true)).unwrap(),
            0x7788
        );
        assert_eq!(
            vcpu.mmio_write_value(&syndrome(8, 31, false, true))
                .unwrap(),
            0
        );
        assert_eq!(
            vcpu.mmio_write_value(&syndrome(8, 32, false, true)),
            Err(Error::InvalidRegister(32))
        );

        // `complete_mmio_write` returns the same value and advances the PC,
        // like `complete_mmio_read`; on error it leaves the PC alone.
        vcpu.set_reg(Reg::PC, 0x2000).unwrap();
        assert_eq!(
            vcpu.complete_mmio_write(&syndrome(4, 3, false, true))
                .unwrap(),
            0x5566_7788
        );
        assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), 0x2004);
        assert_eq!(
            vcpu.complete_mmio_write(&syndrome(8, 32, false, true)),
            Err(Error::InvalidRegister(32))
        );
        assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), 0x2004);

        vcpu.set_reg(Reg::PC, 0x1000).unwrap();
        vcpu.complete_mmio_read(&syndrome(1, 4, true, true), 0x80)
            .unwrap();
        assert_eq!(vcpu.get_reg(Reg::x(4).unwrap()).unwrap(), u64::MAX - 0x7f);
        assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), 0x1004);
        // A load into XZR only advances the PC.
        vcpu.complete_mmio_read(&syndrome(4, 31, false, false), 1)
            .unwrap();
        assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), 0x1008);
    }

    #[test]
    fn test_second_vcpu_on_thread() {
        let Some(_g) = hv_test_lock() else { return };
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        let vcpu = vm.create_vcpu(None).unwrap();
        let e = vm.create_vcpu(None).unwrap_err();
        assert!(
            matches!(e, Error::Hv { call: "hv_vcpu_create", err } if err.is_busy()),
            "{e:?}"
        );
        drop(vcpu);
        // With the first one destroyed, the thread can own a new vCPU.
        let cfg = crate::VcpuConfig::new().unwrap();
        drop(vm.create_vcpu(Some(&cfg)).unwrap());
    }

    #[test]
    fn test_kick_before_run() {
        let Some(_g) = hv_test_lock() else { return };
        let guest = Guest::new(&HVC);
        let mut vcpu = guest.vcpu();
        let kicker = vcpu.kicker();
        assert_eq!(kicker.id(), vcpu.id());
        assert!(kicker.kick().unwrap());
        assert_eq!(vcpu.run().unwrap(), VcpuExit::Canceled);
        // The kick is consumed: the next run reaches the guest.
        match vcpu.run().unwrap() {
            VcpuExit::Exception(e) => {
                assert!(matches!(e.decode(), ExceptionClass::Hvc { imm: 0 }))
            }
            other => panic!("unexpected exit {other:?}"),
        }
        drop(vcpu);
        // A kicker of a destroyed vCPU does nothing.
        assert!(!kicker.kick().unwrap());
    }

    #[test]
    fn test_kick_from_other_thread() {
        let Some(_g) = hv_test_lock() else { return };
        let guest = Guest::new(&SPIN);
        let mut vcpu = guest.vcpu();
        let kicker = vcpu.kicker();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            kicker.kick().unwrap()
        });
        assert_eq!(vcpu.run().unwrap(), VcpuExit::Canceled);
        assert!(handle.join().unwrap());
    }

    #[test]
    fn test_stale_kicker_and_id_reuse() {
        let Some(_g) = hv_test_lock() else { return };
        let guest = Guest::new(&HVC);
        let vcpu = guest.vcpu();
        let stale = vcpu.kicker();
        let old_id = vcpu.id();
        drop(vcpu);

        // The framework reuses the id of the destroyed vCPU.
        let mut vcpu = guest.vcpu();
        assert_eq!(vcpu.id(), old_id);
        assert!(!stale.kick().unwrap());
        match vcpu.run().unwrap() {
            VcpuExit::Exception(e) => {
                assert!(matches!(e.decode(), ExceptionClass::Hvc { imm: 0 }))
            }
            other => panic!("unexpected exit {other:?}"),
        }
    }

    #[test]
    fn test_vcpus_on_threads() {
        let Some(_g) = hv_test_lock() else { return };
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        let mut ids: Vec<u64> = thread::scope(|s| {
            let (tx, rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
            let release_rx = Arc::new(Mutex::new(release_rx));
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let tx = tx.clone();
                    let release_rx = release_rx.clone();
                    let vm = &vm;
                    s.spawn(move || {
                        let vcpu = vm.create_vcpu(None).unwrap();
                        tx.send(vcpu.id()).unwrap();
                        // Keep every vCPU alive until all were created.
                        release_rx.lock().unwrap().recv().unwrap();
                    })
                })
                .collect();
            let ids = (0..8).map(|_| rx.recv().unwrap()).collect();
            for _ in 0..8 {
                release_tx.send(()).unwrap();
            }
            for h in handles {
                h.join().unwrap();
            }
            ids
        });
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 8);
    }
}
