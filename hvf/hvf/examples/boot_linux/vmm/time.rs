// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! The host counter that the guest's virtual counter is derived from.

use std::sync::OnceLock;
use std::time::Duration;

/// `mach_timebase_info_data_t`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct Timebase {
    numer: u32,
    denom: u32,
}

// The libc crate deprecates its declarations of these in favour of the
// `mach2` crate; the example declares the two it needs itself.
unsafe extern "C" {
    /// The host counter. The guest counter is this minus the vCPU's vtimer
    /// offset.
    safe fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut Timebase) -> i32;
}

/// The current value of the host counter, in ticks.
pub fn mach_ticks() -> u64 {
    mach_absolute_time()
}

/// The ratio of nanoseconds to counter ticks (125/3 on Apple silicon, a
/// 24 MHz counter).
fn timebase() -> Timebase {
    static TIMEBASE: OnceLock<Timebase> = OnceLock::new();
    *TIMEBASE.get_or_init(|| {
        let mut tb = Timebase::default();
        // SAFETY: `tb` is a valid, writable `mach_timebase_info_data_t`.
        let ret = unsafe { mach_timebase_info(&mut tb) };
        if ret != 0 || tb.denom == 0 {
            // The documented Apple silicon value.
            Timebase {
                numer: 125,
                denom: 3,
            }
        } else {
            tb
        }
    })
}

/// Converts counter ticks to a duration, without the rounding error of an
/// integer nanoseconds-per-tick factor.
pub fn ticks_to_duration(ticks: u64) -> Duration {
    let tb = timebase();
    let nanos = u128::from(ticks) * u128::from(tb.numer) / u128::from(tb.denom);
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticks_to_duration() {
        // One second of a 24 MHz counter.
        let d = ticks_to_duration(24_000_000);
        assert!(d.as_millis().abs_diff(1000) <= 1, "{d:?}");
        assert!(mach_ticks() > 0);
    }
}
