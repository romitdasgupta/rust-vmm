// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! MMIO access arithmetic.
//!
//! Hypervisor.framework has no in-kernel MMIO emulation: a guest access to
//! unmapped memory exits with a data abort, and the VMM performs the transfer
//! between the guest register and the device. These helpers compute the
//! values involved, following the architectural rules for the register
//! width, sign extension and access size of the syndrome.

use crate::exit::DataAbortSyndrome;

/// The mask selecting the low `len` bytes of a 64-bit value.
fn len_mask(len: u8) -> u64 {
    match u32::from(len).checked_mul(8) {
        Some(bits) if bits < 64 => (1 << bits) - 1,
        _ => u64::MAX,
    }
}

/// The value to write to the guest's transfer register after an MMIO load of
/// `raw` (the little-endian device value, of which only the low
/// `syndrome.len` bytes are used).
///
/// The value is truncated to the access size, sign-extended if
/// `syndrome.sign_extend` is set, and truncated to 32 bits unless
/// `syndrome.sf64` is set.
///
/// # Example
///
/// ```
/// # use hvf::{DataAbortSyndrome, mmio_load_value};
/// // `ldrsb w2, [x1]` of the byte 0x80.
/// let ldrsb_w = DataAbortSyndrome {
///     len: 1,
///     srt: 2,
///     sign_extend: true,
///     sf64: false,
///     acquire_release: false,
/// };
/// assert_eq!(mmio_load_value(&ldrsb_w, 0x1234_5680), 0xffff_ff80);
/// ```
pub fn mmio_load_value(syndrome: &DataAbortSyndrome, raw: u64) -> u64 {
    let mask = len_mask(syndrome.len);
    let mut value = raw & mask;
    if syndrome.sign_extend && mask != u64::MAX {
        let sign_bit = (mask >> 1) + 1;
        if value & sign_bit != 0 {
            value |= !mask;
        }
    }
    if !syndrome.sf64 {
        value &= 0xffff_ffff;
    }
    value
}

/// The value an MMIO store transfers to the device, given the content `reg`
/// of the guest's transfer register: the low `syndrome.len` bytes of `reg`.
pub fn mmio_store_value(syndrome: &DataAbortSyndrome, reg: u64) -> u64 {
    reg & len_mask(syndrome.len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syndrome(len: u8, sign_extend: bool, sf64: bool) -> DataAbortSyndrome {
        DataAbortSyndrome {
            len,
            srt: 0,
            sign_extend,
            sf64,
            acquire_release: false,
        }
    }

    #[test]
    fn test_load_zero_extend() {
        const RAW: u64 = 0xfedc_ba98_8765_4321;
        // (len, sf64, expected)
        let cases = [
            (1, false, 0x21),
            (1, true, 0x21),
            (2, false, 0x4321),
            (2, true, 0x4321),
            (4, false, 0x8765_4321),
            (4, true, 0x8765_4321),
            (8, true, RAW),
            // A 64-bit access always has SF set; without it the
            // architecture writes a W register.
            (8, false, 0x8765_4321),
        ];
        for (len, sf64, expected) in cases {
            assert_eq!(
                mmio_load_value(&syndrome(len, false, sf64), RAW),
                expected,
                "len {len} sf64 {sf64}"
            );
        }
    }

    #[test]
    fn test_load_sign_extend() {
        // (len, sf64, raw, expected)
        let cases = [
            // Negative values.
            (1, false, 0x80, 0xffff_ff80),
            (1, true, 0x80, 0xffff_ffff_ffff_ff80),
            (2, false, 0x8001, 0xffff_8001),
            (2, true, 0x8001, 0xffff_ffff_ffff_8001),
            (4, true, 0x8000_0000, 0xffff_ffff_8000_0000),
            (4, false, 0x8000_0000, 0x8000_0000),
            // Positive values are unchanged.
            (1, false, 0x7f, 0x7f),
            (1, true, 0x7f, 0x7f),
            (2, false, 0x7fff, 0x7fff),
            (2, true, 0x7fff, 0x7fff),
            (4, true, 0x7fff_ffff, 0x7fff_ffff),
            (4, false, 0x7fff_ffff, 0x7fff_ffff),
            // Bytes above the access size are ignored.
            (1, true, 0xffff_ff00_0000_007f, 0x7f),
            (1, false, 0x1234_5680, 0xffff_ff80),
            (2, true, 0xabcd_0000_0001_ffff, 0xffff_ffff_ffff_ffff),
            (4, true, 0x1_0000_0001, 0x1),
        ];
        for (len, sf64, raw, expected) in cases {
            assert_eq!(
                mmio_load_value(&syndrome(len, true, sf64), raw),
                expected,
                "len {len} sf64 {sf64} raw {raw:#x}"
            );
        }
    }

    #[test]
    fn test_load_mmio_guest() {
        // `ldr w2, [x1, #4]` from the MMIO test guest, completed with
        // 0xdeadbeef: the full 32-bit value reaches X2.
        assert_eq!(
            mmio_load_value(&syndrome(4, false, false), 0xdead_beef),
            0xdead_beef
        );
    }

    #[test]
    fn test_store_value() {
        const REG: u64 = 0xfedc_ba98_7654_3210;
        let cases = [
            (1, 0x10),
            (2, 0x3210),
            (4, 0x7654_3210),
            (8, REG),
            // Not produced by the hardware, handled without panicking.
            (0, 0),
            (16, REG),
            (u8::MAX, REG),
        ];
        for (len, expected) in cases {
            for sign_extend in [false, true] {
                for sf64 in [false, true] {
                    assert_eq!(
                        mmio_store_value(&syndrome(len, sign_extend, sf64), REG),
                        expected,
                        "len {len}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_load_odd_lengths() {
        // Lengths the hardware never reports do not panic.
        assert_eq!(mmio_load_value(&syndrome(0, true, true), u64::MAX), 0);
        assert_eq!(
            mmio_load_value(&syndrome(u8::MAX, true, true), u64::MAX),
            u64::MAX
        );
    }
}
