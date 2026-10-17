// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Host capability probes, VM creation and vCPU configuration.

use std::mem;
use std::ptr;

use hvf_bindings::{
    HV_IPA_GRANULE_4KB, HV_IPA_GRANULE_16KB, HV_UNSUPPORTED, hv_ipa_granule_t, hv_return_t,
    hv_vcpu_config_create, hv_vcpu_config_get_ccsidr_el1_sys_reg_values,
    hv_vcpu_config_get_feature_reg, hv_vcpu_config_s, hv_vm_config_s, hv_vm_create,
    hv_vm_get_max_vcpu_count,
};

use crate::avail::api;
use crate::error::{Error, Result, check};
use crate::gic_types::{GicIntid, GicParams};
use crate::os_object::OsObject;
use crate::regs::{CacheType, FeatureReg};
use crate::vm::{Vm, VmSlot};

/// The IPA size of VMs on releases without `hv_vm_config_get_default_ipa_size`
/// (macOS 11 and 12), which do not support configuring it.
const LEGACY_IPA_BITS: u32 = 36;

/// Calls `f` with a pointer to a default-initialized `T` that it fills in,
/// and returns the value if `f` returns `HV_SUCCESS`. `call` names the C
/// function for the error.
pub(crate) fn out_param<T: Default>(
    call: &'static str,
    f: impl FnOnce(*mut T) -> hv_return_t,
) -> Result<T> {
    let mut value = T::default();
    check(call, f(&mut value))?;
    Ok(value)
}

/// Replaces an [`Error::Unavailable`] result by `default`.
fn or_if_unavailable<T>(r: Result<T>, default: T) -> Result<T> {
    match r {
        Err(Error::Unavailable { .. }) => Ok(default),
        r => r,
    }
}

/// Entry point of the crate: host probes and VM creation.
///
/// Every method states the macOS release that introduced the underlying
/// framework function. On older releases, methods using newer functions return
/// [`Error::Unavailable`](crate::Error::Unavailable).
///
/// The probes neither need the `com.apple.security.hypervisor` entitlement nor
/// an existing VM; only [`create_vm`](Self::create_vm) does.
#[derive(Debug, Clone, Copy)]
pub struct Hvf;

impl Hvf {
    /// Whether the host supports Hypervisor.framework (`kern.hv_support`).
    ///
    /// This is `false` on Intel Macs without VT-x and inside virtual machines
    /// without nested virtualization. It does not check the entitlement of the
    /// running binary.
    pub fn is_supported() -> bool {
        let mut value: libc::c_int = 0;
        let mut len = mem::size_of_val(&value);
        // SAFETY: the name is a NUL-terminated string, `value` is valid for
        // writes of `len` bytes, and no new value is set.
        let ret = unsafe {
            libc::sysctlbyname(
                c"kern.hv_support".as_ptr(),
                (&raw mut value).cast(),
                &mut len,
                ptr::null_mut(),
                0,
            )
        };
        ret == 0 && len == mem::size_of_val(&value) && value == 1
    }

    /// The host page size in bytes (16 KiB on Apple silicon).
    ///
    /// Host addresses passed to [`Vm::map`] must be aligned to it with the
    /// default 16 KiB IPA granule.
    pub fn host_page_size() -> usize {
        // SAFETY: `sysconf` has no preconditions.
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        usize::try_from(size).expect("sysconf(_SC_PAGESIZE) failed")
    }

    /// The maximum number of vCPUs per VM (macOS 11.0).
    pub fn max_vcpu_count() -> Result<u32> {
        // SAFETY: the pointer is valid for writing a `u32`.
        out_param("hv_vm_get_max_vcpu_count", |p| unsafe {
            hv_vm_get_max_vcpu_count(p)
        })
    }

    /// The maximum supported IPA size in bits (macOS 13.0).
    pub fn max_ipa_bits() -> Result<u32> {
        let f = need!(hv_vm_config_get_max_ipa_size);
        // SAFETY: the pointer is valid for writing a `u32`.
        out_param("hv_vm_config_get_max_ipa_size", |p| unsafe { f(p) })
    }

    /// The default IPA size in bits (macOS 13.0). Older releases always use
    /// 36 bits.
    pub fn default_ipa_bits() -> Result<u32> {
        let f = need!(hv_vm_config_get_default_ipa_size);
        // SAFETY: the pointer is valid for writing a `u32`.
        out_param("hv_vm_config_get_default_ipa_size", |p| unsafe { f(p) })
    }

    /// Whether VMs can enable EL2, that is nested virtualization (macOS 15.0).
    pub fn el2_supported() -> Result<bool> {
        let f = need!(hv_vm_config_get_el2_supported);
        // SAFETY: the pointer is valid for writing a `bool`.
        out_param("hv_vm_config_get_el2_supported", |p| unsafe { f(p) })
    }

    /// The default stage-2 translation granule (macOS 26.0). Older releases
    /// always use 16 KiB.
    pub fn default_ipa_granule() -> Result<IpaGranule> {
        let f = need!(hv_vm_config_get_default_ipa_granule);
        // SAFETY: the pointer is valid for writing a `hv_ipa_granule_t`.
        let raw = out_param("hv_vm_config_get_default_ipa_granule", |p| unsafe { f(p) })?;
        IpaGranule::from_raw(raw)
    }

    /// The maximum SME streaming vector length in bytes, or `None` if the
    /// host has no SME (macOS 15.2).
    pub fn sme_max_svl_bytes() -> Result<Option<usize>> {
        let f = need!(hv_sme_config_get_max_svl_bytes);
        let mut value = 0usize;
        // SAFETY: the pointer is valid for writing a `usize`.
        let ret = unsafe { f(&mut value) };
        if ret == HV_UNSUPPORTED {
            return Ok(None);
        }
        check("hv_sme_config_get_max_svl_bytes", ret)?;
        Ok(Some(value))
    }

    /// Sizes, alignments and interrupt ranges of the in-kernel GIC
    /// (macOS 15.0).
    pub fn gic_params() -> Result<GicParams> {
        /// Reads one `size_t` parameter.
        fn size(
            call: &'static str,
            f: unsafe extern "C" fn(*mut usize) -> hv_return_t,
        ) -> Result<u64> {
            // SAFETY: `f` is one of the `hv_gic_get_*` parameter getters,
            // which write one `size_t` through the valid pointer.
            let value = out_param(call, |p| unsafe { f(p) })?;
            Ok(value as u64)
        }

        let spi_range = need!(hv_gic_get_spi_interrupt_range);
        let mut spi_base = 0u32;
        let mut spi_count = 0u32;
        // SAFETY: both pointers are valid for writing a `u32`.
        let ret = unsafe { spi_range(&mut spi_base, &mut spi_count) };
        check("hv_gic_get_spi_interrupt_range", ret)?;

        Ok(GicParams {
            distributor_size: size(
                "hv_gic_get_distributor_size",
                need!(hv_gic_get_distributor_size),
            )?,
            distributor_base_alignment: size(
                "hv_gic_get_distributor_base_alignment",
                need!(hv_gic_get_distributor_base_alignment),
            )?,
            redistributor_region_size: size(
                "hv_gic_get_redistributor_region_size",
                need!(hv_gic_get_redistributor_region_size),
            )?,
            redistributor_size: size(
                "hv_gic_get_redistributor_size",
                need!(hv_gic_get_redistributor_size),
            )?,
            redistributor_base_alignment: size(
                "hv_gic_get_redistributor_base_alignment",
                need!(hv_gic_get_redistributor_base_alignment),
            )?,
            msi_region_size: size(
                "hv_gic_get_msi_region_size",
                need!(hv_gic_get_msi_region_size),
            )?,
            msi_region_base_alignment: size(
                "hv_gic_get_msi_region_base_alignment",
                need!(hv_gic_get_msi_region_base_alignment),
            )?,
            spi_base,
            spi_count,
        })
    }

    /// The interrupt id the in-kernel GIC uses for `intid` (macOS 15.0).
    pub fn gic_intid(intid: GicIntid) -> Result<u32> {
        let f = need!(hv_gic_get_intid);
        // SAFETY: the pointer is valid for writing a `u32`.
        out_param("hv_gic_get_intid", |p| unsafe { f(intid.raw(), p) })
    }

    /// Creates the VM of this process.
    ///
    /// Hypervisor.framework supports a single VM per process: this fails with
    /// [`Error::VmExists`](crate::Error::VmExists) while another [`Vm`] (or
    /// any vCPU or GIC handle keeping it alive) exists.
    ///
    /// A default `cfg` uses `hv_vm_create(NULL)` (macOS 11.0). Any other
    /// setting needs `hv_vm_config_create` (macOS 13.0), EL2 needs macOS 15.0
    /// and the IPA granule macOS 26.0. The effective settings are available
    /// from [`Vm::ipa_granule`], [`Vm::ipa_bits`] and [`Vm::el2_enabled`].
    ///
    /// The binary must be signed with the `com.apple.security.hypervisor`
    /// entitlement, otherwise this fails with `HV_DENIED`
    /// ([`Error::is_unavailable`](crate::Error::is_unavailable)).
    ///
    /// If the previous VM could not be destroyed when its last handle was
    /// dropped (for example because a vCPU leaked on an exiting thread was
    /// still being torn down), the destroy is retried first and its error is
    /// returned if it still fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use hvf::{Hvf, VmConfig};
    /// let vm = Hvf::create_vm(&VmConfig {
    ///     ipa_bits: Some(Hvf::max_ipa_bits().unwrap()),
    ///     ..Default::default()
    /// })
    /// .unwrap();
    /// assert_eq!(vm.ipa_bits(), Hvf::max_ipa_bits().unwrap());
    /// ```
    pub fn create_vm(cfg: &VmConfig) -> Result<Vm> {
        let slot = VmSlot::claim()?;

        let effective = if *cfg == VmConfig::default() {
            // SAFETY: a null configuration selects the default settings.
            check("hv_vm_create", unsafe { hv_vm_create(ptr::null_mut()) })?;
            default_vm_settings()
        } else {
            let config = vm_config(cfg)?;
            // SAFETY: `config` is a live VM configuration.
            check("hv_vm_create", unsafe { hv_vm_create(config.as_ptr()) })?;
            vm_config_settings(&config)
        };

        match effective {
            Ok((granule, ipa_bits, el2)) => Ok(slot.into_vm(granule.bytes(), ipa_bits, el2)),
            Err(e) => {
                slot.destroy_created();
                Err(e)
            }
        }
    }
}

/// The effective `(granule, ipa_bits, el2)` of a VM created with the default
/// configuration.
fn default_vm_settings() -> Result<(IpaGranule, u32, bool)> {
    Ok((
        or_if_unavailable(Hvf::default_ipa_granule(), IpaGranule::K16)?,
        or_if_unavailable(Hvf::default_ipa_bits(), LEGACY_IPA_BITS)?,
        false,
    ))
}

/// Creates a VM configuration object with the settings of `cfg`.
fn vm_config(cfg: &VmConfig) -> Result<OsObject<hv_vm_config_s>> {
    let create = need!(hv_vm_config_create);
    // SAFETY: `hv_vm_config_create` has no preconditions and returns an owned
    // reference (or null), which `from_raw` takes over.
    let config =
        unsafe { OsObject::from_raw(create()) }.ok_or(Error::NullObject("hv_vm_config_create"))?;

    if let Some(bits) = cfg.ipa_bits {
        let set = need!(hv_vm_config_set_ipa_size);
        // SAFETY: `config` is a live VM configuration.
        let ret = unsafe { set(config.as_ptr(), bits) };
        check("hv_vm_config_set_ipa_size", ret)?;
    }
    if cfg.el2 {
        let set = need!(hv_vm_config_set_el2_enabled);
        // SAFETY: `config` is a live VM configuration.
        let ret = unsafe { set(config.as_ptr(), true) };
        check("hv_vm_config_set_el2_enabled", ret)?;
    }
    if let Some(granule) = cfg.ipa_granule {
        let set = need!(hv_vm_config_set_ipa_granule);
        // SAFETY: `config` is a live VM configuration.
        let ret = unsafe { set(config.as_ptr(), granule.raw()) };
        check("hv_vm_config_set_ipa_granule", ret)?;
    }
    Ok(config)
}

/// The effective `(granule, ipa_bits, el2)` of a VM created with `config`.
///
/// Settings whose getter the running release lacks cannot have been changed,
/// so they have their only possible value.
fn vm_config_settings(config: &OsObject<hv_vm_config_s>) -> Result<(IpaGranule, u32, bool)> {
    let api = api();
    let config = config.as_ptr();

    let get_ipa_size = need!(hv_vm_config_get_ipa_size);
    // SAFETY: `config` is a live VM configuration and the pointer is valid
    // for writing a `u32`.
    let ipa_bits = out_param("hv_vm_config_get_ipa_size", |p| unsafe {
        get_ipa_size(config, p)
    })?;

    let el2 = match api.hv_vm_config_get_el2_enabled {
        // SAFETY: as above, for a `bool`.
        Some(get) => out_param("hv_vm_config_get_el2_enabled", |p| unsafe {
            get(config, p)
        })?,
        None => false,
    };

    let granule = match api.hv_vm_config_get_ipa_granule {
        Some(get) => {
            // SAFETY: as above, for a `hv_ipa_granule_t`.
            let raw = out_param("hv_vm_config_get_ipa_granule", |p| unsafe {
                get(config, p)
            })?;
            IpaGranule::from_raw(raw)?
        }
        None => IpaGranule::K16,
    };

    Ok((granule, ipa_bits, el2))
}

/// VM creation options. The default leaves every setting to the framework.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VmConfig {
    /// IPA size in bits, at most [`Hvf::max_ipa_bits`]. `None` keeps the
    /// default ([`Hvf::default_ipa_bits`]).
    pub ipa_bits: Option<u32>,
    /// Enables EL2 (nested virtualization) in the guest. Needs
    /// [`Hvf::el2_supported`].
    pub el2: bool,
    /// Stage-2 translation granule. `None` keeps the default
    /// ([`Hvf::default_ipa_granule`], 16 KiB).
    pub ipa_granule: Option<IpaGranule>,
}

/// Stage-2 translation granule of a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpaGranule {
    /// 4 KiB pages (macOS 26.0).
    K4,
    /// 16 KiB pages, the default.
    K16,
}

impl IpaGranule {
    /// The granule size in bytes.
    pub fn bytes(self) -> u64 {
        match self {
            IpaGranule::K4 => 0x1000,
            IpaGranule::K16 => 0x4000,
        }
    }

    /// The corresponding `hv_ipa_granule_t` value.
    fn raw(self) -> hv_ipa_granule_t {
        match self {
            IpaGranule::K4 => HV_IPA_GRANULE_4KB,
            IpaGranule::K16 => HV_IPA_GRANULE_16KB,
        }
    }

    /// Converts a `hv_ipa_granule_t` returned by the framework.
    fn from_raw(raw: hv_ipa_granule_t) -> Result<IpaGranule> {
        match raw {
            HV_IPA_GRANULE_4KB => Ok(IpaGranule::K4),
            HV_IPA_GRANULE_16KB => Ok(IpaGranule::K16),
            _ => Err(Error::OutOfRange {
                what: "hv_ipa_granule_t",
                value: u64::from(raw),
                limit: u64::from(HV_IPA_GRANULE_16KB) + 1,
            }),
        }
    }
}

/// A vCPU configuration, used to read the feature registers a vCPU will
/// report and to create vCPUs with [`Vm::create_vcpu`].
///
/// Like [`Vcpu`](crate::Vcpu) it is neither `Send` nor `Sync`:
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<hvf::VcpuConfig>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<hvf::VcpuConfig>();
/// ```
#[derive(Debug)]
pub struct VcpuConfig(pub(crate) OsObject<hv_vcpu_config_s>);

impl VcpuConfig {
    /// Creates a default vCPU configuration (macOS 11.0).
    pub fn new() -> Result<Self> {
        // SAFETY: `hv_vcpu_config_create` has no preconditions and returns an
        // owned reference (or null), which `from_raw` takes over.
        let obj = unsafe { OsObject::from_raw(hv_vcpu_config_create()) }
            .ok_or(Error::NullObject("hv_vcpu_config_create"))?;
        Ok(VcpuConfig(obj))
    }

    /// The value of a feature register for vCPUs created with this
    /// configuration (macOS 11.0; some registers need newer releases).
    pub fn feature_reg(&self, reg: FeatureReg) -> Result<u64> {
        let config = self.0.as_ptr();
        // SAFETY: `config` is a live vCPU configuration and the pointer is
        // valid for writing a `u64`.
        out_param("hv_vcpu_config_get_feature_reg", |p| unsafe {
            hv_vcpu_config_get_feature_reg(config, reg.0, p)
        })
    }

    /// The eight `CCSIDR_EL1` values for cache type `cache` (macOS 11.0),
    /// one per cache level as selected by `CSSELR_EL1`.
    pub fn ccsidr_el1_values(&self, cache: CacheType) -> Result<[u64; 8]> {
        let config = self.0.as_ptr();
        // SAFETY: `config` is a live vCPU configuration and the pointer is
        // valid for writing the eight `u64` values the function stores.
        out_param(
            "hv_vcpu_config_get_ccsidr_el1_sys_reg_values",
            |p: *mut [u64; 8]| unsafe {
                hv_vcpu_config_get_ccsidr_el1_sys_reg_values(config, cache as u32, p.cast())
            },
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use std::sync::{Mutex, MutexGuard, PoisonError};

    /// Serializes the unit tests that create a VM (one VM per process) and
    /// skips them when the hypervisor is unusable, unless `HVF_REQUIRE=1`.
    pub(crate) fn hv_test_lock() -> Option<MutexGuard<'static, ()>> {
        static LOCK: Mutex<()> = Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let require = std::env::var_os("HVF_REQUIRE").is_some_and(|v| v == "1");
        let reason = if !Hvf::is_supported() {
            "kern.hv_support is not 1".to_string()
        } else {
            match Hvf::create_vm(&VmConfig::default()) {
                Ok(_) => return Some(guard),
                Err(e) if e.is_unavailable() => e.to_string(),
                Err(e) => panic!("cannot create a VM: {e}"),
            }
        };
        assert!(!require, "HVF_REQUIRE=1 but HVF is unusable: {reason}");
        println!("SKIP: Hypervisor.framework is unusable: {reason}");
        None
    }

    /// Whether the host supports Hypervisor.framework, for the tests that
    /// need neither the entitlement nor a VM. Prints `SKIP` and returns
    /// `false` on hosts without it (such as macOS VMs without nested
    /// virtualization), unless `HVF_REQUIRE=1`.
    fn hv_supported() -> bool {
        if Hvf::is_supported() {
            return true;
        }
        let require = std::env::var_os("HVF_REQUIRE").is_some_and(|v| v == "1");
        assert!(!require, "HVF_REQUIRE=1 but kern.hv_support is not 1");
        println!("SKIP: Hypervisor.framework is unusable: kern.hv_support is not 1");
        false
    }

    #[test]
    fn test_ipa_granule() {
        assert_eq!(IpaGranule::K4.bytes(), 4096);
        assert_eq!(IpaGranule::K16.bytes(), 16384);
        for g in [IpaGranule::K4, IpaGranule::K16] {
            assert_eq!(IpaGranule::from_raw(g.raw()), Ok(g));
        }
        assert!(matches!(
            IpaGranule::from_raw(7),
            Err(Error::OutOfRange { value: 7, .. })
        ));
    }

    #[test]
    fn test_or_if_unavailable() {
        let unavailable = Err(Error::Unavailable {
            api: "x",
            since: (99, 0),
        });
        assert_eq!(or_if_unavailable(unavailable, 3), Ok(3));
        assert_eq!(or_if_unavailable(Ok(1), 3), Ok(1));
        assert_eq!(
            or_if_unavailable::<u32>(Err(Error::VmExists), 3),
            Err(Error::VmExists)
        );
    }

    #[test]
    fn test_probes() {
        // None of these need the entitlement or a VM, but all except the page
        // size need a host that supports the framework.
        assert!(Hvf::host_page_size().is_power_of_two());
        assert!(Hvf::host_page_size() >= 4096);
        if !hv_supported() {
            return;
        }
        assert!(Hvf::max_vcpu_count().unwrap() >= 1);

        let max = Hvf::max_ipa_bits().unwrap();
        let default = Hvf::default_ipa_bits().unwrap();
        assert!((32..=52).contains(&max), "{max}");
        assert!(default <= max, "{default} > {max}");

        // Newer probes either work or report the release they need.
        match Hvf::el2_supported() {
            Ok(_) | Err(Error::Unavailable { since: (15, 0), .. }) => {}
            Err(e) => panic!("{e}"),
        }
        match Hvf::default_ipa_granule() {
            Ok(_) | Err(Error::Unavailable { since: (26, 0), .. }) => {}
            Err(e) => panic!("{e}"),
        }
        match Hvf::sme_max_svl_bytes() {
            Ok(Some(svl)) => assert!(svl.is_power_of_two(), "{svl}"),
            Ok(None) | Err(Error::Unavailable { since: (15, 2), .. }) => {}
            Err(e) => panic!("{e}"),
        }
        match Hvf::gic_params() {
            Ok(params) => {
                assert!(params.distributor_size > 0);
                assert!(params.redistributor_size > 0);
                assert!(params.redistributor_region_size >= params.redistributor_size);
                assert!(params.spi_base >= 32);
                assert!(params.spi_count > 0);
            }
            Err(Error::Unavailable { since: (15, 0), .. }) => {}
            Err(e) => panic!("{e}"),
        }
        match Hvf::gic_intid(GicIntid::El1VirtualTimer) {
            Ok(intid) => assert_eq!(intid, 27),
            Err(Error::Unavailable { since: (15, 0), .. }) => {}
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn test_vcpu_config() {
        if !hv_supported() {
            return;
        }
        let cfg = VcpuConfig::new().unwrap();
        assert_ne!(cfg.feature_reg(FeatureReg::ID_AA64PFR0_EL1).unwrap(), 0);
        assert_ne!(cfg.feature_reg(FeatureReg::ID_AA64ISAR0_EL1).unwrap(), 0);
        let e = cfg.feature_reg(FeatureReg(0xffff)).unwrap_err();
        assert!(
            matches!(
                e,
                Error::Hv {
                    call: "hv_vcpu_config_get_feature_reg",
                    ..
                }
            ),
            "{e:?}"
        );
        let data = cfg.ccsidr_el1_values(CacheType::Data).unwrap();
        let inst = cfg.ccsidr_el1_values(CacheType::Instruction).unwrap();
        // The L1 caches exist.
        assert_ne!(data[0], 0);
        assert_ne!(inst[0], 0);
        assert!(format!("{cfg:?}").starts_with("VcpuConfig(OsObject("));
    }

    #[test]
    fn test_create_vm_default() {
        let Some(_g) = hv_test_lock() else { return };
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        assert_eq!(vm.ipa_bits(), Hvf::default_ipa_bits().unwrap());
        let granule = or_if_unavailable(Hvf::default_ipa_granule(), IpaGranule::K16).unwrap();
        assert_eq!(vm.ipa_granule(), granule.bytes());
        assert!(!vm.el2_enabled());
        assert_eq!(
            Hvf::create_vm(&VmConfig::default()).unwrap_err(),
            Error::VmExists
        );
        drop(vm);
        // Dropping the VM frees the slot.
        drop(Hvf::create_vm(&VmConfig::default()).unwrap());
    }

    #[test]
    fn test_create_vm_config() {
        let Some(_g) = hv_test_lock() else { return };
        let max = Hvf::max_ipa_bits().unwrap();

        let cfg = VmConfig {
            ipa_bits: Some(max),
            ..Default::default()
        };
        let vm = Hvf::create_vm(&cfg).unwrap();
        assert_eq!(vm.ipa_bits(), max);
        assert_eq!(vm.ipa_granule(), IpaGranule::K16.bytes());
        drop(vm);

        // An IPA size beyond the maximum is rejected by the setter, and the
        // slot is released.
        let cfg = VmConfig {
            ipa_bits: Some(max + 1),
            ..Default::default()
        };
        let e = Hvf::create_vm(&cfg).unwrap_err();
        assert!(matches!(e, Error::Hv { .. }), "{e:?}");
        drop(Hvf::create_vm(&VmConfig::default()).unwrap());

        // The 4 KiB granule (macOS 26.0).
        let cfg = VmConfig {
            ipa_granule: Some(IpaGranule::K4),
            ..Default::default()
        };
        match Hvf::create_vm(&cfg) {
            Ok(vm) => assert_eq!(vm.ipa_granule(), 4096),
            Err(Error::Unavailable { since: (26, 0), .. }) => {}
            Err(e) => panic!("{e}"),
        }

        // EL2, where supported (macOS 15.0, M3 or newer).
        if Hvf::el2_supported().unwrap_or(false) {
            let cfg = VmConfig {
                el2: true,
                ..Default::default()
            };
            let vm = Hvf::create_vm(&cfg).unwrap();
            assert!(vm.el2_enabled());
        }
    }
}
