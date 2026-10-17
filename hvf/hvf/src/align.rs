// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Stateless argument checks run before calling into the framework, so that
//! callers get a precise [`Error`](crate::Error) instead of `HV_BAD_ARGUMENT`.

// The callers are only compiled on macOS/aarch64; the checks are tested on
// every target.
#![cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]

use crate::error::{Error, Result};
use crate::gic_types::{GicConfig, GicParams};

/// The size of an IPA space of `ipa_bits` bits, saturated to `u64::MAX`.
fn ipa_limit(ipa_bits: u32) -> u64 {
    1u64.checked_shl(ipa_bits).unwrap_or(u64::MAX)
}

/// Checks that `value` is a multiple of `align`. An alignment of 0 or 1
/// accepts every value.
fn check_aligned(what: &'static str, value: u64, align: u64) -> Result<()> {
    if align > 1 && !value.is_multiple_of(align) {
        return Err(Error::Misaligned { what, value, align });
    }
    Ok(())
}

/// Checks that `[base, base + size)` lies within an IPA space of `ipa_bits`
/// bits, and returns the end of the range.
fn check_range(what: &'static str, base: u64, size: u64, ipa_bits: u32) -> Result<u64> {
    let limit = ipa_limit(ipa_bits);
    match base.checked_add(size) {
        Some(end) if end <= limit => Ok(end),
        // An overflowing range is reported with the largest possible end.
        end => Err(Error::OutOfRange {
            what,
            value: end.unwrap_or(u64::MAX),
            limit,
        }),
    }
}

/// Checks the arguments of a memory map, unmap or protect call.
///
/// `host` is the host address for `map` (`None` for `unmap`/`protect`).
/// `host`, `ipa` and `size` must be multiples of `granule`, `size` must not be
/// zero, and `[ipa, ipa + size)` must fit in an IPA space of `ipa_bits` bits.
///
/// A zero `size` is reported as [`Error::OutOfRange`] with `what` set to
/// `"size (must be non-zero)"` and `limit` set to the minimum size, `granule`.
pub(crate) fn check_map_args(
    host: Option<usize>,
    ipa: u64,
    size: usize,
    granule: u64,
    ipa_bits: u32,
) -> Result<()> {
    if let Some(host) = host {
        check_aligned("host address", host as u64, granule)?;
    }
    check_aligned("ipa", ipa, granule)?;
    let size = size as u64;
    check_aligned("size", size, granule)?;
    if size == 0 {
        return Err(Error::OutOfRange {
            what: "size (must be non-zero)",
            value: 0,
            limit: granule,
        });
    }
    check_range("ipa + size", ipa, size, ipa_bits)?;
    Ok(())
}

/// Checks a GIC layout against the host's requirements: alignment of every
/// base, no overlap between the distributor, redistributor and MSI regions,
/// every region within an IPA space of `ipa_bits` bits, and the MSI interrupt
/// range within the SPI range.
pub(crate) fn check_gic_layout(cfg: &GicConfig, params: &GicParams, ipa_bits: u32) -> Result<()> {
    check_aligned(
        "distributor_base",
        cfg.distributor_base,
        params.distributor_base_alignment,
    )?;
    check_aligned(
        "redistributor_base",
        cfg.redistributor_base,
        params.redistributor_base_alignment,
    )?;
    if let Some(msi) = &cfg.msi {
        check_aligned("msi.base", msi.base, params.msi_region_base_alignment)?;
    }

    // (name, start, end) of every region.
    let mut regions: [Option<(&'static str, u64, u64)>; 3] = [None; 3];
    let end = check_range(
        "distributor_base + size",
        cfg.distributor_base,
        params.distributor_size,
        ipa_bits,
    )?;
    regions[0] = Some(("distributor", cfg.distributor_base, end));
    let end = check_range(
        "redistributor_base + size",
        cfg.redistributor_base,
        params.redistributor_region_size,
        ipa_bits,
    )?;
    regions[1] = Some(("redistributor", cfg.redistributor_base, end));
    if let Some(msi) = &cfg.msi {
        let end = check_range(
            "msi.base + size",
            msi.base,
            params.msi_region_size,
            ipa_bits,
        )?;
        regions[2] = Some(("msi", msi.base, end));
    }

    let regions: Vec<_> = regions.into_iter().flatten().collect();
    for (i, &(what, start, end)) in regions.iter().enumerate() {
        for &(other, other_start, other_end) in &regions[i + 1..] {
            if start < other_end && other_start < end {
                return Err(Error::Overlap { what, other });
            }
        }
    }

    if let Some(msi) = &cfg.msi {
        if msi.intid_base < params.spi_base {
            return Err(Error::OutOfRange {
                what: "msi.intid_base (must be at least the first SPI)",
                value: u64::from(msi.intid_base),
                limit: u64::from(params.spi_base),
            });
        }
        let spi_end = u64::from(params.spi_base) + u64::from(params.spi_count);
        let msi_end = u64::from(msi.intid_base) + u64::from(msi.intid_count);
        if msi_end > spi_end {
            return Err(Error::OutOfRange {
                what: "msi.intid_base + msi.intid_count",
                value: msi_end,
                limit: spi_end,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gic_types::MsiConfig;

    const K4: u64 = 0x1000;
    const K16: u64 = 0x4000;

    #[test]
    fn test_map_args_valid() {
        for granule in [K4, K16] {
            let g = granule as usize;
            check_map_args(Some(g), granule, g, granule, 36).unwrap();
            check_map_args(None, 0, g, granule, 36).unwrap();
            check_map_args(Some(0x1_0000_0000), 0x4000_0000, 16 * g, granule, 36).unwrap();
            // The range may end exactly at the top of the IPA space.
            check_map_args(None, (1 << 36) - granule, g, granule, 36).unwrap();
            check_map_args(None, 0, 1 << 36, granule, 36).unwrap();
        }
        // 4 KiB aligned values are accepted with a 4 KiB granule only.
        check_map_args(Some(0x3000), 0x1000, 0x1000, K4, 40).unwrap();
    }

    #[test]
    fn test_map_args_misaligned() {
        assert_eq!(
            check_map_args(Some(0x1000), 0x4000, 0x4000, K16, 36),
            Err(Error::Misaligned {
                what: "host address",
                value: 0x1000,
                align: K16
            })
        );
        assert_eq!(
            check_map_args(Some(0x4000), 0x1000, 0x4000, K16, 36),
            Err(Error::Misaligned {
                what: "ipa",
                value: 0x1000,
                align: K16
            })
        );
        assert_eq!(
            check_map_args(None, 0x4000, 0x1000, K16, 36),
            Err(Error::Misaligned {
                what: "size",
                value: 0x1000,
                align: K16
            })
        );
        assert_eq!(
            check_map_args(None, 0x1001, 0x1000, K4, 36),
            Err(Error::Misaligned {
                what: "ipa",
                value: 0x1001,
                align: K4
            })
        );
        assert_eq!(
            check_map_args(Some(0x10), 0x1000, 0x1000, K4, 36),
            Err(Error::Misaligned {
                what: "host address",
                value: 0x10,
                align: K4
            })
        );
        assert_eq!(
            check_map_args(None, 0x1000, 0x1fff, K4, 36),
            Err(Error::Misaligned {
                what: "size",
                value: 0x1fff,
                align: K4
            })
        );
    }

    #[test]
    fn test_map_args_zero_size() {
        for granule in [K4, K16] {
            assert_eq!(
                check_map_args(None, 0, 0, granule, 36),
                Err(Error::OutOfRange {
                    what: "size (must be non-zero)",
                    value: 0,
                    limit: granule
                })
            );
        }
    }

    #[test]
    fn test_map_args_out_of_range() {
        for granule in [K4, K16] {
            let g = granule as usize;
            assert_eq!(
                check_map_args(None, 1 << 36, g, granule, 36),
                Err(Error::OutOfRange {
                    what: "ipa + size",
                    value: (1 << 36) + granule,
                    limit: 1 << 36
                })
            );
            assert_eq!(
                check_map_args(None, (1 << 36) - granule, 2 * g, granule, 36),
                Err(Error::OutOfRange {
                    what: "ipa + size",
                    value: (1 << 36) + granule,
                    limit: 1 << 36
                })
            );
            // Overflow of `ipa + size`.
            let ipa = u64::MAX - granule + 1;
            assert_eq!(
                check_map_args(None, ipa, g, granule, 64),
                Err(Error::OutOfRange {
                    what: "ipa + size",
                    value: u64::MAX,
                    limit: u64::MAX
                })
            );
            assert_eq!(
                check_map_args(None, ipa, 2 * g, granule, 40),
                Err(Error::OutOfRange {
                    what: "ipa + size",
                    value: u64::MAX,
                    limit: 1 << 40
                })
            );
        }
        // A larger IPA space accepts the range.
        check_map_args(None, 1 << 36, 0x4000, K16, 40).unwrap();
    }

    /// The parameters reported by Hypervisor.framework on an M-series host.
    fn params() -> GicParams {
        GicParams {
            distributor_size: 0x1_0000,
            distributor_base_alignment: 0x1_0000,
            redistributor_region_size: 0x200_0000,
            redistributor_size: 0x2_0000,
            redistributor_base_alignment: 0x1_0000,
            msi_region_size: 0x1_0000,
            msi_region_base_alignment: 0x1_0000,
            spi_base: 32,
            spi_count: 988,
        }
    }

    fn config() -> GicConfig {
        GicConfig {
            distributor_base: 0x0800_0000,
            redistributor_base: 0x080a_0000,
            msi: Some(MsiConfig {
                base: 0x0802_0000,
                intid_base: 64,
                intid_count: 64,
            }),
        }
    }

    #[test]
    fn test_gic_layout_valid() {
        check_gic_layout(&config(), &params(), 36).unwrap();
        check_gic_layout(
            &GicConfig {
                msi: None,
                ..config()
            },
            &params(),
            36,
        )
        .unwrap();

        // Adjacent regions do not overlap.
        let cfg = GicConfig {
            distributor_base: 0,
            redistributor_base: 0x2_0000,
            msi: Some(MsiConfig {
                base: 0x1_0000,
                intid_base: 32,
                intid_count: 988,
            }),
        };
        check_gic_layout(&cfg, &params(), 36).unwrap();

        // The redistributor region may end at the top of the IPA space.
        let cfg = GicConfig {
            redistributor_base: (1 << 36) - 0x200_0000,
            ..config()
        };
        check_gic_layout(&cfg, &params(), 36).unwrap();
    }

    #[test]
    fn test_gic_layout_misaligned() {
        let cfg = GicConfig {
            distributor_base: 0x0800_1000,
            ..config()
        };
        assert_eq!(
            check_gic_layout(&cfg, &params(), 36),
            Err(Error::Misaligned {
                what: "distributor_base",
                value: 0x0800_1000,
                align: 0x1_0000
            })
        );

        let cfg = GicConfig {
            redistributor_base: 0x080a_8000,
            ..config()
        };
        assert_eq!(
            check_gic_layout(&cfg, &params(), 36),
            Err(Error::Misaligned {
                what: "redistributor_base",
                value: 0x080a_8000,
                align: 0x1_0000
            })
        );

        let cfg = GicConfig {
            msi: Some(MsiConfig {
                base: 0x0802_0040,
                intid_base: 64,
                intid_count: 64,
            }),
            ..config()
        };
        assert_eq!(
            check_gic_layout(&cfg, &params(), 36),
            Err(Error::Misaligned {
                what: "msi.base",
                value: 0x0802_0040,
                align: 0x1_0000
            })
        );
    }

    #[test]
    fn test_gic_layout_out_of_range() {
        let cfg = GicConfig {
            redistributor_base: 1 << 36,
            ..config()
        };
        assert_eq!(
            check_gic_layout(&cfg, &params(), 36),
            Err(Error::OutOfRange {
                what: "redistributor_base + size",
                value: (1 << 36) + 0x200_0000,
                limit: 1 << 36
            })
        );

        let cfg = GicConfig {
            distributor_base: u64::MAX - 0xffff,
            ..config()
        };
        assert_eq!(
            check_gic_layout(&cfg, &params(), 64),
            Err(Error::OutOfRange {
                what: "distributor_base + size",
                value: u64::MAX,
                limit: u64::MAX
            })
        );

        let cfg = GicConfig {
            msi: Some(MsiConfig {
                base: 0xf_ffff_0000,
                intid_base: 64,
                intid_count: 64,
            }),
            ..config()
        };
        check_gic_layout(&cfg, &params(), 36).unwrap();
        assert_eq!(
            check_gic_layout(&cfg, &params(), 32),
            Err(Error::OutOfRange {
                what: "msi.base + size",
                value: 0x10_0000_0000,
                limit: 1 << 32
            })
        );
    }

    #[test]
    fn test_gic_layout_overlap() {
        // The redistributor region (32 MiB) covers the distributor.
        let cfg = GicConfig {
            distributor_base: 0x0900_0000,
            ..config()
        };
        assert_eq!(
            check_gic_layout(&cfg, &params(), 36),
            Err(Error::Overlap {
                what: "distributor",
                other: "redistributor"
            })
        );

        let cfg = GicConfig {
            msi: Some(MsiConfig {
                base: 0x0800_0000,
                intid_base: 64,
                intid_count: 64,
            }),
            ..config()
        };
        assert_eq!(
            check_gic_layout(&cfg, &params(), 36),
            Err(Error::Overlap {
                what: "distributor",
                other: "msi"
            })
        );

        let cfg = GicConfig {
            msi: Some(MsiConfig {
                base: 0x0a00_0000,
                intid_base: 64,
                intid_count: 64,
            }),
            ..config()
        };
        assert_eq!(
            check_gic_layout(&cfg, &params(), 36),
            Err(Error::Overlap {
                what: "redistributor",
                other: "msi"
            })
        );
    }

    #[test]
    fn test_gic_layout_msi_intids() {
        let msi = |intid_base, intid_count| GicConfig {
            msi: Some(MsiConfig {
                base: 0x0802_0000,
                intid_base,
                intid_count,
            }),
            ..config()
        };

        // The whole SPI range.
        check_gic_layout(&msi(32, 988), &params(), 36).unwrap();
        check_gic_layout(&msi(1019, 1), &params(), 36).unwrap();

        assert_eq!(
            check_gic_layout(&msi(16, 64), &params(), 36),
            Err(Error::OutOfRange {
                what: "msi.intid_base (must be at least the first SPI)",
                value: 16,
                limit: 32
            })
        );
        assert_eq!(
            check_gic_layout(&msi(1000, 64), &params(), 36),
            Err(Error::OutOfRange {
                what: "msi.intid_base + msi.intid_count",
                value: 1064,
                limit: 1020
            })
        );
        // No overflow with extreme values.
        assert_eq!(
            check_gic_layout(&msi(u32::MAX, u32::MAX), &params(), 36),
            Err(Error::OutOfRange {
                what: "msi.intid_base + msi.intid_count",
                value: 2 * u64::from(u32::MAX),
                limit: 1020
            })
        );
    }
}
