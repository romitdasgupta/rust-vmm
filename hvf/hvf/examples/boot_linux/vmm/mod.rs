// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! A minimal arm64 Linux VMM on top of `hvf`.
//!
//! - [`layout`]: the guest physical memory map.
//! - [`fdt`]: the device tree handed to the kernel.
//! - [`pl011`]: the serial console.
//! - [`psci`]: CPU power management and system power off over `HVC`.
//! - [`sysreg`]: the system registers the framework traps.
//! - [`cpu`]: vCPU threads and their exit loop.
//!
//! The in-kernel GICv3 of macOS 15 provides the interrupt controller, the
//! timer interrupts and `WFI` handling, so the VMM needs no interrupt
//! controller or timer model of its own.

pub mod layout;

mod cpu;
mod fdt;
mod pl011;
mod psci;
mod sysreg;
mod time;

use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier, Mutex, PoisonError, Weak};
use std::thread;
use std::time::{Duration, Instant};

use hvf::{GicConfig, GicIntid, Hvf, MemoryFlags, Vcpu, VcpuKicker, Vm, VmConfig};
use linux_loader::cmdline::Cmdline;
use linux_loader::loader::{KernelLoader, PE};
use vm_device::DeviceMmio;
use vm_device::bus::{Bus, BusRange, MmioAddress};
use vm_memory::{Address, Bytes, GuestAddress, GuestMemoryMmap};

use layout::{DTB_MAX_SIZE, GICD_BASE, GICR_BASE, RAM_BASE, SZ_2M, UART_BASE, UART_SIZE, UART_SPI};
use pl011::Pl011;

/// The error type of the VMM.
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// The command line used when none is given: the PL011 as console and
/// early console, and a reset (which ends the VMM) on panic.
pub const DEFAULT_CMDLINE: &str = "console=ttyAMA0 earlycon=pl011,mmio32,0x9000000 panic=-1";

/// The longest command line arm64 Linux accepts, including the NUL.
const COMMAND_LINE_SIZE: usize = 2048;

/// The arm64 Image magic, `"ARM\x64"` at offset 0x38.
const ARM64_IMAGE_MAGIC: &[u8; 4] = b"ARM\x64";

/// The machine to boot.
pub struct Config {
    /// The uncompressed arm64 `Image`.
    pub kernel: PathBuf,
    /// An optional initramfs (for example a gzip'd cpio archive).
    pub initrd: Option<PathBuf>,
    /// The kernel command line; [`DEFAULT_CMDLINE`] when `None`.
    pub cmdline: Option<String>,
    /// RAM size in bytes, a multiple of 2 MiB.
    pub mem_size: u64,
    /// Number of vCPUs.
    pub cpus: usize,
    /// Logs layout, PSCI calls and vCPU state changes to stderr.
    pub verbose: bool,
    /// Where the console output goes.
    pub output: Box<dyn Write + Send>,
    /// Where console input comes from. `Ctrl-A x` in the input stops the
    /// VMM; `Ctrl-A Ctrl-A` sends a `Ctrl-A`.
    pub input: Option<Box<dyn Read + Send>>,
    /// The terminal is in raw mode, so log lines end with `\r\n`.
    pub raw_terminal: bool,
}

/// How the guest ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// PSCI `SYSTEM_OFF`.
    PowerOff,
    /// PSCI `SYSTEM_RESET`. The VMM does not reboot the guest.
    Reset,
    /// `Ctrl-A x` on the console.
    Quit,
}

/// Logging to stderr.
#[derive(Debug, Clone, Copy)]
struct Log {
    verbose: bool,
    raw: bool,
}

impl Log {
    fn print(&self, args: fmt::Arguments<'_>) {
        let eol = if self.raw { "\r\n" } else { "\n" };
        eprint!("[boot_linux] {args}{eol}");
    }
}

type MmioBus = Bus<MmioAddress, Arc<dyn DeviceMmio + Send + Sync>>;

/// State shared by the vCPU threads.
pub struct Machine {
    vm: Vm,
    bus: MmioBus,
    console: Arc<Pl011>,
    cpus: psci::Cpus,
    kickers: Mutex<Vec<Option<VcpuKicker>>>,
    /// Set once, by the first vCPU that stops the machine.
    outcome: Mutex<Option<std::result::Result<Outcome, String>>>,
    warned: Mutex<HashSet<String>>,
    log: Log,
}

impl Machine {
    fn register_kicker(&self, index: usize, kicker: VcpuKicker) {
        self.kickers.lock().unwrap_or_else(PoisonError::into_inner)[index] = Some(kicker);
    }

    /// Stops the machine with `outcome` unless it is already stopping: wakes
    /// the vCPUs that are off, and kicks those that run (or wait in `WFI`).
    fn stop(&self, outcome: std::result::Result<Outcome, String>) {
        {
            let mut slot = self.outcome.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.is_some() {
                return;
            }
            *slot = Some(outcome);
        }
        self.cpus.stop();
        let kickers = self.kickers.lock().unwrap_or_else(PoisonError::into_inner);
        for kicker in kickers.iter().flatten() {
            if let Err(e) = kicker.kick() {
                self.log
                    .print(format_args!("cannot kick vCPU {}: {e}", kicker.id()));
            }
        }
    }

    /// Stops the machine because the guest asked for it.
    fn shutdown(&self, outcome: Outcome) {
        self.stop(Ok(outcome));
    }

    /// Stops the machine because of an error.
    fn fail(&self, message: String) {
        self.stop(Err(message));
    }

    /// Logs `message` the first time it occurs.
    fn warn_once(&self, message: String) {
        let mut warned = self.warned.lock().unwrap_or_else(PoisonError::into_inner);
        if warned.len() < 64 && !warned.contains(&message) {
            self.log.print(format_args!("{message}"));
            warned.insert(message);
        }
    }

    /// Logs when verbose.
    fn debug(&self, args: fmt::Arguments<'_>) {
        if self.log.verbose {
            self.log.print(args);
        }
    }

    /// Logs where the GIC put this vCPU's redistributor.
    fn log_vcpu(&self, index: usize, vcpu: &Vcpu) {
        if !self.log.verbose {
            return;
        }
        let base = vcpu.gic_redistributor_base();
        // GICR_TYPER, at offset 0x8 of the redistributor.
        let typer = vcpu.gic_redistributor_reg(0x8);
        self.log.print(format_args!(
            "vCPU {index}: id {}, MPIDR {:#x}, redistributor {base:x?}, GICR_TYPER {typer:x?}",
            vcpu.id(),
            psci::mpidr(index),
        ));
    }
}

/// Rejects files that are not uncompressed arm64 Images, with a hint.
fn check_image_header(header: &[u8; 64]) -> Result<u64> {
    if header.starts_with(&[0x1f, 0x8b]) {
        return Err("the kernel is gzip-compressed (Image.gz); decompress it first".into());
    }
    if header.starts_with(b"MZ") && &header[4..8] == b"zimg" {
        return Err(
            "the kernel is an EFI zboot image (vmlinuz.efi); this example needs the \
             uncompressed arm64 Image"
                .into(),
        );
    }
    if &header[0x38..0x3c] != ARM64_IMAGE_MAGIC {
        return Err("the kernel is not an arm64 Image (no \"ARM\\x64\" magic)".into());
    }
    // `image_size`, the memory the kernel uses from its load address,
    // including the BSS that is not in the file.
    Ok(u64::from_le_bytes(header[0x10..0x18].try_into().unwrap()))
}

/// Loads the kernel at the start of RAM. Returns the entry point and the
/// first address after the kernel's memory, rounded up to 2 MiB.
fn load_kernel(mem: &GuestMemoryMmap, path: &Path) -> Result<(u64, u64)> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let image_size = check_image_header(&header)?;
    file.rewind()?;
    let loaded = PE::load(mem, Some(GuestAddress(RAM_BASE)), &mut file, None)?;
    let entry = loaded.kernel_load.raw_value();
    // `kernel_end` only covers the file.
    let end = loaded.kernel_end.max(entry + image_size);
    Ok((entry, layout::align_up(end, SZ_2M)))
}

/// Builds the command line with `linux-loader`, which validates it.
fn build_cmdline(cmdline: Option<&str>) -> Result<String> {
    let mut built = Cmdline::new(COMMAND_LINE_SIZE)?;
    built.insert_str(cmdline.unwrap_or(DEFAULT_CMDLINE))?;
    Ok(built.as_cstring()?.into_string()?)
}

/// Random bytes for the guest's RNG and KASLR seeds.
fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Creates the VM.
fn create_vm() -> Result<Vm> {
    let start = Instant::now();
    loop {
        match Hvf::create_vm(&VmConfig::default()) {
            Ok(vm) => return Ok(vm),
            // The previous VM of this process (when `run` is called more
            // than once) is destroyed once the framework has released its
            // vCPUs, a few milliseconds after their threads exit.
            Err(hvf::Error::Hv {
                call: "hv_vm_destroy",
                err,
            }) if err.is_busy() && start.elapsed() < Duration::from_secs(2) => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(hvf::Error::Hv { err, .. }) if err.is_unavailable() => {
                return Err(format!(
                    "Hypervisor.framework is unavailable ({err}); is the binary signed with \
                     the com.apple.security.hypervisor entitlement?"
                )
                .into());
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Forwards console input to the UART, and stops the machine on `Ctrl-A x`.
fn spawn_input(machine: Weak<Machine>, mut input: Box<dyn Read + Send>) -> Result<()> {
    const CTRL_A: u8 = 0x01;
    thread::Builder::new()
        .name("console-input".into())
        .spawn(move || {
            let mut buf = [0u8; 256];
            let mut escape = false;
            loop {
                let n = match input.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => return,
                };
                let Some(machine) = machine.upgrade() else {
                    return;
                };
                let mut bytes = Vec::with_capacity(n);
                for &b in &buf[..n] {
                    if escape {
                        escape = false;
                        match b {
                            b'x' | b'X' => {
                                machine.console.receive(&bytes);
                                machine.shutdown(Outcome::Quit);
                                return;
                            }
                            CTRL_A => bytes.push(CTRL_A),
                            _ => bytes.extend_from_slice(&[CTRL_A, b]),
                        }
                    } else if b == CTRL_A {
                        escape = true;
                    } else {
                        bytes.push(b);
                    }
                }
                machine.console.receive(&bytes);
            }
        })?;
    Ok(())
}

/// Boots the kernel and runs the guest until it powers off or resets.
pub fn run(cfg: Config) -> Result<Outcome> {
    let log = Log {
        verbose: cfg.verbose,
        raw: cfg.raw_terminal,
    };
    let params = match Hvf::gic_params() {
        Ok(params) => params,
        Err(hvf::Error::Unavailable { .. }) => {
            return Err("this example needs the in-kernel GICv3 of macOS 15.0 or newer".into());
        }
        Err(e) => return Err(e.into()),
    };
    let max_cpus = (Hvf::max_vcpu_count()? as usize)
        .min(layout::max_cpus_below_uart(params.redistributor_size) as usize);
    if cfg.cpus == 0 || cfg.cpus > max_cpus {
        return Err(format!("--cpus must be between 1 and {max_cpus}").into());
    }
    if cfg.mem_size == 0 || !cfg.mem_size.is_multiple_of(SZ_2M) {
        return Err("--mem must be a non-zero multiple of 2 MiB".into());
    }

    // Guest RAM, unmapped from the VM before it is dropped.
    let mem = GuestMemoryMmap::<()>::from_ranges(&[(
        GuestAddress(RAM_BASE),
        usize::try_from(cfg.mem_size)?,
    )])?;

    let (entry, kernel_end) = load_kernel(&mem, &cfg.kernel)?;
    let initrd_size = match &cfg.initrd {
        Some(path) => Some(
            std::fs::metadata(path)
                .map_err(|e| format!("{}: {e}", path.display()))?
                .len(),
        ),
        None => None,
    };
    let placement = layout::place(cfg.mem_size, kernel_end, initrd_size)?;
    let initrd = match (&cfg.initrd, placement.initrd, initrd_size) {
        (Some(path), Some(start), Some(size)) => {
            let mut file = File::open(path)?;
            mem.read_exact_volatile_from(GuestAddress(start), &mut file, usize::try_from(size)?)?;
            Some((start, start + size))
        }
        _ => None,
    };

    let cmdline = build_cmdline(cfg.cmdline.as_deref())?;
    let seeds: [u8; 72] = random_bytes()?;
    let dtb = fdt::build(&fdt::FdtParams {
        cpus: u32::try_from(cfg.cpus)?,
        ram_size: cfg.mem_size,
        cmdline: cmdline.clone(),
        initrd,
        redistributor_size: params.redistributor_size,
        vtimer_intid: Hvf::gic_intid(GicIntid::El1VirtualTimer)?,
        ptimer_intid: Hvf::gic_intid(GicIntid::El1PhysicalTimer)?,
        rng_seed: seeds[..64].to_vec(),
        kaslr_seed: u64::from_le_bytes(seeds[64..].try_into()?),
    })?;
    if dtb.len() as u64 > DTB_MAX_SIZE {
        return Err(format!("the DTB is too big ({} bytes)", dtb.len()).into());
    }
    mem.write_slice(&dtb, GuestAddress(placement.dtb))?;

    if log.verbose {
        log.print(format_args!(
            "RAM {RAM_BASE:#x}..{:#x}, kernel {entry:#x}..{kernel_end:#x}, initrd {:x?}, \
             DTB {:#x} ({} bytes), {} vCPUs",
            placement.ram_end,
            initrd,
            placement.dtb,
            dtb.len(),
            cfg.cpus,
        ));
        log.print(format_args!("cmdline: {cmdline}"));
    }

    let vm = create_vm()?;
    // SAFETY: `mem` stays mapped until it is unmapped from the guest at the
    // end of this function, and the host accesses it only through
    // vm-memory's volatile accessors.
    unsafe { vm.map_guest_memory(&mem, MemoryFlags::RWX)? };

    // The GIC must exist before the first vCPU.
    let gic = Arc::new(vm.create_gic(&GicConfig {
        distributor_base: GICD_BASE,
        redistributor_base: GICR_BASE,
        msi: None,
    })?);
    let uart_intid = params.spi_base + UART_SPI;
    let irq_log = log;
    let console = Arc::new(Pl011::new(
        cfg.output,
        Box::new(move |level| {
            if let Err(e) = gic.set_spi(uart_intid, level) {
                irq_log.print(format_args!("cannot set SPI {uart_intid}: {e}"));
            }
        }),
    ));
    let mut bus = MmioBus::new();
    bus.register(
        BusRange::new(MmioAddress(UART_BASE), UART_SIZE)?,
        console.clone(),
    )?;

    let machine = Arc::new(Machine {
        vm,
        bus,
        console,
        cpus: psci::Cpus::new(cfg.cpus, entry, placement.dtb),
        kickers: Mutex::new(vec![None; cfg.cpus]),
        outcome: Mutex::new(None),
        warned: Mutex::new(HashSet::new()),
        log,
    });
    if let Some(input) = cfg.input {
        spawn_input(Arc::downgrade(&machine), input)?;
    }

    let order = Arc::new(cpu::CreationOrder::default());
    let barrier = Arc::new(Barrier::new(cfg.cpus));
    let mut threads = Vec::with_capacity(cfg.cpus);
    for index in 0..cfg.cpus {
        let (machine, order, barrier) = (machine.clone(), order.clone(), barrier.clone());
        threads.push(
            thread::Builder::new()
                .name(format!("vcpu{index}"))
                .spawn(move || cpu::thread(machine, index, order, barrier))?,
        );
    }
    for (index, t) in threads.into_iter().enumerate() {
        if t.join().is_err() {
            machine.fail(format!("vCPU {index} panicked"));
        }
    }

    let outcome = machine
        .outcome
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .unwrap_or_else(|| Err("every vCPU stopped".into()));
    machine.debug(format_args!("guest stopped: {outcome:?}"));

    // Every vCPU is gone; remove the RAM from the guest before freeing it.
    machine.vm.unmap(RAM_BASE, usize::try_from(cfg.mem_size)?)?;
    drop(machine);
    drop(mem);
    outcome.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> [u8; 64] {
        let mut h = [0u8; 64];
        h[0..2].copy_from_slice(b"MZ");
        h[0x10..0x18].copy_from_slice(&0x1e7_0000u64.to_le_bytes());
        h[0x38..0x3c].copy_from_slice(ARM64_IMAGE_MAGIC);
        h
    }

    #[test]
    fn test_check_image_header() {
        assert_eq!(check_image_header(&header()).unwrap(), 0x1e7_0000);

        let mut gzip = header();
        gzip[..2].copy_from_slice(&[0x1f, 0x8b]);
        assert!(
            check_image_header(&gzip)
                .unwrap_err()
                .to_string()
                .contains("gzip")
        );

        let mut zboot = header();
        zboot[4..8].copy_from_slice(b"zimg");
        assert!(
            check_image_header(&zboot)
                .unwrap_err()
                .to_string()
                .contains("zboot")
        );

        let mut elf = header();
        elf[0x38] = 0;
        assert!(check_image_header(&elf).is_err());
    }

    #[test]
    fn test_build_cmdline() {
        assert_eq!(build_cmdline(None).unwrap(), DEFAULT_CMDLINE);
        assert_eq!(build_cmdline(Some("console=hvc0")).unwrap(), "console=hvc0");
        assert!(build_cmdline(Some(&"x".repeat(COMMAND_LINE_SIZE))).is_err());
    }
}
