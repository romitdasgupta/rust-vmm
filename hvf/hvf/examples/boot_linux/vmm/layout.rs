// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Guest physical memory map, and where the kernel, initrd and DTB go.
//!
//! The MMIO part follows QEMU's `virt` machine, so the addresses are familiar
//! from QEMU boot logs:
//!
//! ```text
//! 0x0800_0000  GICv3 distributor (64 KiB)
//! 0x080a_0000  GICv3 redistributors (one 128 KiB frame per vCPU)
//! 0x0900_0000  PL011 UART (4 KiB), SPI 1 = INTID 33
//! 0x8000_0000  RAM: kernel Image, then initrd, ..., DTB in the last 2 MiB
//! ```

/// Base of the GICv3 distributor.
pub const GICD_BASE: u64 = 0x0800_0000;
/// Base of the GICv3 redistributor region.
pub const GICR_BASE: u64 = 0x080a_0000;
/// Base of the PL011 UART.
pub const UART_BASE: u64 = 0x0900_0000;
/// Size of the PL011 register window. Linux's AMBA bus reads the
/// identification registers from the last 32 bytes, so it must be 4 KiB.
pub const UART_SIZE: u64 = 0x1000;
/// The PL011's interrupt as a device tree SPI number (INTID 32 + 1).
pub const UART_SPI: u32 = 1;
/// Base of guest RAM.
pub const RAM_BASE: u64 = 0x8000_0000;

/// 2 MiB, the alignment of the kernel Image and of the DTB block.
pub const SZ_2M: u64 = 0x20_0000;
/// The DTB lives alone in the last 2 MiB of RAM, so it may be up to that big.
pub const DTB_MAX_SIZE: u64 = SZ_2M;
/// Alignment of the initrd.
const INITRD_ALIGN: u64 = 0x1_0000;

/// Rounds `value` up to a multiple of `align` (a power of two).
pub fn align_up(value: u64, align: u64) -> u64 {
    value.next_multiple_of(align)
}

/// The number of redistributor frames that fit between [`GICR_BASE`] and
/// the UART, which bounds the number of vCPUs.
pub fn max_cpus_below_uart(redistributor_size: u64) -> u64 {
    (UART_BASE - GICR_BASE) / redistributor_size
}

/// Where the boot payloads are placed in RAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// First byte after RAM.
    pub ram_end: u64,
    /// Guest address of the DTB.
    pub dtb: u64,
    /// Guest address of the initrd, if there is one.
    pub initrd: Option<u64>,
}

/// Places the initrd after the kernel's reservation (`kernel_end`, which
/// includes the BSS) and the DTB at the start of the last 2 MiB of RAM.
pub fn place(
    ram_size: u64,
    kernel_end: u64,
    initrd_size: Option<u64>,
) -> Result<Placement, String> {
    let ram_end = RAM_BASE + ram_size;
    let dtb = ram_end
        .checked_sub(DTB_MAX_SIZE)
        .filter(|&dtb| dtb >= kernel_end)
        .ok_or_else(|| {
            format!(
                "{} MiB of RAM is too small for the kernel ({} MiB including its BSS) and the DTB",
                ram_size >> 20,
                (kernel_end - RAM_BASE) >> 20
            )
        })?;
    let initrd = match initrd_size {
        None => None,
        Some(size) => {
            let start = align_up(kernel_end, INITRD_ALIGN);
            if start + size > dtb {
                return Err(format!(
                    "the initrd ({size} bytes) does not fit between the kernel and the DTB; \
                     use a larger --mem"
                ));
            }
            Some(start)
        }
    };
    Ok(Placement {
        ram_end,
        dtb,
        initrd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_place() {
        let kernel_end = RAM_BASE + 0x200_0000;
        let p = place(0x2000_0000, kernel_end, Some(0x1000)).unwrap();
        assert_eq!(p.ram_end, 0xa000_0000);
        assert_eq!(p.dtb, 0x9fe0_0000);
        assert_eq!(p.initrd, Some(kernel_end));
        assert!(place(0x200_0000, kernel_end, None).is_err());
        // 36 MiB: the DTB starts 2 MiB after the kernel.
        assert!(place(0x240_0000, kernel_end, Some(0x20_0000)).is_ok());
        assert!(place(0x240_0000, kernel_end, Some(0x20_0001)).is_err());
        assert_eq!(max_cpus_below_uart(0x2_0000), 123);
    }
}
