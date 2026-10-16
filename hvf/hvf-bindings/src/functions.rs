// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Hypervisor.framework functions (arm64).
//!
//! Functions available since macOS 11.0 are declared here and linked
//! directly. Newer functions are resolved at run time through [`DynApi`]; their
//! hard-linked declarations are in [`direct`].
//!
//! `hv_vcpu_set_simd_fp_reg` takes a vector by value and is therefore not
//! declared here; see
//! [`hv_vcpu_set_simd_fp_reg_by_ref`](crate::simd::hv_vcpu_set_simd_fp_reg_by_ref).
//!
//! Unless stated otherwise every function returns [`HV_SUCCESS`] or an error
//! code. All `hv_vm_*` and `hv_gic_*` functions act on the single VM of the
//! calling process. Functions taking a `vcpu` must be called on the thread
//! that created that vCPU, except `hv_vcpus_exit` and
//! `hv_gic_get_redistributor_base`.

use core::ffi::c_void;

use crate::gic::*;
use crate::regs::*;
use crate::sme::*;
use crate::types::*;

hv_fns! {
    // hv_vm.h

    /// Returns the maximum number of vCPUs supported in `max_vcpu_count`.
    (11, 0) fn hv_vm_get_max_vcpu_count(max_vcpu_count: *mut u32) -> hv_return_t;
    /// Creates the VM instance of the current process. `config` may be null
    /// for the default configuration.
    (11, 0) fn hv_vm_create(config: hv_vm_config_t) -> hv_return_t;
    /// Destroys the VM instance of the current process. Every vCPU must have
    /// been destroyed first.
    (11, 0) fn hv_vm_destroy() -> hv_return_t;
    /// Maps `size` bytes of the process address space at `addr` into the
    /// guest physical address space at `ipa`. `addr`, `ipa` and `size` must
    /// be aligned to the IPA granule.
    (11, 0) fn hv_vm_map(addr: *mut c_void, ipa: hv_ipa_t, size: usize, flags: hv_memory_flags_t) -> hv_return_t;
    /// Unmaps `size` bytes of the guest physical address space at `ipa`.
    (11, 0) fn hv_vm_unmap(ipa: hv_ipa_t, size: usize) -> hv_return_t;
    /// Changes the guest permissions of `size` bytes of the guest physical
    /// address space at `ipa`.
    (11, 0) fn hv_vm_protect(ipa: hv_ipa_t, size: usize, flags: hv_memory_flags_t) -> hv_return_t;

    // hv_vm_allocate.h

    /// Allocates `size` bytes (a multiple of the host page size) of anonymous
    /// memory suitable as guest memory, with accurate memory accounting, and
    /// stores its address in `*uvap`. Release it with `hv_vm_deallocate`.
    (12, 1) fn hv_vm_allocate(uvap: *mut *mut c_void, size: usize, flags: hv_allocate_flags_t) -> hv_return_t;
    /// Deallocates memory allocated with `hv_vm_allocate`.
    (12, 1) fn hv_vm_deallocate(uva: *mut c_void, size: usize) -> hv_return_t;

    // hv_vm_config.h

    /// Creates a VM configuration object, to be released with `os_release`.
    (13, 0) fn hv_vm_config_create() -> hv_vm_config_t;
    /// Returns the maximum supported IPA size in bits.
    (13, 0) fn hv_vm_config_get_max_ipa_size(ipa_bit_length: *mut u32) -> hv_return_t;
    /// Returns the IPA size in bits used when a configuration does not set
    /// one.
    (13, 0) fn hv_vm_config_get_default_ipa_size(ipa_bit_length: *mut u32) -> hv_return_t;
    /// Sets the IPA size in bits of a VM configuration. It must not exceed
    /// the value returned by `hv_vm_config_get_max_ipa_size`.
    (13, 0) fn hv_vm_config_set_ipa_size(config: hv_vm_config_t, ipa_bit_length: u32) -> hv_return_t;
    /// Returns the IPA size in bits of a VM configuration.
    (13, 0) fn hv_vm_config_get_ipa_size(config: hv_vm_config_t, ipa_bit_length: *mut u32) -> hv_return_t;
    /// Returns whether EL2 (nested virtualization) is supported on this host.
    (15, 0) fn hv_vm_config_get_el2_supported(el2_supported: *mut bool) -> hv_return_t;
    /// Returns whether EL2 is enabled in a VM configuration.
    (15, 0) fn hv_vm_config_get_el2_enabled(config: hv_vm_config_t, el2_enabled: *mut bool) -> hv_return_t;
    /// Enables or disables EL2 in a VM configuration.
    ///
    /// With EL2 disabled, guest PMU register accesses trap to the VMM. With
    /// EL2 enabled they are emulated by the framework or undefined, depending
    /// on `ID_AA64DFR0_EL1.PMUVer`.
    (15, 0) fn hv_vm_config_set_el2_enabled(config: hv_vm_config_t, el2_enabled: bool) -> hv_return_t;
    /// Returns the IPA granule used when a configuration does not set one.
    (26, 0) fn hv_vm_config_get_default_ipa_granule(granule: *mut hv_ipa_granule_t) -> hv_return_t;
    /// Returns the IPA granule of a VM configuration.
    (26, 0) fn hv_vm_config_get_ipa_granule(config: hv_vm_config_t, granule: *mut hv_ipa_granule_t) -> hv_return_t;
    /// Sets the IPA granule of a VM configuration.
    (26, 0) fn hv_vm_config_set_ipa_granule(config: hv_vm_config_t, granule: hv_ipa_granule_t) -> hv_return_t;

    // hv_vcpu_config.h

    /// Creates a vCPU configuration object, to be released with `os_release`.
    (11, 0) fn hv_vcpu_config_create() -> hv_vcpu_config_t;
    /// Returns the value of a feature register for a vCPU configuration.
    (11, 0) fn hv_vcpu_config_get_feature_reg(config: hv_vcpu_config_t, feature_reg: hv_feature_reg_t, value: *mut u64) -> hv_return_t;
    /// Returns the eight `CCSIDR_EL1` values of a cache type for a vCPU
    /// configuration. `values` must point to an array of 8 `u64`.
    (11, 0) fn hv_vcpu_config_get_ccsidr_el1_sys_reg_values(config: hv_vcpu_config_t, cache_type: hv_cache_type_t, values: *mut u64) -> hv_return_t;

    // hv_vcpu.h

    /// Creates a vCPU bound to the calling thread. A thread can own only one
    /// vCPU at a time. On success `*vcpu` is its ID and `*exit` points to
    /// framework-owned exit information, valid until the vCPU is destroyed.
    /// `config` may be null.
    (11, 0) fn hv_vcpu_create(vcpu: *mut hv_vcpu_t, exit: *mut *mut hv_vcpu_exit_t, config: hv_vcpu_config_t) -> hv_return_t;
    /// Destroys a vCPU. Must be called by the owning thread.
    (11, 0) fn hv_vcpu_destroy(vcpu: hv_vcpu_t) -> hv_return_t;
    /// Returns the value of a general purpose or special register.
    (11, 0) fn hv_vcpu_get_reg(vcpu: hv_vcpu_t, reg: hv_reg_t, value: *mut u64) -> hv_return_t;
    /// Sets the value of a general purpose or special register.
    (11, 0) fn hv_vcpu_set_reg(vcpu: hv_vcpu_t, reg: hv_reg_t, value: u64) -> hv_return_t;
    /// Returns the value of a SIMD/FP register.
    (11, 0) fn hv_vcpu_get_simd_fp_reg(vcpu: hv_vcpu_t, reg: hv_simd_fp_reg_t, value: *mut hv_simd_fp_uchar16_t) -> hv_return_t;
    /// Returns the SME state (`PSTATE.SM` and `PSTATE.ZA`). Returns
    /// [`HV_UNSUPPORTED`] if SME is not supported.
    (15, 2) fn hv_vcpu_get_sme_state(vcpu: hv_vcpu_t, sme_state: *mut hv_vcpu_sme_state_t) -> hv_return_t;
    /// Sets the SME state. Entering or leaving streaming SVE mode zeroes the
    /// `Z` and `P` registers and sets all `FPSR` flags. Returns
    /// [`HV_UNSUPPORTED`] if SME is not supported.
    (15, 2) fn hv_vcpu_set_sme_state(vcpu: hv_vcpu_t, sme_state: *const hv_vcpu_sme_state_t) -> hv_return_t;
    /// Returns a `Z` register in streaming SVE mode. `length` must be the
    /// maximum streaming vector length in bytes.
    (15, 2) fn hv_vcpu_get_sme_z_reg(vcpu: hv_vcpu_t, reg: hv_sme_z_reg_t, value: *mut u8, length: usize) -> hv_return_t;
    /// Sets a `Z` register in streaming SVE mode. `length` must be the
    /// maximum streaming vector length in bytes.
    (15, 2) fn hv_vcpu_set_sme_z_reg(vcpu: hv_vcpu_t, reg: hv_sme_z_reg_t, value: *const u8, length: usize) -> hv_return_t;
    /// Returns a `P` register in streaming SVE mode. `length` must be the
    /// maximum streaming vector length in bytes divided by 8.
    (15, 2) fn hv_vcpu_get_sme_p_reg(vcpu: hv_vcpu_t, reg: hv_sme_p_reg_t, value: *mut u8, length: usize) -> hv_return_t;
    /// Sets a `P` register in streaming SVE mode. `length` must be the
    /// maximum streaming vector length in bytes divided by 8.
    (15, 2) fn hv_vcpu_set_sme_p_reg(vcpu: hv_vcpu_t, reg: hv_sme_p_reg_t, value: *const u8, length: usize) -> hv_return_t;
    /// Returns the `ZA` matrix. Requires `PSTATE.ZA`; `length` must be the
    /// square of the maximum streaming vector length in bytes.
    (15, 2) fn hv_vcpu_get_sme_za_reg(vcpu: hv_vcpu_t, value: *mut u8, length: usize) -> hv_return_t;
    /// Sets the `ZA` matrix. Requires `PSTATE.ZA`; `length` must be the
    /// square of the maximum streaming vector length in bytes.
    (15, 2) fn hv_vcpu_set_sme_za_reg(vcpu: hv_vcpu_t, value: *const u8, length: usize) -> hv_return_t;
    /// Returns the SME2 `ZT0` register. Requires `PSTATE.ZA`.
    (15, 2) fn hv_vcpu_get_sme_zt0_reg(vcpu: hv_vcpu_t, value: *mut hv_sme_zt0_uchar64_t) -> hv_return_t;
    /// Sets the SME2 `ZT0` register. Requires `PSTATE.ZA`.
    (15, 2) fn hv_vcpu_set_sme_zt0_reg(vcpu: hv_vcpu_t, value: *const hv_sme_zt0_uchar64_t) -> hv_return_t;
    /// Returns the value of a system register.
    (11, 0) fn hv_vcpu_get_sys_reg(vcpu: hv_vcpu_t, reg: hv_sys_reg_t, value: *mut u64) -> hv_return_t;
    /// Sets the value of a system register.
    (11, 0) fn hv_vcpu_set_sys_reg(vcpu: hv_vcpu_t, reg: hv_sys_reg_t, value: u64) -> hv_return_t;
    /// Returns whether an SError is pending.
    (27, 0) fn hv_vcpu_get_serror(vcpu: hv_vcpu_t, pending: *mut bool) -> hv_return_t;
    /// Sets whether an SError is pending.
    (27, 0) fn hv_vcpu_set_serror(vcpu: hv_vcpu_t, pending: bool) -> hv_return_t;
    /// Returns whether an IRQ or FIQ is pending. Returns [`HV_UNSUPPORTED`]
    /// if the VM has an in-kernel GIC.
    (11, 0) fn hv_vcpu_get_pending_interrupt(vcpu: hv_vcpu_t, type_: hv_interrupt_type_t, pending: *mut bool) -> hv_return_t;
    /// Sets whether an IRQ or FIQ is pending. The pending state is cleared
    /// every time `hv_vcpu_run` returns, so it must be set before each run.
    /// Returns [`HV_UNSUPPORTED`] if the VM has an in-kernel GIC.
    (11, 0) fn hv_vcpu_set_pending_interrupt(vcpu: hv_vcpu_t, type_: hv_interrupt_type_t, pending: bool) -> hv_return_t;
    /// Returns whether guest debug exceptions are trapped to the host.
    (11, 0) fn hv_vcpu_get_trap_debug_exceptions(vcpu: hv_vcpu_t, value: *mut bool) -> hv_return_t;
    /// Sets whether guest debug exceptions are trapped to the host.
    (11, 0) fn hv_vcpu_set_trap_debug_exceptions(vcpu: hv_vcpu_t, value: bool) -> hv_return_t;
    /// Returns whether guest accesses to the debug registers
    /// (`DBG{B,W}{C,V}R<n>_EL1`, `MDSCR_EL1`) are trapped to the host.
    (11, 0) fn hv_vcpu_get_trap_debug_reg_accesses(vcpu: hv_vcpu_t, value: *mut bool) -> hv_return_t;
    /// Sets whether guest accesses to the debug registers are trapped to the
    /// host.
    (11, 0) fn hv_vcpu_set_trap_debug_reg_accesses(vcpu: hv_vcpu_t, value: bool) -> hv_return_t;
    /// Runs a vCPU until the next exit, which is described by the exit
    /// information returned by `hv_vcpu_create`.
    (11, 0) fn hv_vcpu_run(vcpu: hv_vcpu_t) -> hv_return_t;
    /// Forces the `vcpu_count` vCPUs in `vcpus` to exit. May be called from
    /// any thread. A vCPU that is not running exits immediately from its
    /// next `hv_vcpu_run`.
    (11, 0) fn hv_vcpus_exit(vcpus: *mut hv_vcpu_t, vcpu_count: u32) -> hv_return_t;
    /// Returns the cumulative execution time of a vCPU in
    /// `mach_absolute_time` units.
    (11, 0) fn hv_vcpu_get_exec_time(vcpu: hv_vcpu_t, time: *mut u64) -> hv_return_t;
    /// Returns the cumulative time a vCPU spent in `WFI` waiting for an
    /// interrupt, in `mach_absolute_time` units. Returns [`HV_UNSUPPORTED`]
    /// if the VM has no in-kernel GIC.
    (27, 0) fn hv_vcpu_get_wait_for_interrupt_time(vcpu: hv_vcpu_t, time: *mut u64) -> hv_return_t;
    /// Returns whether the virtual timer is masked.
    (11, 0) fn hv_vcpu_get_vtimer_mask(vcpu: hv_vcpu_t, vtimer_is_masked: *mut bool) -> hv_return_t;
    /// Masks or unmasks the virtual timer. The framework masks it on every
    /// [`HV_EXIT_REASON_VTIMER_ACTIVATED`] exit; unmask it once the guest has
    /// serviced the timer interrupt.
    (11, 0) fn hv_vcpu_set_vtimer_mask(vcpu: hv_vcpu_t, vtimer_is_masked: bool) -> hv_return_t;
    /// Returns the virtual timer offset.
    (11, 0) fn hv_vcpu_get_vtimer_offset(vcpu: hv_vcpu_t, vtimer_offset: *mut u64) -> hv_return_t;
    /// Sets the virtual timer offset:
    /// `CNTVCT_EL0 = mach_absolute_time() - vtimer_offset`.
    (11, 0) fn hv_vcpu_set_vtimer_offset(vcpu: hv_vcpu_t, vtimer_offset: u64) -> hv_return_t;
    /// Performs a TLB invalidation operation for a vCPU. With EL2 enabled it
    /// acts on the guest hypervisor's entries, not the nested guest's.
    (27, 0) fn hv_vcpu_invalidate_tlb(vcpu: hv_vcpu_t, op: hv_tlbi_op_t, param: u64) -> hv_return_t;

    // hv_gic.h

    /// Creates the in-kernel GICv3 of the VM. Must be called after
    /// `hv_vm_create` and before any vCPU is created; only one GIC can exist.
    (15, 0) fn hv_gic_create(gic_config: hv_gic_config_t) -> hv_return_t;
    /// Sets the level of an SPI (a `true` level also signals an edge). Fails
    /// with [`HV_BAD_ARGUMENT`] outside the SPI range or inside the MSI range.
    (15, 0) fn hv_gic_set_spi(intid: u32, level: bool) -> hv_return_t;
    /// Sends an MSI. `address` is the guest physical address of the
    /// `GICM_SET_SPI_NSR` register of the MSI frame.
    (15, 0) fn hv_gic_send_msi(address: hv_ipa_t, intid: u32) -> hv_return_t;
    /// Reads a distributor register.
    (15, 0) fn hv_gic_get_distributor_reg(reg: hv_gic_distributor_reg_t, value: *mut u64) -> hv_return_t;
    /// Writes a distributor register.
    (15, 0) fn hv_gic_set_distributor_reg(reg: hv_gic_distributor_reg_t, value: u64) -> hv_return_t;
    /// Returns the guest physical base address of the redistributor of a
    /// vCPU. The vCPU's `MPIDR_EL1` affinity must have been set.
    (15, 0) fn hv_gic_get_redistributor_base(vcpu: hv_vcpu_t, redistributor_base_address: *mut hv_ipa_t) -> hv_return_t;
    /// Reads a redistributor register of a vCPU. Must be called by the
    /// owning thread.
    (15, 0) fn hv_gic_get_redistributor_reg(vcpu: hv_vcpu_t, reg: hv_gic_redistributor_reg_t, value: *mut u64) -> hv_return_t;
    /// Writes a redistributor register of a vCPU. Must be called by the
    /// owning thread.
    (15, 0) fn hv_gic_set_redistributor_reg(vcpu: hv_vcpu_t, reg: hv_gic_redistributor_reg_t, value: u64) -> hv_return_t;
    /// Reads a CPU interface (`ICC_*`) register of a vCPU.
    (15, 0) fn hv_gic_get_icc_reg(vcpu: hv_vcpu_t, reg: hv_gic_icc_reg_t, value: *mut u64) -> hv_return_t;
    /// Writes a CPU interface (`ICC_*`) register of a vCPU.
    (15, 0) fn hv_gic_set_icc_reg(vcpu: hv_vcpu_t, reg: hv_gic_icc_reg_t, value: u64) -> hv_return_t;
    /// Reads a virtualization control (`ICH_*`) register of a vCPU. Fails
    /// unless EL2 is enabled.
    (15, 0) fn hv_gic_get_ich_reg(vcpu: hv_vcpu_t, reg: hv_gic_ich_reg_t, value: *mut u64) -> hv_return_t;
    /// Writes a virtualization control (`ICH_*`) register of a vCPU. Fails
    /// unless EL2 is enabled.
    (15, 0) fn hv_gic_set_ich_reg(vcpu: hv_vcpu_t, reg: hv_gic_ich_reg_t, value: u64) -> hv_return_t;
    /// Reads a virtual CPU interface (`ICV_*`) register of a vCPU. Fails
    /// unless EL2 is enabled.
    (15, 0) fn hv_gic_get_icv_reg(vcpu: hv_vcpu_t, reg: hv_gic_icv_reg_t, value: *mut u64) -> hv_return_t;
    /// Writes a virtual CPU interface (`ICV_*`) register of a vCPU. Fails
    /// unless EL2 is enabled.
    (15, 0) fn hv_gic_set_icv_reg(vcpu: hv_vcpu_t, reg: hv_gic_icv_reg_t, value: u64) -> hv_return_t;
    /// Reads an MSI frame register.
    (15, 0) fn hv_gic_get_msi_reg(reg: hv_gic_msi_reg_t, value: *mut u64) -> hv_return_t;
    /// Writes an MSI frame register.
    (15, 0) fn hv_gic_set_msi_reg(reg: hv_gic_msi_reg_t, value: u64) -> hv_return_t;
    /// Restores GIC state saved with `hv_gic_state_get_data`. Must be called
    /// after the GIC and the vCPUs are created and before any vCPU runs.
    (15, 0) fn hv_gic_set_state(gic_state_data: *const c_void, gic_state_size: usize) -> hv_return_t;
    /// Resets the distributor, the redistributors and the internal state of
    /// the GIC.
    (15, 0) fn hv_gic_reset() -> hv_return_t;

    // hv_gic_config.h

    /// Creates a GIC configuration object, to be released with `os_release`.
    /// Must be called after `hv_vm_create`.
    (15, 0) fn hv_gic_config_create() -> hv_gic_config_t;
    /// Sets the guest physical base address of the distributor.
    (15, 0) fn hv_gic_config_set_distributor_base(config: hv_gic_config_t, distributor_base_address: hv_ipa_t) -> hv_return_t;
    /// Sets the guest physical base address of the redistributor region,
    /// which holds the redistributors of all supported vCPUs.
    (15, 0) fn hv_gic_config_set_redistributor_base(config: hv_gic_config_t, redistributor_base_address: hv_ipa_t) -> hv_return_t;
    /// Sets the guest physical base address of the MSI frame.
    (15, 0) fn hv_gic_config_set_msi_region_base(config: hv_gic_config_t, msi_region_base_address: hv_ipa_t) -> hv_return_t;
    /// Sets the range of interrupt IDs used for MSIs, which must lie inside
    /// the SPI range.
    (15, 0) fn hv_gic_config_set_msi_interrupt_range(config: hv_gic_config_t, msi_intid_base: u32, msi_intid_count: u32) -> hv_return_t;

    // hv_gic_parameters.h

    /// Returns the size in bytes of the distributor region.
    (15, 0) fn hv_gic_get_distributor_size(distributor_size: *mut usize) -> hv_return_t;
    /// Returns the required alignment in bytes of the distributor base.
    (15, 0) fn hv_gic_get_distributor_base_alignment(distributor_base_alignment: *mut usize) -> hv_return_t;
    /// Returns the total size in bytes of the redistributor region.
    (15, 0) fn hv_gic_get_redistributor_region_size(redistributor_region_size: *mut usize) -> hv_return_t;
    /// Returns the size in bytes of the redistributor of one vCPU.
    (15, 0) fn hv_gic_get_redistributor_size(redistributor_size: *mut usize) -> hv_return_t;
    /// Returns the required alignment in bytes of the redistributor region
    /// base.
    (15, 0) fn hv_gic_get_redistributor_base_alignment(redistributor_base_alignment: *mut usize) -> hv_return_t;
    /// Returns the size in bytes of the MSI frame.
    (15, 0) fn hv_gic_get_msi_region_size(msi_region_size: *mut usize) -> hv_return_t;
    /// Returns the required alignment in bytes of the MSI frame base.
    (15, 0) fn hv_gic_get_msi_region_base_alignment(msi_region_base_alignment: *mut usize) -> hv_return_t;
    /// Returns the first SPI interrupt ID and the number of SPIs.
    (15, 0) fn hv_gic_get_spi_interrupt_range(spi_intid_base: *mut u32, spi_intid_count: *mut u32) -> hv_return_t;
    /// Returns the interrupt ID the framework uses for a reserved interrupt
    /// source.
    (15, 0) fn hv_gic_get_intid(interrupt: hv_gic_intid_t, intid: *mut u32) -> hv_return_t;

    // hv_gic_state.h

    /// Creates a GIC state object holding a snapshot of the current GIC
    /// state, to be released with `os_release`. The VM must be stopped.
    /// Returns null if there is no GIC or the state cannot be captured.
    (15, 0) fn hv_gic_state_create() -> hv_gic_state_t;
    /// Returns the size in bytes of the serialized GIC state.
    (15, 0) fn hv_gic_state_get_size(state: hv_gic_state_t, gic_state_size: *mut usize) -> hv_return_t;
    /// Writes the serialized GIC state (which excludes the per-vCPU CPU
    /// interface registers) to `gic_state_data`, which must hold at least
    /// `hv_gic_state_get_size` bytes.
    (15, 0) fn hv_gic_state_get_data(state: hv_gic_state_t, gic_state_data: *mut c_void) -> hv_return_t;

    // hv_sme_config.h

    /// Returns the maximum streaming vector length in bytes. Returns
    /// [`HV_UNSUPPORTED`] if SME is not supported.
    (15, 2) fn hv_sme_config_get_max_svl_bytes(value: *mut usize) -> hv_return_t;
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use super::*;

    /// Serializes the unit tests that create the (single, per-process) VM.
    static VM_LOCK: Mutex<()> = Mutex::new(());

    /// Creates the process VM with the default configuration and destroys it
    /// when dropped.
    pub(crate) struct TestVm {
        _guard: MutexGuard<'static, ()>,
    }

    impl Drop for TestVm {
        fn drop(&mut self) {
            // SAFETY: the VM was created by `TestVm::new` and every vCPU of
            // the test has been destroyed before this runs.
            let ret = unsafe { hv_vm_destroy() };
            assert_eq!(ret, HV_SUCCESS, "hv_vm_destroy");
        }
    }

    impl TestVm {
        /// Returns `None` (after printing why) if Hypervisor.framework is not
        /// usable, for example because the test binary is not signed with the
        /// hypervisor entitlement. Panics instead if `HVF_REQUIRE=1`.
        pub(crate) fn new() -> Option<Self> {
            let guard = VM_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
            // SAFETY: a null configuration selects the defaults.
            let ret = unsafe { hv_vm_create(core::ptr::null_mut()) };
            match ret {
                HV_SUCCESS => Some(TestVm { _guard: guard }),
                HV_DENIED | HV_UNSUPPORTED | HV_NO_DEVICE
                    if std::env::var_os("HVF_REQUIRE").is_none_or(|v| v != "1") =>
                {
                    std::eprintln!("SKIP: hv_vm_create returned {:#x}", ret as u32);
                    None
                }
                _ => panic!("hv_vm_create returned {:#x}", ret as u32),
            }
        }
    }

    /// A vCPU owned by the current thread, destroyed when dropped.
    pub(crate) struct TestVcpu {
        pub(crate) id: hv_vcpu_t,
        pub(crate) exit: *mut hv_vcpu_exit_t,
    }

    impl TestVcpu {
        pub(crate) fn new(_vm: &TestVm) -> Self {
            let mut id = 0;
            let mut exit = core::ptr::null_mut();
            // SAFETY: both out pointers are valid for writes, and a null
            // configuration selects the defaults.
            let ret = unsafe { hv_vcpu_create(&mut id, &mut exit, core::ptr::null_mut()) };
            assert_eq!(ret, HV_SUCCESS, "hv_vcpu_create");
            assert!(!exit.is_null());
            TestVcpu { id, exit }
        }
    }

    impl Drop for TestVcpu {
        fn drop(&mut self) {
            // SAFETY: the vCPU was created on this thread (`TestVcpu` is not
            // `Send`, since it holds a raw pointer) and is destroyed once.
            let ret = unsafe { hv_vcpu_destroy(self.id) };
            assert_eq!(ret, HV_SUCCESS, "hv_vcpu_destroy");
        }
    }

    #[test]
    fn test_dyn_api_resolves_every_function() {
        let host = host_release();
        let mut seen = std::vec::Vec::new();
        let resolve = |name: &core::ffi::CStr| {
            // SAFETY: `name` is NUL-terminated and `RTLD_DEFAULT` is a valid
            // pseudo-handle.
            let ptr = unsafe { crate::os::dlsym(crate::os::RTLD_DEFAULT, name.as_ptr()) };
            seen.push((
                std::string::String::from(name.to_str().unwrap()),
                !ptr.is_null(),
            ));
            ptr
        };
        // SAFETY: same as `DynApi::load`, whose resolver this replicates.
        let api = unsafe { DynApi::load_with(resolve) };
        assert_eq!(
            std::format!("{api:?}"),
            std::format!("{:?}", DynApi::load())
        );

        // Every function newer than macOS 11.0 is looked up, and it is found
        // exactly when the running release is not older than its `since`.
        let newer: std::vec::Vec<_> = __FNS.iter().filter(|f| f.since != (11, 0)).collect();
        assert_eq!(seen.len(), newer.len());
        for (f, (name, found)) in newer.iter().zip(&seen) {
            assert_eq!(f.name, name);
            assert_eq!(
                *found,
                f.since <= host,
                "{} (since {:?}, host {:?})",
                f.name,
                f.since,
                host
            );
        }
    }

    /// Returns the (major, minor) release of the running macOS.
    fn host_release() -> (u16, u16) {
        let out = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .expect("sw_vers");
        let text = std::string::String::from_utf8(out.stdout).unwrap();
        let mut parts = text.trim().split('.').map(|p| p.parse::<u16>().unwrap());
        (parts.next().unwrap(), parts.next().unwrap_or(0))
    }

    #[test]
    fn test_vm_vcpu_lifecycle() {
        let Some(vm) = TestVm::new() else { return };

        let mut max = 0u32;
        // SAFETY: `max` is valid for writes.
        assert_eq!(unsafe { hv_vm_get_max_vcpu_count(&mut max) }, HV_SUCCESS);
        assert!(max >= 1);

        let vcpu = TestVcpu::new(&vm);

        // A second VM in the same process is refused.
        // SAFETY: a null configuration selects the defaults.
        assert_ne!(unsafe { hv_vm_create(core::ptr::null_mut()) }, HV_SUCCESS);

        // Register round trips through the hard-linked functions.
        // SAFETY: the vCPU is owned by this thread, every out pointer is
        // valid for writes, and `vcpu.exit` stays valid until the vCPU is
        // destroyed.
        unsafe {
            assert_eq!(hv_vcpu_set_reg(vcpu.id, HV_REG_X0, 0x1234_5678), HV_SUCCESS);
            let mut v = 0;
            assert_eq!(hv_vcpu_get_reg(vcpu.id, HV_REG_X0, &mut v), HV_SUCCESS);
            assert_eq!(v, 0x1234_5678);

            assert_eq!(
                hv_vcpu_set_sys_reg(vcpu.id, HV_SYS_REG_VBAR_EL1, 0x8_0000),
                HV_SUCCESS
            );
            assert_eq!(
                hv_vcpu_get_sys_reg(vcpu.id, HV_SYS_REG_VBAR_EL1, &mut v),
                HV_SUCCESS
            );
            assert_eq!(v, 0x8_0000);

            // An invalid register is rejected by the framework.
            assert_eq!(hv_vcpu_get_reg(vcpu.id, 0xffff, &mut v), HV_BAD_ARGUMENT);

            let mut masked = false;
            assert_eq!(hv_vcpu_set_vtimer_mask(vcpu.id, true), HV_SUCCESS);
            assert_eq!(hv_vcpu_get_vtimer_mask(vcpu.id, &mut masked), HV_SUCCESS);
            assert!(masked);

            // A kick before running makes the next run return immediately.
            let mut id = vcpu.id;
            assert_eq!(hv_vcpus_exit(&mut id, 1), HV_SUCCESS);
            assert_eq!(hv_vcpu_run(vcpu.id), HV_SUCCESS);
            assert_eq!((*vcpu.exit).reason, HV_EXIT_REASON_CANCELED);
        }

        // A newer function, called through the run-time resolved table.
        let api = DynApi::load();
        if let Some(get_max_ipa) = api.hv_vm_config_get_max_ipa_size {
            let mut bits = 0;
            // SAFETY: `bits` is valid for writes.
            assert_eq!(unsafe { get_max_ipa(&mut bits) }, HV_SUCCESS);
            assert!((32..=52).contains(&bits), "{bits}");
        }

        drop(vcpu);
        drop(vm);
    }

    #[test]
    fn test_vcpu_config_feature_regs() {
        // SAFETY: no preconditions.
        let config = unsafe { hv_vcpu_config_create() };
        assert!(!config.is_null());
        let mut pfr0 = 0;
        let mut ccsidr = [0u64; 8];
        // SAFETY: `config` is a live configuration object and the out
        // pointers are valid for writes (`ccsidr` holds 8 values).
        unsafe {
            assert_eq!(
                hv_vcpu_config_get_feature_reg(config, HV_FEATURE_REG_ID_AA64PFR0_EL1, &mut pfr0),
                HV_SUCCESS
            );
            assert_eq!(
                hv_vcpu_config_get_ccsidr_el1_sys_reg_values(
                    config,
                    HV_CACHE_TYPE_DATA,
                    ccsidr.as_mut_ptr()
                ),
                HV_SUCCESS
            );
            crate::os::os_release(config.cast());
        }
        // EL0 and EL1 are implemented (AArch64 only or AArch64 + AArch32).
        assert_ne!(pfr0 & 0xff, 0);
        assert_ne!(ccsidr[0], 0);
    }
}
