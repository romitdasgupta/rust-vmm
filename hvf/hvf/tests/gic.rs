// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! The in-kernel GICv3.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`. The GIC
//! needs macOS 15.0: on older releases every test prints `SKIP` and passes.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use std::thread;

use common::hv_guard;
use hvf::{
    Error, Gic, GicConfig, GicIntid, GicParams, Hvf, InterruptType, MemoryFlags, MsiConfig, Reg,
    SysReg, Vcpu, VcpuExit, Vm, VmConfig,
};
use hvf_bindings as hv;

const DIST_BASE: u64 = 0x0800_0000;
const REDIST_BASE: u64 = 0x080a_0000;
const MSI_BASE: u64 = 0x0802_0000;
const MSI_INTID_BASE: u32 = 64;
const MSI_INTID_COUNT: u32 = 64;

/// Guest physical address of the guest code in [`Guest`].
const GUEST_IPA: u64 = 0x4000_0000;
/// `hvc #0; b .`
const HVC: [u8; 8] = [0x02, 0x00, 0x00, 0xd4, 0x00, 0x00, 0x00, 0x14];

/// The layout used by most tests.
const LAYOUT: GicConfig = GicConfig {
    distributor_base: DIST_BASE,
    redistributor_base: REDIST_BASE,
    msi: Some(MsiConfig {
        base: MSI_BASE,
        intid_base: MSI_INTID_BASE,
        intid_count: MSI_INTID_COUNT,
    }),
};

/// The GIC parameters of the host, or `None` (after printing `SKIP`) if the
/// running macOS release has no in-kernel GIC.
fn gic_params_or_skip() -> Option<GicParams> {
    match Hvf::gic_params() {
        Ok(params) => Some(params),
        Err(Error::Unavailable { api, since }) => {
            println!("SKIP: {api} needs macOS {}.{}", since.0, since.1);
            None
        }
        Err(e) => panic!("gic_params: {e}"),
    }
}

/// Creates a VM with the GIC of [`LAYOUT`].
fn vm_with_gic() -> (Vm, Gic) {
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let gic = vm.create_gic(&LAYOUT).unwrap();
    (vm, gic)
}

/// Creates a vCPU on the calling thread with `MPIDR_EL1.Aff0 = aff0`.
fn vcpu_with_aff0(vm: &Vm, aff0: u8) -> Vcpu {
    let mut vcpu = vm.create_vcpu(None).unwrap();
    vcpu.set_sys_reg(SysReg::MPIDR_EL1, u64::from(aff0))
        .unwrap();
    vcpu
}

/// Asserts that `r` is the framework error `code` of the C function `call`.
#[track_caller]
fn assert_hv_err<T: std::fmt::Debug>(r: hvf::Result<T>, call: &str, code: hv::hv_return_t) {
    match r {
        Err(Error::Hv { call: c, err }) if c == call && err.0 == code => {}
        r => panic!("expected {call} to fail with {code:#x}, got {r:?}"),
    }
}

/// One granule of guest code mapped at [`GUEST_IPA`], unmapped and freed on
/// drop.
struct Guest {
    vm: Vm,
    host: *mut libc::c_void,
    size: usize,
}

impl Guest {
    fn new(vm: &Vm, code: &[u8]) -> Guest {
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
        // SAFETY: the mapping lives until `drop`, which unmaps it from the
        // guest first, and the host does not touch it in between.
        unsafe {
            vm.map(
                host.cast(),
                GUEST_IPA,
                size,
                MemoryFlags::READ | MemoryFlags::EXEC,
            )
        }
        .unwrap();
        Guest {
            vm: vm.clone(),
            host,
            size,
        }
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
fn test_gic_params() {
    let Some(_g) = hv_guard() else { return };
    let Some(params) = gic_params_or_skip() else {
        return;
    };
    for align in [
        params.distributor_base_alignment,
        params.redistributor_base_alignment,
        params.msi_region_base_alignment,
    ] {
        assert!(align.is_power_of_two(), "{params:?}");
    }
    // A GICv3 distributor and MSI frame are at least one 64 KiB frame, and a
    // redistributor has an RD_base and an SGI_base frame.
    assert!(params.distributor_size >= 0x10000, "{params:?}");
    assert!(params.msi_region_size >= 0x10000, "{params:?}");
    assert!(params.redistributor_size >= 0x20000, "{params:?}");
    // The region holds the redistributors of every vCPU.
    let max_vcpus = u64::from(Hvf::max_vcpu_count().unwrap());
    assert!(
        params.redistributor_region_size >= max_vcpus * params.redistributor_size,
        "{params:?}"
    );
    // SPIs start after the SGIs and PPIs and end before the special ids.
    assert_eq!(params.spi_base, 32, "{params:?}");
    assert!(params.spi_count > 0, "{params:?}");
    assert!(params.spi_base + params.spi_count <= 1020, "{params:?}");

    assert_eq!(Hvf::gic_intid(GicIntid::El1VirtualTimer).unwrap(), 27);
    assert_eq!(Hvf::gic_intid(GicIntid::El1PhysicalTimer).unwrap(), 30);
    assert_eq!(Hvf::gic_intid(GicIntid::PerformanceMonitor).unwrap(), 23);
}

#[test]
fn test_create_gic() {
    let Some(_g) = hv_guard() else { return };
    let Some(params) = gic_params_or_skip() else {
        return;
    };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();

    // Invalid layouts are rejected before reaching the framework, and leave
    // the GIC creatable.
    let misaligned = GicConfig {
        distributor_base: DIST_BASE + 0x1000,
        ..LAYOUT
    };
    assert_eq!(
        vm.create_gic(&misaligned).unwrap_err(),
        Error::Misaligned {
            what: "distributor_base",
            value: DIST_BASE + 0x1000,
            align: params.distributor_base_alignment,
        }
    );
    let overlapping = GicConfig {
        redistributor_base: DIST_BASE,
        ..LAYOUT
    };
    assert!(matches!(
        vm.create_gic(&overlapping).unwrap_err(),
        Error::Overlap { .. }
    ));
    let beyond_ipa = GicConfig {
        distributor_base: 1 << vm.ipa_bits(),
        ..LAYOUT
    };
    assert!(matches!(
        vm.create_gic(&beyond_ipa).unwrap_err(),
        Error::OutOfRange { .. }
    ));
    let msi_below_spis = GicConfig {
        msi: Some(MsiConfig {
            intid_base: 16,
            ..LAYOUT.msi.unwrap()
        }),
        ..LAYOUT
    };
    assert!(matches!(
        vm.create_gic(&msi_below_spis).unwrap_err(),
        Error::OutOfRange { .. }
    ));

    let gic = vm.create_gic(&LAYOUT).unwrap();
    assert_eq!(*gic.config(), LAYOUT);
    assert_eq!(vm.create_gic(&LAYOUT).unwrap_err(), Error::GicExists);

    // The distributor reports a GICv3 (`GICD_PIDR2.ArchRev`) supporting at
    // least the SPI range (`GICD_TYPER.ITLinesNumber`).
    let pidr2 = gic
        .distributor_reg(hv::HV_GIC_DISTRIBUTOR_REG_GICD_PIDR2)
        .unwrap();
    assert_eq!((pidr2 >> 4) & 0xf, 3, "GICD_PIDR2 {pidr2:#x}");
    let typer = gic
        .distributor_reg(hv::HV_GIC_DISTRIBUTOR_REG_GICD_TYPER)
        .unwrap();
    let it_lines = (typer & 0x1f) as u32;
    assert!(
        32 * (it_lines + 1) >= params.spi_base + params.spi_count,
        "GICD_TYPER {typer:#x}"
    );

    // The handle keeps the VM alive and is usable from any thread.
    drop(vm);
    thread::spawn(move || gic.set_spi(32, true).unwrap())
        .join()
        .unwrap();
}

#[test]
fn test_create_gic_after_vcpu() {
    let Some(_g) = hv_guard() else { return };
    if gic_params_or_skip().is_none() {
        return;
    }
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let vcpu = vm.create_vcpu(None).unwrap();
    assert_eq!(vm.create_gic(&LAYOUT).unwrap_err(), Error::GicAfterVcpu);
    // Destroying the vCPU does not make the GIC creatable again.
    drop(vcpu);
    assert_eq!(vm.create_gic(&LAYOUT).unwrap_err(), Error::GicAfterVcpu);
}

#[test]
fn test_create_gic_without_msi() {
    let Some(_g) = hv_guard() else { return };
    if gic_params_or_skip().is_none() {
        return;
    }
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let cfg = GicConfig {
        msi: None,
        ..LAYOUT
    };
    let gic = vm.create_gic(&cfg).unwrap();
    assert_eq!(gic.config().msi, None);
    // Without an MSI frame every SPI is a wired interrupt.
    gic.set_spi(MSI_INTID_BASE, true).unwrap();
    gic.set_spi(MSI_INTID_BASE, false).unwrap();
}

#[test]
fn test_redistributor_base() {
    let Some(_g) = hv_guard() else { return };
    let Some(params) = gic_params_or_skip() else {
        return;
    };
    let (vm, _gic) = vm_with_gic();

    let vcpu0 = vcpu_with_aff0(&vm, 0);
    assert_eq!(vcpu0.gic_redistributor_base().unwrap(), REDIST_BASE);

    // vCPUs are bound to their thread; the second one gets the next
    // redistributor by its affinity.
    let vm1 = vm.clone();
    let base1 = thread::spawn(move || {
        let vcpu1 = vcpu_with_aff0(&vm1, 1);
        vcpu1.gic_redistributor_base().unwrap()
    })
    .join()
    .unwrap();
    assert_eq!(base1, REDIST_BASE + params.redistributor_size);
    assert_eq!(params.redistributor_size, 0x20000);
}

#[test]
fn test_spi_and_msi() {
    let Some(_g) = hv_guard() else { return };
    if gic_params_or_skip().is_none() {
        return;
    }
    let (_vm, gic) = vm_with_gic();

    // VM-wide GIC calls work from a thread that owns no vCPU.
    let g = gic.clone();
    thread::spawn(move || {
        g.set_spi(32, true).unwrap();
        g.set_spi(32, false).unwrap();
    })
    .join()
    .unwrap();

    // An SGI/PPI id, or an SPI reserved for MSIs, is not a wired SPI.
    assert_hv_err(gic.set_spi(10, true), "hv_gic_set_spi", hv::HV_BAD_ARGUMENT);
    assert_hv_err(
        gic.set_spi(MSI_INTID_BASE, true),
        "hv_gic_set_spi",
        hv::HV_BAD_ARGUMENT,
    );

    // MSIs go through the doorbell of the MSI frame.
    gic.send_msi(MSI_BASE + 0x40, MSI_INTID_BASE).unwrap();
    gic.send_msi(
        MSI_BASE + u64::from(hv::HV_GIC_REG_GICM_SET_SPI_NSR),
        MSI_INTID_BASE + MSI_INTID_COUNT - 1,
    )
    .unwrap();

    // `GICM_TYPER` describes the MSI range: `INTID` in bits [25:16] and the
    // count in bits [9:0].
    let typer = gic.msi_reg(hv::HV_GIC_REG_GICM_TYPER).unwrap();
    assert_eq!(
        (typer >> 16) & 0x3ff,
        u64::from(MSI_INTID_BASE),
        "{typer:#x}"
    );
    assert_eq!(typer & 0x3ff, u64::from(MSI_INTID_COUNT), "{typer:#x}");
}

#[test]
fn test_distributor_regs_and_reset() {
    let Some(_g) = hv_guard() else { return };
    if gic_params_or_skip().is_none() {
        return;
    }
    let (_vm, gic) = vm_with_gic();

    // Enable SPI 33 (`GICD_ISENABLER1` bit 1) and read it back.
    let isenabler1 = hv::HV_GIC_DISTRIBUTOR_REG_GICD_ISENABLER0 + 4;
    assert_eq!(gic.distributor_reg(isenabler1).unwrap() & 0b10, 0);
    gic.set_distributor_reg(isenabler1, 0b10).unwrap();
    assert_eq!(gic.distributor_reg(isenabler1).unwrap() & 0b10, 0b10);

    // A reset disables it again.
    gic.reset().unwrap();
    assert_eq!(gic.distributor_reg(isenabler1).unwrap() & 0b10, 0);
}

#[test]
fn test_cpu_interface_regs() {
    let Some(_g) = hv_guard() else { return };
    if gic_params_or_skip().is_none() {
        return;
    }
    let (vm, _gic) = vm_with_gic();
    let mut vcpu = vcpu_with_aff0(&vm, 0);

    // ICC_PMR_EL1 implements at least the top four priority bits.
    vcpu.set_gic_icc_reg(hv::HV_GIC_ICC_REG_PMR_EL1, 0xf0)
        .unwrap();
    assert_eq!(vcpu.gic_icc_reg(hv::HV_GIC_ICC_REG_PMR_EL1).unwrap(), 0xf0);
    vcpu.set_gic_icc_reg(hv::HV_GIC_ICC_REG_PMR_EL1, 0x80)
        .unwrap();
    assert_eq!(vcpu.gic_icc_reg(hv::HV_GIC_ICC_REG_PMR_EL1).unwrap(), 0x80);

    // The redistributor of this vCPU: enable PPI 27, the virtual timer.
    let isenabler0 = hv::HV_GIC_REDISTRIBUTOR_REG_GICR_ISENABLER0;
    vcpu.set_gic_redistributor_reg(isenabler0, 1 << 27).unwrap();
    assert_ne!(
        vcpu.gic_redistributor_reg(isenabler0).unwrap() & (1 << 27),
        0
    );

    // The hypervisor control and virtual CPU interfaces need EL2.
    assert!(matches!(
        vcpu.gic_ich_reg(hv::HV_GIC_ICH_REG_HCR_EL2),
        Err(Error::Hv {
            call: "hv_gic_get_ich_reg",
            ..
        })
    ));
    assert!(matches!(
        vcpu.set_gic_ich_reg(hv::HV_GIC_ICH_REG_HCR_EL2, 0),
        Err(Error::Hv {
            call: "hv_gic_set_ich_reg",
            ..
        })
    ));
    assert!(matches!(
        vcpu.gic_icv_reg(hv::HV_GIC_ICV_REG_PMR_EL1),
        Err(Error::Hv {
            call: "hv_gic_get_icv_reg",
            ..
        })
    ));
    assert!(matches!(
        vcpu.set_gic_icv_reg(hv::HV_GIC_ICV_REG_PMR_EL1, 0),
        Err(Error::Hv {
            call: "hv_gic_set_icv_reg",
            ..
        })
    ));

    // With the in-kernel GIC the framework owns the interrupt lines (P7).
    assert_hv_err(
        vcpu.set_pending_interrupt(InterruptType::Irq, true),
        "hv_vcpu_set_pending_interrupt",
        hv::HV_UNSUPPORTED,
    );
}

#[test]
fn test_el2_cpu_interface_regs() {
    let Some(_g) = hv_guard() else { return };
    if gic_params_or_skip().is_none() {
        return;
    }
    if !Hvf::el2_supported().unwrap_or(false) {
        println!("SKIP: EL2 is not supported on this host");
        return;
    }
    let vm = Hvf::create_vm(&VmConfig {
        el2: true,
        ..Default::default()
    })
    .unwrap();
    let _gic = vm.create_gic(&LAYOUT).unwrap();
    let mut vcpu = vcpu_with_aff0(&vm, 0);

    // ICH_HCR_EL2.En (bit 0).
    vcpu.set_gic_ich_reg(hv::HV_GIC_ICH_REG_HCR_EL2, 1).unwrap();
    assert_eq!(vcpu.gic_ich_reg(hv::HV_GIC_ICH_REG_HCR_EL2).unwrap() & 1, 1);
    vcpu.set_gic_icv_reg(hv::HV_GIC_ICV_REG_PMR_EL1, 0xf0)
        .unwrap();
    assert_eq!(vcpu.gic_icv_reg(hv::HV_GIC_ICV_REG_PMR_EL1).unwrap(), 0xf0);
}

/// Distributor register used to tell saved states apart: `GICD_ISENABLER1`.
const MARKER_REG: u16 = hv::HV_GIC_DISTRIBUTOR_REG_GICD_ISENABLER0 + 4;
/// Enables SPIs 33 and 35.
const MARKER: u64 = 0b1010;

#[test]
fn test_save_restore_state() {
    let Some(_g) = hv_guard() else { return };
    if gic_params_or_skip().is_none() {
        return;
    }

    // Save with nothing running.
    let state = {
        let (vm, gic) = vm_with_gic();
        let _vcpu = vcpu_with_aff0(&vm, 0);
        gic.set_distributor_reg(MARKER_REG, MARKER).unwrap();
        let state = gic.save_state().unwrap();
        assert!(!state.is_empty());
        state
    };

    // Restore in a fresh VM, after creating the vCPUs and before the first
    // run.
    let (vm, gic) = vm_with_gic();
    let mut vcpu = vcpu_with_aff0(&vm, 0);
    assert_eq!(gic.distributor_reg(MARKER_REG).unwrap() & MARKER, 0);
    gic.restore_state(&state).unwrap();
    assert_eq!(gic.distributor_reg(MARKER_REG).unwrap() & MARKER, MARKER);

    // Garbage is rejected.
    assert!(matches!(
        gic.restore_state(&[0u8; 16]),
        Err(Error::Hv {
            call: "hv_gic_set_state",
            ..
        })
    ));
    assert!(matches!(
        gic.restore_state(&[]),
        Err(Error::Hv {
            call: "hv_gic_set_state",
            ..
        })
    ));

    // The restored VM runs.
    let _guest = Guest::new(&vm, &HVC);
    vcpu.set_reg(Reg::PC, GUEST_IPA).unwrap();
    vcpu.set_reg(Reg::CPSR, 0x3c5).unwrap();
    let exit = vcpu.run().unwrap();
    assert!(matches!(exit, VcpuExit::Exception(_)), "{exit:?}");
}

#[test]
fn test_restore_state_after_run() {
    let Some(_g) = hv_guard() else { return };
    if gic_params_or_skip().is_none() {
        return;
    }
    let (vm, gic) = vm_with_gic();
    let mut vcpu = vcpu_with_aff0(&vm, 0);
    let state = gic.save_state().unwrap();
    gic.set_distributor_reg(MARKER_REG, MARKER).unwrap();

    let _guest = Guest::new(&vm, &HVC);
    vcpu.set_reg(Reg::PC, GUEST_IPA).unwrap();
    vcpu.set_reg(Reg::CPSR, 0x3c5).unwrap();
    let exit = vcpu.run().unwrap();
    assert!(matches!(exit, VcpuExit::Exception(_)), "{exit:?}");

    // Saving while the vCPU is stopped between runs works.
    let after_run = gic.save_state().unwrap();
    assert!(!after_run.is_empty());

    // The crate does not enforce "restore before the first run", and
    // neither does the framework (observed on macOS 27): a restore after a run
    // succeeds and takes effect. Restoring into a VM whose vCPUs already ran
    // is still unsupported by the documentation, so VMMs must not rely on it.
    gic.restore_state(&state).unwrap();
    assert_eq!(gic.distributor_reg(MARKER_REG).unwrap() & MARKER, 0);
    gic.restore_state(&after_run).unwrap();
    assert_eq!(gic.distributor_reg(MARKER_REG).unwrap() & MARKER, MARKER);
}
