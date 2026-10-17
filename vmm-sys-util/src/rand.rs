// Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: BSD-3-Clause

//! Miscellaneous functions related to getting (pseudo) random numbers and
//! strings.
//!
//! NOTE! This should not be used when you do need __real__ random numbers such
//! as for encryption but will probably be suitable when you want locally
//! unique ID's that will not be shared over the network.

use std::cell::Cell;
use std::ffi::OsString;
use std::str;
use std::sync::atomic::{AtomicU32, Ordering};

/// Gets an ever increasing u64 (at least for this process).
///
/// The number retrieved will be based upon the time of the last reboot (x86_64)
/// and something undefined for other architectures.
///
/// The value is monotonically non-decreasing, but two consecutive calls may
/// return the same value when the underlying clock has a coarser resolution
/// than the time between the calls (e.g. the 24 MHz system counter on Apple
/// Silicon).
pub fn timestamp_cycles() -> u64 {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: Safe because there's nothing that can go wrong with this call.
    unsafe {
        std::arch::x86_64::_rdtsc()
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        const MONOTONIC_CLOCK_MULTPIPLIER: u64 = 1_000_000_000;

        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // `CLOCK_MONOTONIC` only has microsecond resolution on Apple platforms,
        // while `CLOCK_UPTIME_RAW` is backed by the system counter
        // (`mach_absolute_time`) and has nanosecond granularity.
        #[cfg(target_vendor = "apple")]
        const CLOCK_ID: libc::clockid_t = libc::CLOCK_UPTIME_RAW;
        #[cfg(not(target_vendor = "apple"))]
        const CLOCK_ID: libc::clockid_t = libc::CLOCK_MONOTONIC;

        // SAFETY: We initialized the parameters correctly and we trust the function.
        unsafe {
            libc::clock_gettime(CLOCK_ID, &mut ts);
        }
        (ts.tv_sec as u64) * MONOTONIC_CLOCK_MULTPIPLIER + (ts.tv_nsec as u64)
    }
}

/// Generate pseudo random u32 numbers.
///
/// Each thread keeps its own xorshift state, seeded on first use from
/// [`timestamp_cycles`] mixed with a process-wide counter, and advanced on
/// every call. Consecutive calls on the same thread therefore never return
/// the same value, even when the clock is coarser than the time between them.
pub fn xor_pseudo_rng_u32() -> u32 {
    thread_local! {
        static STATE: Cell<u32> = const { Cell::new(0) };
    }

    STATE.with(|state| {
        let mut t = state.get();
        if t == 0 {
            t = seed();
        }
        // Taken from https://en.wikipedia.org/wiki/Xorshift
        t ^= t << 13;
        t ^= t >> 17;
        t ^= t << 5;
        state.set(t);
        t
    })
}

// Returns a non-zero xorshift seed. The process-wide counter makes threads
// seeded within the same clock tick start from different states.
fn seed() -> u32 {
    static SEED_COUNTER: AtomicU32 = AtomicU32::new(0);

    let ts = timestamp_cycles();
    // Use a Weyl increment so that consecutive counter values are far apart.
    let count = SEED_COUNTER
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_mul(0x9e37_79b9);
    let seed = (ts as u32) ^ ((ts >> 32) as u32) ^ count;
    // xorshift has an all-zero fixed point, so never start from zero.
    if seed == 0 {
        0x9e37_79b9
    } else {
        seed
    }
}

// This will get an array of numbers that can safely be converted to strings
// because they will be in the range [a-zA-Z0-9]. The return vector could be any
// size between 0 and 4.
fn xor_pseudo_rng_u8_alphanumerics(rand_fn: &dyn Fn() -> u32) -> Vec<u8> {
    rand_fn()
        .to_ne_bytes()
        .to_vec()
        .drain(..)
        .filter(|val| {
            (48..=57).contains(val) || (65..=90).contains(val) || (97..=122).contains(val)
        })
        .collect()
}

fn xor_pseudo_rng_u8_bytes(rand_fn: &dyn Fn() -> u32) -> Vec<u8> {
    rand_fn().to_ne_bytes().to_vec()
}

fn rand_alphanumerics_impl(rand_fn: &dyn Fn() -> u32, len: usize) -> OsString {
    let mut buf = OsString::new();
    let mut done = 0;
    loop {
        for n in xor_pseudo_rng_u8_alphanumerics(rand_fn) {
            done += 1;
            buf.push(str::from_utf8(&[n]).unwrap_or("_"));
            if done >= len {
                return buf;
            }
        }
    }
}

fn rand_bytes_impl(rand_fn: &dyn Fn() -> u32, len: usize) -> Vec<u8> {
    let mut buf: Vec<Vec<u8>> = Vec::new();
    let mut num = if len.is_multiple_of(4) {
        len / 4
    } else {
        len / 4 + 1
    };
    while num > 0 {
        buf.push(xor_pseudo_rng_u8_bytes(rand_fn));
        num -= 1;
    }
    buf.into_iter().flatten().take(len).collect()
}

/// Gets a pseudo random OsString of length `len` with characters in the
/// range [a-zA-Z0-9].
pub fn rand_alphanumerics(len: usize) -> OsString {
    rand_alphanumerics_impl(&xor_pseudo_rng_u32, len)
}

/// Get a pseudo random vector of `len` bytes.
pub fn rand_bytes(len: usize) -> Vec<u8> {
    rand_bytes_impl(&xor_pseudo_rng_u32, len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_cycles() {
        // The timestamp never goes backwards.
        for _ in 0..1000 {
            let a = timestamp_cycles();
            let b = timestamp_cycles();
            assert!(a <= b);
        }

        // The clock may be coarser than the time between two calls, but it
        // must strictly advance within a bounded number of iterations.
        let start = timestamp_cycles();
        assert!((0..1_000_000).any(|_| timestamp_cycles() > start));
    }

    #[test]
    fn test_xor_pseudo_rng_u32() {
        for _ in 0..1000 {
            assert_ne!(xor_pseudo_rng_u32(), xor_pseudo_rng_u32());
        }
    }

    #[test]
    fn test_xor_pseudo_rng_u32_no_repeated_words() {
        // The generator must not depend on the clock advancing between calls:
        // no 4-byte word may repeat within a short burst of draws.
        for _ in 0..1000 {
            let bytes = rand_bytes(16);
            let words: Vec<&[u8]> = bytes.chunks(4).collect();
            for i in 0..words.len() {
                for j in i + 1..words.len() {
                    assert_ne!(words[i], words[j], "repeated word in {bytes:x?}");
                }
            }
        }
    }

    #[test]
    fn test_xor_pseudo_rng_u32_threads_diverge() {
        // Threads seeded within the same clock tick still get different
        // sequences.
        let handles: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| [xor_pseudo_rng_u32(), xor_pseudo_rng_u32()]))
            .collect();
        let mut seqs: Vec<[u32; 2]> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let n = seqs.len();
        seqs.sort_unstable();
        seqs.dedup();
        assert_eq!(seqs.len(), n);
    }

    #[test]
    fn test_xor_pseudo_rng_u8_alphas() {
        let i = 3612982; // 55 (shifted 16 places), 33 (shifted 8 places), 54...
                         // The 33 will be discarded as it is not a valid letter
                         // (upper or lower) or number.
        let s = xor_pseudo_rng_u8_alphanumerics(&|| i);
        if cfg!(target_endian = "big") {
            assert_eq!(vec![55, 54], s);
        } else {
            assert_eq!(vec![54, 55], s);
        }
    }

    #[test]
    fn test_rand_alphanumerics_impl() {
        let s = rand_alphanumerics_impl(&|| 14134, 5);
        if cfg!(target_endian = "big") {
            assert_eq!("76767", s);
        } else {
            assert_eq!("67676", s);
        }
    }

    #[test]
    fn test_rand_alphanumerics() {
        let s = rand_alphanumerics(5);
        assert_eq!(5, s.len());
    }

    #[test]
    fn test_xor_pseudo_rng_u8_bytes() {
        let i = 3612982; // 55 (shifted 16 places), 33 (shifted 8 places), 54...
                         // The 33 will be discarded as it is not a valid letter
                         // (upper or lower) or number.
        let s = xor_pseudo_rng_u8_bytes(&|| i);
        if cfg!(target_endian = "big") {
            assert_eq!(vec![0, 55, 33, 54], s);
        } else {
            assert_eq!(vec![54, 33, 55, 0], s);
        }
    }

    #[test]
    fn test_rand_bytes_impl() {
        let s = rand_bytes_impl(&|| 1234567, 4);
        if cfg!(target_endian = "big") {
            assert_eq!(vec![0, 18, 214, 135], s);
        } else {
            assert_eq!(vec![135, 214, 18, 0], s);
        }
    }

    #[test]
    fn test_rand_bytes() {
        for i in 0..8 {
            assert_eq!(i, rand_bytes(i).len());
        }
    }
}
