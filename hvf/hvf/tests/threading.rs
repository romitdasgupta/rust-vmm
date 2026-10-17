// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! vCPU thread binding and kicking from other threads.
//!
//! Needs real Hypervisor.framework access; see `common::hv_guard`.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod common;

use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use common::{
    GUEST_IPA, GuestRam, SPIN, WFI, boot_vcpu, expect_exception, hv_guard, hvc, kick_after,
};
use hvf::{Error, ExceptionClass, Hvf, Reg, Vcpu, VcpuExit, VcpuKicker, VmConfig};

#[test]
fn test_second_vcpu_on_same_thread() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let vcpu = vm.create_vcpu(None).unwrap();
    let e = vm.create_vcpu(None).unwrap_err();
    assert!(
        matches!(e, Error::Hv { call: "hv_vcpu_create", err } if err.is_busy()),
        "{e:?}"
    );
    // Another thread can still create one.
    let other = thread::scope(|s| {
        s.spawn(|| vm.create_vcpu(None).map(|v| v.id()))
            .join()
            .unwrap()
    })
    .unwrap();
    assert_ne!(other, vcpu.id());
    drop(vcpu);
    // With the first vCPU gone, this thread may own a new one.
    drop(vm.create_vcpu(None).unwrap());
}

#[test]
fn test_kick_before_run() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[hvc(0), SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    let kicker = vcpu.kicker();
    assert_eq!(kicker.id(), vcpu.id());

    // A kick of a vCPU that is not running is remembered...
    assert!(kicker.kick().unwrap());
    assert_eq!(vcpu.run().unwrap(), VcpuExit::Canceled);
    // ...without running any guest code...
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), GUEST_IPA);
    // ...and only once: several kicks collapse into one cancellation.
    assert!(kicker.kick().unwrap());
    assert!(kicker.kick().unwrap());
    assert_eq!(vcpu.run().unwrap(), VcpuExit::Canceled);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
}

#[test]
fn test_kick_running_vcpu_from_other_thread() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[SPIN]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    for _ in 0..3 {
        let kicked = kick_after(vcpu.kicker(), Duration::from_millis(50));
        assert_eq!(vcpu.run().unwrap(), VcpuExit::Canceled);
        assert!(kicked.recv().unwrap().unwrap());
        // The guest was interrupted in its loop, and can be resumed.
        assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), GUEST_IPA);
    }
    assert!(vcpu.exec_time().unwrap() > 0);
}

/// A vCPU waiting in `wfi` is handed back to the VMM rather than blocked in
/// the framework, so it never needs a kick to wake up.
#[test]
fn test_wfi_exits() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[WFI, hvc(0)]);
    let mut vcpu = boot_vcpu(&vm, GUEST_IPA);
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Wfx { wfe: false }
    );
    // The PC points at the `wfi`; the VMM resumes past it.
    assert_eq!(vcpu.get_reg(Reg::PC).unwrap(), GUEST_IPA);
    vcpu.advance_pc().unwrap();
    assert_eq!(
        expect_exception(vcpu.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
}

#[test]
fn test_kicker_after_vcpu_dropped() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let vcpu = vm.create_vcpu(None).unwrap();
    let kicker = vcpu.kicker();
    let clone = kicker.clone();
    assert!(kicker.kick().unwrap());
    drop(vcpu);
    assert!(!kicker.kick().unwrap());
    assert!(!clone.kick().unwrap());
    // Also from another thread, and after the VM is gone.
    drop(vm);
    assert!(!thread::spawn(move || clone.kick().unwrap()).join().unwrap());
}

/// Regression test for P2: a kicker of a destroyed vCPU must not cancel a
/// new vCPU that the framework gave the same id.
#[test]
fn test_stale_kicker_and_id_reuse() {
    let Some(_g) = hv_guard() else { return };
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[hvc(0), WFI, SPIN]);

    let old = boot_vcpu(&vm, GUEST_IPA);
    let stale = old.kicker();
    let old_id = old.id();
    drop(old);

    let mut new = boot_vcpu(&vm, GUEST_IPA);
    // The framework hands out the lowest free id, so the id is reused.
    assert_eq!(new.id(), old_id);
    assert!(!stale.kick().unwrap());
    // The stale kick did not reach the new vCPU.
    assert_eq!(
        expect_exception(new.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
    assert_eq!(
        expect_exception(new.run().unwrap()),
        ExceptionClass::Wfx { wfe: false }
    );

    // The same holds when the old vCPU lived on another thread.
    drop(new);
    let (tx, rx) = mpsc::channel();
    {
        let vm = vm.clone();
        thread::spawn(move || tx.send(vm.create_vcpu(None).unwrap().kicker()).unwrap())
            .join()
            .unwrap();
    }
    let stale = rx.recv().unwrap();
    let mut new = boot_vcpu(&vm, GUEST_IPA);
    assert_eq!(new.id(), stale.id());
    assert!(!stale.kick().unwrap());
    assert_eq!(
        expect_exception(new.run().unwrap()),
        ExceptionClass::Hvc { imm: 0 }
    );
}

/// vCPUs started on their own threads by [`spawn_vcpu_threads`].
struct VcpuThreads {
    /// The kickers of the vCPUs, in no particular order.
    kickers: Vec<VcpuKicker>,
    /// Lets the threads run their vCPUs: one sender per thread.
    start: Vec<mpsc::Sender<()>>,
    /// The threads, which return the id of their vCPU and its exit.
    handles: Vec<JoinHandle<(u64, VcpuExit)>>,
}

impl VcpuThreads {
    /// Lets every thread run its vCPU.
    fn start(&self) {
        for tx in &self.start {
            tx.send(()).unwrap();
        }
    }
}

/// Spawns `n` threads that each create a vCPU with `boot`, send back its
/// kicker, wait for [`VcpuThreads::start`] and then run it.
///
/// Every vCPU exists once this returns. If a thread dies before sending its
/// kicker, this returns the thread handles instead of waiting forever for the
/// kicker, and the other threads give up: nothing blocks on a thread that is
/// gone.
fn spawn_vcpu_threads<F>(n: usize, boot: F) -> Result<VcpuThreads, Vec<JoinHandle<(u64, VcpuExit)>>>
where
    F: Fn(usize) -> Vcpu + Clone + Send + 'static,
{
    let (kicker_tx, kicker_rx) = mpsc::channel();
    let mut start = Vec::new();
    let handles: Vec<_> = (0..n)
        .map(|i| {
            let boot = boot.clone();
            let kicker_tx = kicker_tx.clone();
            let (start_tx, start_rx) = mpsc::channel::<()>();
            start.push(start_tx);
            thread::spawn(move || {
                let mut vcpu = boot(i);
                kicker_tx.send(vcpu.kicker()).unwrap();
                drop(kicker_tx);
                // Keep every vCPU alive until all were created. The sender
                // is gone if the test thread gave up.
                start_rx.recv().expect("the test thread gave up");
                // Every vCPU runs its guest until kicked.
                let exit = vcpu.run().unwrap();
                (vcpu.id(), exit)
            })
        })
        .collect();
    // Only the threads hold senders now, so `recv` fails once every thread
    // either sent its kicker or died.
    drop(kicker_tx);
    match (0..n).map(|_| kicker_rx.recv()).collect() {
        Ok(kickers) => Ok(VcpuThreads {
            kickers,
            start,
            handles,
        }),
        // Dropping `start` makes the surviving threads give up.
        Err(mpsc::RecvError) => Err(handles),
    }
}

#[test]
fn test_vcpus_on_many_threads() {
    let Some(_g) = hv_guard() else { return };
    const N: usize = 8;
    let max = Hvf::max_vcpu_count().unwrap() as usize;
    assert!(max >= N, "the host supports only {max} vCPUs");

    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[SPIN]);
    let threads = {
        let vm = vm.clone();
        spawn_vcpu_threads(N, move |_| boot_vcpu(&vm, GUEST_IPA))
    }
    .unwrap_or_else(|_| panic!("a vCPU thread died before sending its kicker"));
    threads.start();
    thread::sleep(Duration::from_millis(20));
    for kicker in &threads.kickers {
        assert!(kicker.kick().unwrap());
    }

    let mut ids: Vec<u64> = threads
        .handles
        .into_iter()
        .map(|h| {
            let (id, exit) = h.join().unwrap();
            assert_eq!(exit, VcpuExit::Canceled);
            id
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), N, "vCPU ids are not distinct");
    // All vCPUs are destroyed with their threads.
    for kicker in &threads.kickers {
        assert!(!kicker.kick().unwrap());
    }
}

/// Regression test for D2-6: a vCPU thread that dies before handing over
/// its kicker fails the test instead of hanging it.
#[test]
fn test_vcpu_thread_dies_before_start() {
    let Some(_g) = hv_guard() else { return };
    const N: usize = 4;
    let vm = Hvf::create_vm(&VmConfig::default()).unwrap();
    let _code = GuestRam::with_code(&vm, &[SPIN]);
    let (tx, rx) = mpsc::channel();
    {
        let vm = vm.clone();
        thread::spawn(move || {
            let result = spawn_vcpu_threads(N, move |i| {
                assert_ne!(i, 2, "injected failure of vCPU thread {i}");
                boot_vcpu(&vm, GUEST_IPA)
            });
            let _ = tx.send(result.err());
        });
    }
    let handles = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("spawn_vcpu_threads hangs when a vCPU thread dies")
        .expect("spawn_vcpu_threads did not notice the dead vCPU thread");
    // The failed thread and the ones that gave up all panicked, which
    // destroyed their vCPUs.
    assert_eq!(handles.len(), N);
    for h in handles {
        assert!(h.join().is_err());
    }
}
