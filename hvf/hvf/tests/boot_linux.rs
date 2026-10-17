// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Boots Linux to userspace with the VMM of `examples/boot_linux`.
//!
//! The boot test is ignored by default and needs a kernel and an initramfs,
//! which this repository does not ship:
//!
//! ```sh
//! HVF_LINUX_KERNEL=/path/to/Image HVF_LINUX_INITRD=/path/to/initramfs.cpio.gz \
//!     cargo test -p hvf --features vm-memory --test boot_linux -- --ignored --nocapture
//! ```
//!
//! The kernel is an uncompressed arm64 `Image` with the PL011 console built
//! in. The initramfs's `/init` must print `HVF-BOOT-OK` on the console and
//! then power the machine off, for example with
//! `reboot(LINUX_REBOOT_CMD_POWER_OFF)` or busybox's `poweroff -f`.
//! `HVF_LINUX_CMDLINE` replaces the default command line. The test fails
//! without `HVF_LINUX_KERNEL` and `HVF_LINUX_INITRD`: it runs only when
//! asked for, and a skip would be indistinguishable from a boot, because
//! libtest hides what a passing test prints.
//!
//! The file also runs the unit tests of the example's modules, which need
//! no hypervisor.

#![cfg(all(target_os = "macos", target_arch = "aarch64", feature = "vm-memory"))]

mod common;
#[path = "../examples/boot_linux/vmm/mod.rs"]
mod vmm;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use vmm::{Config, Outcome};

/// What `/init` prints once it runs.
const MARKER: &str = "HVF-BOOT-OK";

/// A console output buffer.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Boots the kernel with `cpus` vCPUs and returns how it ended and the
/// console output.
fn boot(kernel: &Path, initrd: &Path, cpus: usize) -> (vmm::Result<Outcome>, String) {
    let capture = Capture::default();
    let outcome = vmm::run(Config {
        kernel: kernel.to_path_buf(),
        initrd: Some(initrd.to_path_buf()),
        cmdline: std::env::var("HVF_LINUX_CMDLINE").ok(),
        mem_size: 512 << 20,
        cpus,
        verbose: false,
        output: Box::new(capture.clone()),
        input: None,
        raw_terminal: false,
    });
    let output = String::from_utf8_lossy(&capture.0.lock().unwrap()).into_owned();
    (outcome, output)
}

#[test]
#[ignore = "needs HVF_LINUX_KERNEL and HVF_LINUX_INITRD; run with --ignored"]
fn test_boot_linux_to_userspace() {
    let (Some(kernel), Some(initrd)) = (
        std::env::var_os("HVF_LINUX_KERNEL"),
        std::env::var_os("HVF_LINUX_INITRD"),
    ) else {
        panic!("set HVF_LINUX_KERNEL and HVF_LINUX_INITRD (absolute paths) to boot Linux");
    };
    let Some(_g) = common::hv_guard() else {
        return;
    };
    let (kernel, initrd) = (PathBuf::from(kernel), PathBuf::from(initrd));

    for cpus in [1, 4] {
        let (outcome, output) = boot(&kernel, &initrd, cpus);
        println!("----- console, {cpus} vCPU(s) -----\n{output}");
        assert!(
            matches!(outcome, Ok(Outcome::PowerOff)),
            "{cpus} vCPU(s): the guest did not power off: {outcome:?}"
        );
        assert!(
            output.contains(MARKER),
            "{cpus} vCPU(s): {MARKER} missing from the console output"
        );
        // "smp: Brought up 1 node, 4 CPUs"
        let smp = format!("Brought up 1 node, {cpus} CPU");
        assert!(output.contains(&smp), "{cpus} vCPU(s): {smp:?} missing");
    }
}
