// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! PSCI 1.0 over `HVC`, and the power state of every vCPU.
//!
//! Linux brings the secondary CPUs up with `CPU_ON` and powers the machine
//! off with `SYSTEM_OFF` (or `SYSTEM_RESET` after a panic with `panic=-1`).
//! Every vCPU has its own thread; a vCPU that is off waits in
//! [`Cpus::wait_for_on`] until another vCPU turns it on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

// Function ids (SMC32 `0x84..`, SMC64 `0xc4..`).
const PSCI_VERSION: u32 = 0x8400_0000;
const CPU_SUSPEND: u32 = 0x8400_0001;
const CPU_SUSPEND_64: u32 = 0xc400_0001;
const CPU_OFF: u32 = 0x8400_0002;
const CPU_ON: u32 = 0x8400_0003;
const CPU_ON_64: u32 = 0xc400_0003;
const AFFINITY_INFO: u32 = 0x8400_0004;
const AFFINITY_INFO_64: u32 = 0xc400_0004;
const MIGRATE: u32 = 0x8400_0005;
const MIGRATE_64: u32 = 0xc400_0005;
const MIGRATE_INFO_TYPE: u32 = 0x8400_0006;
const SYSTEM_OFF: u32 = 0x8400_0008;
const SYSTEM_RESET: u32 = 0x8400_0009;
const PSCI_FEATURES: u32 = 0x8400_000a;

/// The functions [`handle`] implements, as reported by `PSCI_FEATURES`.
const SUPPORTED: [u32; 12] = [
    PSCI_VERSION,
    CPU_SUSPEND,
    CPU_SUSPEND_64,
    CPU_OFF,
    CPU_ON,
    CPU_ON_64,
    AFFINITY_INFO,
    AFFINITY_INFO_64,
    MIGRATE_INFO_TYPE,
    SYSTEM_OFF,
    SYSTEM_RESET,
    PSCI_FEATURES,
];

/// PSCI 1.0.
const VERSION_1_0: i32 = 0x1_0000;

// Return codes.
const SUCCESS: i32 = 0;
const NOT_SUPPORTED: i32 = -1;
const INVALID_PARAMETERS: i32 = -2;
const ALREADY_ON: i32 = -4;
const ON_PENDING: i32 = -5;

/// `MIGRATE_INFO_TYPE`: no trusted OS that needs migrating.
const TOS_NOT_PRESENT: i32 = 2;

// `AFFINITY_INFO` results.
const AFF_ON: i32 = 0;
const AFF_OFF: i32 = 1;
const AFF_ON_PENDING: i32 = 2;

/// The affinity fields of `MPIDR_EL1` (Aff3, Aff2, Aff1, Aff0).
const MPIDR_AFF_MASK: u64 = 0xff_00ff_ffff;

/// `MPIDR_EL1` of vCPU `index`: Aff0 is the index, so that the in-kernel GIC
/// gives vCPU `i` the `i`-th redistributor frame. Bit 31 is RES1.
pub fn mpidr(index: usize) -> u64 {
    0x8000_0000 | index as u64
}

/// The power state of a vCPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PowerState {
    Off,
    /// `CPU_ON` was called; the vCPU starts at `entry` with `x0 = context`.
    OnPending {
        entry: u64,
        context: u64,
    },
    On,
}

#[derive(Debug)]
struct Slot {
    state: Mutex<PowerState>,
    wake: Condvar,
}

/// The power state of every vCPU, shared by the vCPU threads.
#[derive(Debug)]
pub struct Cpus {
    slots: Vec<Slot>,
    stopped: AtomicBool,
}

/// What the vCPU loop does after a PSCI call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Write the value to `x0` and continue.
    Return(u64),
    /// Power this vCPU off (`CPU_OFF`).
    CpuOff,
    /// Power the machine off.
    SystemOff,
    /// Reset the machine.
    SystemReset,
}

/// A PSCI return code as written to `x0`: sign-extended to 64 bits.
fn ret(code: i32) -> Action {
    Action::Return(i64::from(code) as u64)
}

impl Cpus {
    /// `count` vCPUs, all off except vCPU 0, which starts at `entry` with
    /// `x0 = context`.
    pub fn new(count: usize, entry: u64, context: u64) -> Cpus {
        let slots = (0..count)
            .map(|i| Slot {
                state: Mutex::new(if i == 0 {
                    PowerState::OnPending { entry, context }
                } else {
                    PowerState::Off
                }),
                wake: Condvar::new(),
            })
            .collect();
        Cpus {
            slots,
            stopped: AtomicBool::new(false),
        }
    }

    fn lock(&self, index: usize) -> MutexGuard<'_, PowerState> {
        self.slots[index]
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The index of the vCPU with affinity `target`.
    fn index_of(&self, target: u64) -> Option<usize> {
        (0..self.slots.len()).find(|&i| mpidr(i) & MPIDR_AFF_MASK == target & MPIDR_AFF_MASK)
    }

    /// Whether the machine is shutting down.
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Stops the machine: wakes every vCPU thread waiting in
    /// [`wait_for_on`](Self::wait_for_on) or [`idle`](Self::idle).
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        for (i, slot) in self.slots.iter().enumerate() {
            // Taking the lock orders the flag before the waiter's check.
            let _state = self.lock(i);
            slot.wake.notify_all();
        }
    }

    /// Waits until vCPU `index` is turned on, marks it on and returns its
    /// entry point and context id. Returns `None` when the machine stops.
    pub fn wait_for_on(&self, index: usize) -> Option<(u64, u64)> {
        let mut state = self.lock(index);
        loop {
            if self.is_stopped() {
                return None;
            }
            if let PowerState::OnPending { entry, context } = *state {
                *state = PowerState::On;
                return Some((entry, context));
            }
            state = self.slots[index]
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Marks vCPU `index` off, after it called `CPU_OFF`.
    pub fn set_off(&self, index: usize) {
        *self.lock(index) = PowerState::Off;
    }

    /// Sleeps for up to `timeout`, or until the machine stops.
    pub fn idle(&self, index: usize, timeout: Duration) {
        let state = self.lock(index);
        if !self.is_stopped() {
            // Spurious wakeups only shorten the sleep.
            let _ = self.slots[index].wake.wait_timeout(state, timeout);
        }
    }

    fn cpu_on(&self, target: u64, entry: u64, context: u64) -> i32 {
        let Some(index) = self.index_of(target) else {
            return INVALID_PARAMETERS;
        };
        let mut state = self.lock(index);
        match *state {
            PowerState::On => ALREADY_ON,
            PowerState::OnPending { .. } => ON_PENDING,
            PowerState::Off => {
                *state = PowerState::OnPending { entry, context };
                self.slots[index].wake.notify_all();
                SUCCESS
            }
        }
    }

    fn affinity_info(&self, target: u64, level: u64) -> i32 {
        if level != 0 {
            // A single cluster, which is on while any vCPU runs a PSCI call.
            return AFF_ON;
        }
        let Some(index) = self.index_of(target) else {
            return INVALID_PARAMETERS;
        };
        match *self.lock(index) {
            PowerState::On => AFF_ON,
            PowerState::Off => AFF_OFF,
            PowerState::OnPending { .. } => AFF_ON_PENDING,
        }
    }
}

/// Handles the PSCI call in `x[0..4]`. Anything that is not a PSCI function
/// returns `NOT_SUPPORTED`, which is also the SMCCC "unknown function"
/// result.
pub fn handle(cpus: &Cpus, x: [u64; 4]) -> Action {
    let function = x[0] as u32;
    // SMC32 functions take 32-bit arguments.
    let arg = |n: usize| {
        if function & 0x4000_0000 != 0 {
            x[n]
        } else {
            x[n] & 0xffff_ffff
        }
    };
    match function {
        PSCI_VERSION => ret(VERSION_1_0),
        // Standby that ends at once, as if an interrupt woke the vCPU.
        CPU_SUSPEND | CPU_SUSPEND_64 => ret(SUCCESS),
        CPU_OFF => Action::CpuOff,
        CPU_ON | CPU_ON_64 => ret(cpus.cpu_on(arg(1), arg(2), arg(3))),
        AFFINITY_INFO | AFFINITY_INFO_64 => ret(cpus.affinity_info(arg(1), arg(2))),
        MIGRATE | MIGRATE_64 => ret(NOT_SUPPORTED),
        MIGRATE_INFO_TYPE => ret(TOS_NOT_PRESENT),
        SYSTEM_OFF => Action::SystemOff,
        SYSTEM_RESET => Action::SystemReset,
        PSCI_FEATURES => {
            let queried = arg(1) as u32;
            ret(if SUPPORTED.contains(&queried) {
                SUCCESS
            } else {
                NOT_SUPPORTED
            })
        }
        _ => ret(NOT_SUPPORTED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUS_ONE: u64 = u64::MAX;

    #[test]
    fn test_version_and_features() {
        let cpus = Cpus::new(1, 0, 0);
        assert_eq!(
            handle(&cpus, [PSCI_VERSION as u64, 0, 0, 0]),
            Action::Return(0x1_0000)
        );
        assert_eq!(
            handle(&cpus, [PSCI_FEATURES as u64, CPU_ON_64 as u64, 0, 0]),
            Action::Return(0)
        );
        // SMCCC_VERSION is not implemented, so Linux assumes SMCCC 1.0.
        assert_eq!(
            handle(&cpus, [PSCI_FEATURES as u64, 0x8000_0000, 0, 0]),
            Action::Return(MINUS_ONE)
        );
        assert_eq!(
            handle(&cpus, [0x8600_0000, 0, 0, 0]),
            Action::Return(MINUS_ONE)
        );
        assert_eq!(
            handle(&cpus, [SYSTEM_OFF as u64, 0, 0, 0]),
            Action::SystemOff
        );
        assert_eq!(
            handle(&cpus, [SYSTEM_RESET as u64, 0, 0, 0]),
            Action::SystemReset
        );
    }

    #[test]
    fn test_cpu_on() {
        let cpus = Cpus::new(2, 0x8000_0000, 0x9fe0_0000);
        assert_eq!(cpus.wait_for_on(0), Some((0x8000_0000, 0x9fe0_0000)));
        let on = |target: u64| handle(&cpus, [CPU_ON_64 as u64, target, 0x8010_0000, 7]);
        let info = |target: u64| handle(&cpus, [AFFINITY_INFO_64 as u64, target, 0, 0]);

        assert_eq!(info(mpidr(1)), Action::Return(1));
        assert_eq!(on(mpidr(5)), Action::Return(-2i64 as u64));
        assert_eq!(on(mpidr(0)), Action::Return(-4i64 as u64));
        assert_eq!(on(mpidr(1)), Action::Return(0));
        assert_eq!(info(mpidr(1)), Action::Return(2));
        assert_eq!(on(mpidr(1)), Action::Return(-5i64 as u64));
        // Linux passes the affinity without bit 31.
        assert_eq!(cpus.wait_for_on(1), Some((0x8010_0000, 7)));
        assert_eq!(info(1), Action::Return(0));

        assert_eq!(handle(&cpus, [CPU_OFF as u64, 0, 0, 0]), Action::CpuOff);
        cpus.set_off(1);
        assert_eq!(info(mpidr(1)), Action::Return(1));

        cpus.stop();
        assert_eq!(cpus.wait_for_on(1), None);
    }
}
