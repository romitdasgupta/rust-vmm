// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Boots an arm64 Linux kernel on Hypervisor.framework.
//!
//! ```sh
//! cargo run -p hvf --features vm-memory --example boot_linux -- \
//!     --kernel Image [--initrd initramfs.cpio.gz] [--cmdline "..."] \
//!     [--mem 512M] [--cpus N] [--verbose]
//! ```
//!
//! The kernel must be an uncompressed arm64 `Image` with PL011, GICv3, the
//! architected timer and PSCI built in (arm64 `defconfig` has them). The
//! machine has a PL011 console on stdin/stdout, the in-kernel GICv3 of
//! macOS 15 and PSCI 1.0 over `HVC` for SMP and power off. `Ctrl-A x` stops
//! the VMM.
//!
//! The exit status is 0 after the guest powers off, 3 after it resets (for
//! example after a panic with `panic=-1`), 130 after `Ctrl-A x`, and 1 on
//! errors.
//!
//! The pieces of the VMM are in the `vmm` module, built from rust-vmm
//! crates: `vm-memory` for guest RAM, `linux-loader` for the kernel Image
//! and the command line, `vm-device` for the MMIO bus and `vm-fdt` for the
//! device tree.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod vmm;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod cli {
    use std::path::PathBuf;

    use super::vmm::{self, Config, Outcome};

    const USAGE: &str = "\
usage: boot_linux --kernel <Image> [--initrd <file>] [--cmdline <args>]
                  [--mem <size>] [--cpus <n>] [--verbose]

  --kernel <Image>   uncompressed arm64 Linux Image
  --initrd <file>    initramfs, for example a gzip'd cpio archive
  --cmdline <args>   kernel command line (default: \"console=ttyAMA0
                     earlycon=pl011,mmio32,0x9000000 panic=-1\")
  --mem <size>       RAM size, with an optional K, M or G suffix; a bare
                     number is MiB (default: 512M)
  --cpus <n>         number of vCPUs (default: 1)
  --verbose, -v      log the memory layout, PSCI calls and vCPU state

Ctrl-A x stops the VMM.";

    /// Parsed command line arguments.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Args {
        pub kernel: PathBuf,
        pub initrd: Option<PathBuf>,
        pub cmdline: Option<String>,
        pub mem_size: u64,
        pub cpus: usize,
        pub verbose: bool,
    }

    /// Parses a size such as `512M`, `1G` or `256` (MiB).
    pub fn parse_size(s: &str) -> Result<u64, String> {
        let (digits, shift) = match s.char_indices().last() {
            Some((i, 'k' | 'K')) => (&s[..i], 10),
            Some((i, 'm' | 'M')) => (&s[..i], 20),
            Some((i, 'g' | 'G')) => (&s[..i], 30),
            _ => (s, 20),
        };
        digits
            .parse::<u64>()
            .ok()
            .and_then(|n| n.checked_mul(1 << shift))
            .ok_or_else(|| format!("invalid size {s:?}"))
    }

    /// Parses the arguments after the program name. `Ok(None)` means
    /// `--help`.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Args>, String> {
        let mut args = args.into_iter();
        let mut kernel = None;
        let mut parsed = Args {
            kernel: PathBuf::new(),
            initrd: None,
            cmdline: None,
            mem_size: 512 << 20,
            cpus: 1,
            verbose: false,
        };
        while let Some(arg) = args.next() {
            // Accept both `--opt value` and `--opt=value`.
            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) if name.starts_with("--") => {
                    (name.to_string(), Some(value.to_string()))
                }
                _ => (arg, None),
            };
            let mut value = || {
                inline
                    .clone()
                    .or_else(|| args.next())
                    .ok_or_else(|| format!("{name} needs a value"))
            };
            match name.as_str() {
                "--kernel" => kernel = Some(PathBuf::from(value()?)),
                "--initrd" => parsed.initrd = Some(PathBuf::from(value()?)),
                "--cmdline" => parsed.cmdline = Some(value()?),
                "--mem" => parsed.mem_size = parse_size(&value()?)?,
                "--cpus" => {
                    let v = value()?;
                    parsed.cpus = v.parse().map_err(|_| format!("invalid --cpus {v:?}"))?;
                }
                "--verbose" | "-v" => parsed.verbose = true,
                "--help" | "-h" => return Ok(None),
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        parsed.kernel = kernel.ok_or("--kernel is required")?;
        Ok(Some(parsed))
    }

    /// Puts the terminal on stdin into raw mode while it lives, so that
    /// keys reach the guest unprocessed (`Ctrl-C` included).
    struct RawTerminal {
        saved: libc::termios,
    }

    impl RawTerminal {
        /// Enables raw mode if stdin is a terminal.
        fn enable() -> Option<RawTerminal> {
            // SAFETY: `isatty` only inspects the file descriptor.
            if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
                return None;
            }
            // SAFETY: an all-zero `termios` is a valid value of the plain C
            // struct, overwritten by `tcgetattr`.
            let mut saved: libc::termios = unsafe { std::mem::zeroed() };
            // SAFETY: `saved` is a valid, writable `termios`.
            if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut saved) } != 0 {
                return None;
            }
            let mut raw = saved;
            // SAFETY: `raw` is a valid `termios`; `cfmakeraw` only changes
            // its flags.
            unsafe { libc::cfmakeraw(&mut raw) };
            // SAFETY: `raw` is a valid `termios`.
            if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
                return None;
            }
            Some(RawTerminal { saved })
        }
    }

    impl Drop for RawTerminal {
        fn drop(&mut self) {
            // SAFETY: `saved` is the valid `termios` read in `enable`.
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved) };
        }
    }

    /// Runs the VMM and returns the process exit status.
    pub fn main() -> i32 {
        let args = match parse(std::env::args().skip(1)) {
            Ok(Some(args)) => args,
            Ok(None) => {
                println!("{USAGE}");
                return 0;
            }
            Err(e) => {
                eprintln!("boot_linux: {e}\n{USAGE}");
                return 2;
            }
        };

        let raw = RawTerminal::enable();
        let result = vmm::run(Config {
            kernel: args.kernel,
            initrd: args.initrd,
            cmdline: args.cmdline,
            mem_size: args.mem_size,
            cpus: args.cpus,
            verbose: args.verbose,
            output: Box::new(std::io::stdout()),
            input: Some(Box::new(std::io::stdin())),
            raw_terminal: raw.is_some(),
        });
        drop(raw);

        match result {
            Ok(Outcome::PowerOff) => {
                eprintln!("boot_linux: the guest powered off");
                0
            }
            Ok(Outcome::Reset) => {
                eprintln!("boot_linux: the guest requested a reset");
                3
            }
            Ok(Outcome::Quit) => {
                eprintln!("boot_linux: stopped by Ctrl-A x");
                130
            }
            Err(e) => {
                eprintln!("boot_linux: {e}");
                1
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn args(list: &[&str]) -> Result<Option<Args>, String> {
            parse(list.iter().map(|s| s.to_string()))
        }

        #[test]
        fn test_parse_size() {
            assert_eq!(parse_size("512M"), Ok(512 << 20));
            assert_eq!(parse_size("1g"), Ok(1 << 30));
            assert_eq!(parse_size("2048K"), Ok(2 << 20));
            assert_eq!(parse_size("256"), Ok(256 << 20));
            assert!(parse_size("M").is_err());
            assert!(parse_size("1T").is_err());
        }

        #[test]
        fn test_parse() {
            let a = args(&["--kernel", "Image", "--cpus=4", "--mem", "1G", "-v"])
                .unwrap()
                .unwrap();
            assert_eq!(a.kernel, PathBuf::from("Image"));
            assert_eq!((a.cpus, a.mem_size, a.verbose), (4, 1 << 30, true));
            let a = args(&["--kernel=Image", "--cmdline", "console=ttyAMA0 quiet"])
                .unwrap()
                .unwrap();
            assert_eq!(a.cmdline.as_deref(), Some("console=ttyAMA0 quiet"));
            assert_eq!(args(&["--help"]), Ok(None));
            assert!(args(&[]).is_err());
            assert!(args(&["--kernel"]).is_err());
            assert!(args(&["--kernel", "Image", "--bogus"]).is_err());
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() {
    std::process::exit(cli::main());
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("This example requires macOS on Apple silicon.");
}
