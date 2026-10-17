// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! VM creation, the one-VM-per-process rule and teardown.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use common::hv_guard;
use hvf::{Error, Hvf, IpaGranule, VmConfig};

/// The IPA size probes need macOS 13.0; older releases always use a 36-bit
/// IPA. Returns `None` there.
fn ipa_probe(probe: hvf::Result<u32>) -> Option<u32> {
    match probe {
        Ok(bits) => Some(bits),
        Err(Error::Unavailable { since: (13, 0), .. }) => None,
        Err(e) => panic!("{e:?}"),
    }
}

/// Retries `create_vm` while the previous VM is still being torn down by the
/// framework (`hv_vm_destroy` failing with `HV_BUSY`), for at most `limit`.
fn create_vm_within(cfg: &VmConfig, limit: Duration) -> hvf::Vm {
    let start = Instant::now();
    loop {
        match Hvf::create_vm(cfg) {
            Ok(vm) => return vm,
            Err(Error::Hv {
                call: "hv_vm_destroy",
                err,
            }) if err.is_busy() && start.elapsed() < limit => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(e) => panic!("create_vm failed after {:?}: {e:?}", start.elapsed()),
        }
    }
}

#[test]
fn test_second_vm_fails() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    assert_eq!(
        Hvf::create_vm(&VmConfig::default()).unwrap_err(),
        Error::VmExists
    );
    // Any configuration is rejected while a VM is alive.
    let cfg = VmConfig {
        ipa_bits: Some(ipa_probe(Hvf::max_ipa_bits()).unwrap_or(36)),
        ..Default::default()
    };
    assert_eq!(Hvf::create_vm(&cfg).unwrap_err(), Error::VmExists);
    // A clone is the same VM: it keeps the slot taken.
    let clone = vm.clone();
    drop(vm);
    assert_eq!(
        Hvf::create_vm(&VmConfig::default()).unwrap_err(),
        Error::VmExists
    );
    drop(clone);
}

#[test]
fn test_drop_and_recreate() {
    let Some(_g) = hv_guard() else { return };
    for _ in 0..5 {
        let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
        let vcpu = vm.create_vcpu(None).unwrap();
        drop(vcpu);
        drop(vm);
    }
    // The effective configuration is reported.
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    assert_eq!(
        vm.ipa_bits(),
        ipa_probe(Hvf::default_ipa_bits()).unwrap_or(36)
    );
    let granule = match Hvf::default_ipa_granule() {
        Ok(g) => g,
        Err(Error::Unavailable { .. }) => IpaGranule::K16,
        Err(e) => panic!("{e}"),
    };
    assert_eq!(vm.ipa_granule(), granule.bytes());
    assert!(!vm.el2_enabled());
}

#[test]
fn test_vm_kept_alive_by_vcpu_on_other_thread() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let (created_tx, created_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let handle = {
        let vm = vm.clone();
        thread::spawn(move || {
            let vcpu = vm.create_vcpu(None).unwrap();
            // Only the vCPU keeps the VM alive from now on.
            drop(vm);
            created_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            drop(vcpu);
        })
    };
    created_rx.recv().unwrap();
    drop(vm);

    // The vCPU on the other thread still holds the VM.
    assert_eq!(
        Hvf::create_vm(&VmConfig::default()).unwrap_err(),
        Error::VmExists
    );

    // Once the thread dropped its vCPU and exited, the VM is destroyed.
    release_tx.send(()).unwrap();
    handle.join().unwrap();
    drop(Hvf::create_vm(&VmConfig::default()).unwrap());
}

#[test]
fn test_ipa_bits() {
    let Some(_g) = hv_guard() else { return };
    let (Some(max), Some(default)) = (
        ipa_probe(Hvf::max_ipa_bits()),
        ipa_probe(Hvf::default_ipa_bits()),
    ) else {
        println!("SKIP: the IPA size probes need macOS 13.0");
        return;
    };
    assert!(default <= max, "default {default} > max {max}");

    let cfg = VmConfig {
        ipa_bits: Some(max),
        ..Default::default()
    };
    let vm = Hvf::create_vm(&cfg).unwrap();
    assert_eq!(vm.ipa_bits(), max);
    drop(vm);

    // One bit more than the host supports is rejected by the framework, and
    // the failed creation frees the slot again.
    let cfg = VmConfig {
        ipa_bits: Some(max + 1),
        ..Default::default()
    };
    let e = Hvf::create_vm(&cfg).unwrap_err();
    assert!(matches!(e, Error::Hv { .. }), "{e:?}");

    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    assert_eq!(vm.ipa_bits(), default);
}

#[test]
fn test_ipa_granule_and_el2_configs() {
    let Some(_g) = hv_guard() else { return };
    for granule in [IpaGranule::K4, IpaGranule::K16] {
        let cfg = VmConfig {
            ipa_granule: Some(granule),
            ..Default::default()
        };
        match Hvf::create_vm(&cfg) {
            Ok(vm) => assert_eq!(vm.ipa_granule(), granule.bytes()),
            // The granule setter needs macOS 26.0, VM configurations 13.0.
            Err(Error::Unavailable {
                since: (26, 0) | (13, 0),
                ..
            }) => {}
            Err(e) => panic!("{granule:?}: {e:?}"),
        }
    }

    let cfg = VmConfig {
        el2: true,
        ..Default::default()
    };
    match Hvf::el2_supported() {
        Ok(true) => assert!(Hvf::create_vm(&cfg).unwrap().el2_enabled()),
        // Without EL2 support (before M3, or before macOS 15.0) the creation
        // fails rather than silently ignoring the request.
        Ok(false) | Err(Error::Unavailable { .. }) => {
            assert!(Hvf::create_vm(&cfg).is_err());
        }
        Err(e) => panic!("{e:?}"),
    }
}

/// A VM whose destruction fails (`HV_BUSY`) is retried by the next
/// `create_vm` and recovered once the framework lets it go.
///
/// A `mem::forget`-ten [`hvf::Vcpu`] cannot be used to provoke this: it holds
/// a reference to the VM, so forgetting it leaks the VM for the rest of the
/// process (`create_vm` then fails with `VmExists`, as documented). Instead,
/// a vCPU created directly through the bindings, behind the crate's back,
/// keeps `hv_vm_destroy` busy while its thread lives.
#[test]
fn test_zombie_vm_recovery() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();

    let (created_tx, created_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let handle = thread::spawn(move || {
        let mut id = 0;
        let mut exit = std::ptr::null_mut();
        // SAFETY: `id` and `exit` are valid for writes, and a null
        // configuration selects the default one. The vCPU is intentionally
        // never destroyed; the framework reclaims it when the thread exits.
        let ret = unsafe { hvf_bindings::hv_vcpu_create(&mut id, &mut exit, std::ptr::null_mut()) };
        created_tx.send(ret).unwrap();
        release_rx.recv().unwrap();
    });
    assert_eq!(created_rx.recv().unwrap(), hvf_bindings::HV_SUCCESS);

    // Dropping the last handle cannot destroy the VM, so the slot becomes a
    // zombie and the next creation reports the failed destroy.
    drop(vm);
    let e = Hvf::create_vm(&VmConfig::default()).unwrap_err();
    assert!(
        matches!(e, Error::Hv { call: "hv_vm_destroy", err } if err.is_busy()),
        "{e:?}"
    );
    // It keeps failing while the vCPU exists.
    let e = Hvf::create_vm(&VmConfig::default()).unwrap_err();
    assert!(
        matches!(
            e,
            Error::Hv {
                call: "hv_vm_destroy",
                ..
            }
        ),
        "{e:?}"
    );

    // Once the owning thread is gone, creation succeeds within a second.
    release_tx.send(()).unwrap();
    handle.join().unwrap();
    let vm = create_vm_within(&VmConfig::default(), Duration::from_secs(1));
    // The recovered slot behaves like a fresh one.
    assert_eq!(
        Hvf::create_vm(&VmConfig::default()).unwrap_err(),
        Error::VmExists
    );
    drop(vm);
    drop(Hvf::create_vm(&VmConfig::default()).unwrap());
}

#[test]
fn test_vm_handles_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<hvf::Vm>();
    assert_send_sync::<hvf::VcpuKicker>();

    let Some(_g) = hv_guard() else { return };
    // VM-level calls work from threads other than the creator.
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let ipa_bits = thread::scope(|s| s.spawn(|| vm.ipa_bits()).join().unwrap());
    assert_eq!(ipa_bits, vm.ipa_bits());
    // And the VM can be destroyed from another thread.
    thread::spawn(move || drop(vm)).join().unwrap();
    drop(Hvf::create_vm(&VmConfig::default()).unwrap());
}
