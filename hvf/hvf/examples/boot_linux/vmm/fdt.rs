// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! The flattened device tree that describes the machine to Linux.

use vm_fdt::{FdtWriter, FdtWriterResult};

use super::layout::{GICD_BASE, GICR_BASE, RAM_BASE, UART_BASE, UART_SIZE, UART_SPI};
use super::psci;

/// Phandle of the interrupt controller.
const PHANDLE_GIC: u32 = 1;
/// Phandle of the fixed clock that feeds the UART.
const PHANDLE_CLK: u32 = 2;

/// `GIC_SPI` in the first interrupt cell.
const GIC_SPI: u32 = 0;
/// `GIC_PPI` in the first interrupt cell.
const GIC_PPI: u32 = 1;
/// `IRQ_TYPE_LEVEL_HIGH` in the third interrupt cell.
const IRQ_LEVEL_HIGH: u32 = 4;
/// INTID of the first PPI.
const PPI_BASE: u32 = 16;

/// What the device tree describes.
#[derive(Debug, Clone)]
pub struct FdtParams {
    /// Number of vCPUs; vCPU `i` has `MPIDR_EL1` [`psci::mpidr(i)`](psci::mpidr).
    pub cpus: u32,
    /// RAM size in bytes, starting at [`RAM_BASE`].
    pub ram_size: u64,
    /// The kernel command line.
    pub cmdline: String,
    /// Guest address range `[start, end)` of the initrd.
    pub initrd: Option<(u64, u64)>,
    /// Size of one redistributor frame.
    pub redistributor_size: u64,
    /// INTID of the EL1 virtual timer, as assigned by the framework.
    pub vtimer_intid: u32,
    /// INTID of the EL1 physical timer, as assigned by the framework.
    pub ptimer_intid: u32,
    /// Seed for the guest's random number generator (`/chosen/rng-seed`).
    pub rng_seed: Vec<u8>,
    /// Seed for the kernel's address space randomization.
    pub kaslr_seed: u64,
}

/// The `reg` value of one `(address, size)` range with
/// `#address-cells = #size-cells = 2`.
fn reg64(base: u64, size: u64) -> [u64; 2] {
    [base, size]
}

/// The `reg` of the node of vCPU `index`: its MPIDR[23:0].
fn cpu_reg(index: u32) -> u32 {
    (psci::mpidr(index as usize) & 0xff_ffff) as u32
}

/// Builds the device tree blob.
pub fn build(p: &FdtParams) -> FdtWriterResult<Vec<u8>> {
    let mut fdt = FdtWriter::new()?;

    let root = fdt.begin_node("")?;
    fdt.property_string("compatible", "linux,dummy-virt")?;
    fdt.property_string("model", "rust-vmm hvf boot_linux")?;
    fdt.property_u32("#address-cells", 2)?;
    fdt.property_u32("#size-cells", 2)?;
    fdt.property_u32("interrupt-parent", PHANDLE_GIC)?;

    // With `#address-cells = 1`, `reg` holds MPIDR[23:0] (Aff2, Aff1, Aff0),
    // which must match for Linux to find the boot CPU in the tree.
    let cpus = fdt.begin_node("cpus")?;
    fdt.property_u32("#address-cells", 1)?;
    fdt.property_u32("#size-cells", 0)?;
    for i in 0..p.cpus {
        let reg = cpu_reg(i);
        let cpu = fdt.begin_node(&format!("cpu@{reg:x}"))?;
        fdt.property_string("device_type", "cpu")?;
        fdt.property_string("compatible", "arm,arm-v8")?;
        fdt.property_u32("reg", reg)?;
        fdt.property_string("enable-method", "psci")?;
        fdt.end_node(cpu)?;
    }
    fdt.end_node(cpus)?;

    let memory = fdt.begin_node(&format!("memory@{RAM_BASE:x}"))?;
    fdt.property_string("device_type", "memory")?;
    fdt.property_array_u64("reg", &reg64(RAM_BASE, p.ram_size))?;
    fdt.end_node(memory)?;

    let uart_path = format!("/pl011@{UART_BASE:x}");
    let chosen = fdt.begin_node("chosen")?;
    fdt.property_string("bootargs", &p.cmdline)?;
    fdt.property_string("stdout-path", &uart_path)?;
    if let Some((start, end)) = p.initrd {
        fdt.property_u64("linux,initrd-start", start)?;
        fdt.property_u64("linux,initrd-end", end)?;
    }
    fdt.property("rng-seed", &p.rng_seed)?;
    fdt.property_u64("kaslr-seed", p.kaslr_seed)?;
    fdt.end_node(chosen)?;

    let aliases = fdt.begin_node("aliases")?;
    fdt.property_string("serial0", &uart_path)?;
    fdt.end_node(aliases)?;

    // PSCI 1.0 over HVC; the vCPU loop implements it (see `psci.rs`).
    let psci = fdt.begin_node("psci")?;
    fdt.property_string_list(
        "compatible",
        vec![
            "arm,psci-1.0".into(),
            "arm,psci-0.2".into(),
            "arm,psci".into(),
        ],
    )?;
    fdt.property_string("method", "hvc")?;
    fdt.end_node(psci)?;

    // The in-kernel GICv3. No ITS (the framework reports no LPIs), no
    // maintenance interrupt (no EL2), and only the redistributors of the
    // vCPUs that exist.
    let gic = fdt.begin_node(&format!("intc@{GICD_BASE:x}"))?;
    fdt.property_string("compatible", "arm,gic-v3")?;
    fdt.property_null("interrupt-controller")?;
    fdt.property_u32("#interrupt-cells", 3)?;
    fdt.property_u32("#redistributor-regions", 1)?;
    let gicr_size = u64::from(p.cpus) * p.redistributor_size;
    fdt.property_array_u64("reg", &[GICD_BASE, 0x1_0000, GICR_BASE, gicr_size])?;
    fdt.property_phandle(PHANDLE_GIC)?;
    fdt.end_node(gic)?;

    // The architected timer PPIs, in the binding's order: secure physical,
    // non-secure physical, virtual, hypervisor.
    let ppi = |intid: u32| [GIC_PPI, intid - PPI_BASE, IRQ_LEVEL_HIGH];
    let timer = fdt.begin_node("timer")?;
    fdt.property_string_list(
        "compatible",
        vec!["arm,armv8-timer".into(), "arm,armv7-timer".into()],
    )?;
    fdt.property_null("always-on")?;
    let mut interrupts = Vec::new();
    for intid in [29, p.ptimer_intid, p.vtimer_intid, 26] {
        interrupts.extend_from_slice(&ppi(intid));
    }
    fdt.property_array_u32("interrupts", &interrupts)?;
    fdt.end_node(timer)?;

    let clock = fdt.begin_node("apb-pclk")?;
    fdt.property_string("compatible", "fixed-clock")?;
    fdt.property_u32("#clock-cells", 0)?;
    fdt.property_u32("clock-frequency", 24_000_000)?;
    fdt.property_string("clock-output-names", "clk24mhz")?;
    fdt.property_phandle(PHANDLE_CLK)?;
    fdt.end_node(clock)?;

    // "arm,primecell" makes Linux create an AMBA device, which is what the
    // amba-pl011 driver binds to.
    let uart = fdt.begin_node(&format!("pl011@{UART_BASE:x}"))?;
    fdt.property_string_list(
        "compatible",
        vec!["arm,pl011".into(), "arm,primecell".into()],
    )?;
    fdt.property_array_u64("reg", &reg64(UART_BASE, UART_SIZE))?;
    fdt.property_array_u32("interrupts", &[GIC_SPI, UART_SPI, IRQ_LEVEL_HIGH])?;
    fdt.property_array_u32("clocks", &[PHANDLE_CLK, PHANDLE_CLK])?;
    fdt.property_string_list("clock-names", vec!["uartclk".into(), "apb_pclk".into()])?;
    fdt.end_node(uart)?;

    fdt.end_node(root)?;
    fdt.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> FdtParams {
        FdtParams {
            cpus: 4,
            ram_size: 0x2000_0000,
            cmdline: "console=ttyAMA0".into(),
            initrd: Some((0x8200_0000, 0x8200_1000)),
            redistributor_size: 0x2_0000,
            vtimer_intid: 27,
            ptimer_intid: 30,
            rng_seed: vec![0; 64],
            kaslr_seed: 0,
        }
    }

    /// Finds `needle` in `haystack`.
    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn test_build() {
        let dtb = build(&params()).unwrap();
        // FDT magic.
        assert_eq!(dtb[..4], [0xd0, 0x0d, 0xfe, 0xed]);
        assert!((dtb.len() as u64) < super::super::layout::DTB_MAX_SIZE);
        for s in [
            &b"cpu@3\0"[..],
            b"intc@8000000\0",
            b"pl011@9000000\0",
            b"arm,psci-1.0\0",
            b"console=ttyAMA0\0",
        ] {
            assert!(
                contains(&dtb, s),
                "{:?} missing",
                String::from_utf8_lossy(s)
            );
        }
        // The redistributor range covers exactly four frames.
        let gic_reg: Vec<u8> = [GICD_BASE, 0x1_0000, GICR_BASE, 0x8_0000]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect();
        assert!(contains(&dtb, &gic_reg));
        // Timer PPIs: 13, 14, 11, 10.
        let timer: Vec<u8> = [1u32, 13, 4, 1, 14, 4, 1, 11, 4, 1, 10, 4]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect();
        assert!(contains(&dtb, &timer));
    }

    #[test]
    fn test_cpu_nodes_in_clusters() {
        assert_eq!(cpu_reg(15), 0xf);
        assert_eq!(cpu_reg(16), 0x100);
        let mut p = params();
        p.cpus = 20;
        let dtb = build(&p).unwrap();
        for s in [&b"cpu@f\0"[..], b"cpu@100\0", b"cpu@103\0"] {
            assert!(
                contains(&dtb, s),
                "{:?} missing",
                String::from_utf8_lossy(s)
            );
        }
        assert!(!contains(&dtb, b"cpu@10\0"));
        assert!(!contains(&dtb, b"cpu@104\0"));
    }

    #[test]
    fn test_build_without_initrd() {
        let mut p = params();
        p.initrd = None;
        let dtb = build(&p).unwrap();
        assert!(!contains(&dtb, b"linux,initrd-start"));
    }
}
