// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Base types, return codes and the vCPU exit structures
//! (`hv_kern_types.h`, `hv_vcpu_types.h`, `hv_vm_types.h`).

use core::marker::{PhantomData, PhantomPinned};

/// Declares an opaque `OS_OBJECT` struct and its pointer typedef.
macro_rules! os_object {
    ($(#[$m:meta])* $s:ident, $t:ident) => {
        $(#[$m])*
        ///
        /// Opaque; only ever used behind a pointer.
        #[repr(C)]
        #[derive(Debug)]
        pub struct $s {
            _p: [u8; 0],
            _m: PhantomData<(*mut u8, PhantomPinned)>,
        }

        #[doc = concat!("Pointer to [`", stringify!($s), "`] (an `os_object`, released with `os_release`).")]
        pub type $t = *mut $s;
    };
}

os_object!(
    /// VM configuration object.
    hv_vm_config_s,
    hv_vm_config_t
);
os_object!(
    /// vCPU configuration object.
    hv_vcpu_config_s,
    hv_vcpu_config_t
);
os_object!(
    /// GIC configuration object.
    hv_gic_config_s,
    hv_gic_config_t
);
os_object!(
    /// GIC state object.
    hv_gic_state_s,
    hv_gic_state_t
);

/// Guest physical (intermediate physical) address.
pub type hv_ipa_t = u64;

/// vCPU instance identifier.
pub type hv_vcpu_t = u64;

/// Exception syndrome (the `ESR_EL2` value of a vCPU exit).
pub type hv_exception_syndrome_t = u64;

/// Exception address (the `FAR_EL2` value of a vCPU exit).
pub type hv_exception_address_t = u64;

hv_consts! {
    /// Return value of every Hypervisor.framework function (`mach_error_t`).
    pub type hv_return_t = i32 {
        /// The operation completed successfully.
        HV_SUCCESS = 0,
        /// The operation was unsuccessful.
        HV_ERROR = 0xfae9_4001_u32 as i32,
        /// The operation was unsuccessful because the owning resource was busy.
        HV_BUSY = 0xfae9_4002_u32 as i32,
        /// The operation was not successful because the function call had an
        /// invalid argument.
        HV_BAD_ARGUMENT = 0xfae9_4003_u32 as i32,
        /// The guest is in an illegal state.
        HV_ILLEGAL_GUEST_STATE = 0xfae9_4004_u32 as i32,
        /// The operation was unsuccessful because the host had no resources
        /// available to complete the request.
        HV_NO_RESOURCES = 0xfae9_4005_u32 as i32,
        /// The operation was unsuccessful because no VM or vCPU was available.
        HV_NO_DEVICE = 0xfae9_4006_u32 as i32,
        /// The system didn't allow the requested operation (usually a missing
        /// `com.apple.security.hypervisor` entitlement).
        HV_DENIED = 0xfae9_4007_u32 as i32,
        /// The operation was unsuccessful because the object already exists.
        HV_EXISTS = 0xfae9_4008_u32 as i32,
        /// The operation requested isn't supported by the hypervisor.
        HV_UNSUPPORTED = 0xfae9_400f_u32 as i32,
    }

    /// Guest memory permissions for `hv_vm_map` and `hv_vm_protect`.
    pub type hv_memory_flags_t = u64 {
        /// The guest can read from the region.
        HV_MEMORY_READ = 1 << 0,
        /// The guest can write to the region.
        HV_MEMORY_WRITE = 1 << 1,
        /// The guest can execute code from the region.
        HV_MEMORY_EXEC = 1 << 2,
    }

    /// Flags for `hv_vm_allocate`.
    pub type hv_allocate_flags_t = u64 {
        /// Default allocation flags.
        @since(12, 1) HV_ALLOCATE_DEFAULT = 0,
    }

    /// Reason for a vCPU exit.
    pub type hv_exit_reason_t = u32 {
        /// The vCPU exit was requested by `hv_vcpus_exit`.
        HV_EXIT_REASON_CANCELED = 0,
        /// The vCPU took a synchronous exception to EL2.
        HV_EXIT_REASON_EXCEPTION = 1,
        /// The virtual timer fired; the framework masked it.
        HV_EXIT_REASON_VTIMER_ACTIVATED = 2,
        /// The vCPU exited for an unexpected reason.
        HV_EXIT_REASON_UNKNOWN = 3,
    }

    /// Interrupt line of a vCPU.
    pub type hv_interrupt_type_t = u32 {
        /// The IRQ line.
        HV_INTERRUPT_TYPE_IRQ = 0,
        /// The FIQ line.
        HV_INTERRUPT_TYPE_FIQ = 1,
    }

    /// Stage-2 translation granule of a VM.
    pub type hv_ipa_granule_t = u32 {
        /// 4 KiB granule.
        @since(26, 0) HV_IPA_GRANULE_4KB = 0,
        /// 16 KiB granule (the default).
        @since(26, 0) HV_IPA_GRANULE_16KB = 1,
    }

    /// Cache type for `hv_vcpu_config_get_ccsidr_el1_sys_reg_values`.
    pub type hv_cache_type_t = u32 {
        /// Data cache.
        HV_CACHE_TYPE_DATA = 0,
        /// Instruction cache.
        HV_CACHE_TYPE_INSTRUCTION = 1,
    }

    /// Feature register readable from a vCPU configuration. The values are
    /// in [`regs`](crate::regs).
    pub type hv_feature_reg_t = u32 {}

    /// TLB invalidation operation for `hv_vcpu_invalidate_tlb`.
    pub type hv_tlbi_op_t = u16 {
        /// `TLBI RVAE1IS`.
        @since(27, 0) HV_TLBI_OP_RVAE1IS = 0x11,
        /// `TLBI RVAAE1IS`.
        @since(27, 0) HV_TLBI_OP_RVAAE1IS = 0x13,
        /// `TLBI RVALE1IS`.
        @since(27, 0) HV_TLBI_OP_RVALE1IS = 0x15,
        /// `TLBI RVAALE1IS`.
        @since(27, 0) HV_TLBI_OP_RVAALE1IS = 0x17,
        /// `TLBI VMALLE1IS`.
        @since(27, 0) HV_TLBI_OP_VMALLE1IS = 0x18,
        /// `TLBI VAE1IS`.
        @since(27, 0) HV_TLBI_OP_VAE1IS = 0x19,
        /// `TLBI ASIDE1IS`.
        @since(27, 0) HV_TLBI_OP_ASIDE1IS = 0x1a,
        /// `TLBI VAAE1IS`.
        @since(27, 0) HV_TLBI_OP_VAAE1IS = 0x1b,
        /// `TLBI VALE1IS`.
        @since(27, 0) HV_TLBI_OP_VALE1IS = 0x1d,
        /// `TLBI VAALE1IS`.
        @since(27, 0) HV_TLBI_OP_VAALE1IS = 0x1f,
    }
}

/// Details of a synchronous exception exit.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct hv_vcpu_exit_exception_t {
    /// Exception syndrome (`ESR_EL2`).
    pub syndrome: hv_exception_syndrome_t,
    /// Faulting virtual address (`FAR_EL2`).
    pub virtual_address: hv_exception_address_t,
    /// Faulting guest physical address (derived from `HPFAR_EL2`).
    pub physical_address: hv_ipa_t,
}

/// Information about a vCPU exit, written by the framework on every
/// `hv_vcpu_run`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct hv_vcpu_exit_t {
    /// Exit reason, one of `HV_EXIT_REASON_*`.
    pub reason: hv_exit_reason_t,
    /// Exception details, valid when `reason` is
    /// [`HV_EXIT_REASON_EXCEPTION`].
    pub exception: hv_vcpu_exit_exception_t,
}

/// A 128-bit SIMD/FP register value (`ext_vector_type(16) uint8_t`).
///
/// Memory layout only: the C setter takes this type by value in a vector
/// register, which Rust cannot express on stable. Use
/// `hv_vcpu_set_simd_fp_reg_by_ref` to set it.
#[repr(C, align(16))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct hv_simd_fp_uchar16_t(pub [u8; 16]);

/// The 512-bit SME `ZT0` register value (`ext_vector_type(64) uint8_t`),
/// only ever passed by pointer.
///
/// Like the C type, it is 64 bytes long but only 16-byte aligned (the
/// largest vector alignment of AAPCS64).
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct hv_sme_zt0_uchar64_t(pub [u8; 64]);

impl Default for hv_sme_zt0_uchar64_t {
    fn default() -> Self {
        Self([0; 64])
    }
}

/// SME state of a vCPU (macOS 15.2).
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct hv_vcpu_sme_state_t {
    /// `PSTATE.SM`: streaming SVE mode is enabled.
    pub streaming_sve_mode_enabled: bool,
    /// `PSTATE.ZA`: ZA storage is enabled.
    pub za_storage_enabled: bool,
}
