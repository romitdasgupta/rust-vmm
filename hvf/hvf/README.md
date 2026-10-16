# hvf

Safe wrappers over the macOS
[Hypervisor.framework](https://developer.apple.com/documentation/hypervisor)
(HVF) on Apple silicon, in the spirit of [kvm-ioctls](../../kvm/kvm-ioctls).

The crate is a thin layer. It exposes what the framework does and checks
arguments the framework would otherwise reject with an opaque error. VMM
policy (memory layout, boot protocol, device models, interrupt routing, PSCI)
is left to the caller.

- `Hvf` probes the host and creates the (single, per-process) `Vm`.
- `Vm` maps guest memory and creates `Vcpu`s and the optional in-kernel
  GICv3 (`Gic`).
- `Vcpu` runs guest code and accesses registers. It is bound to the thread
  that created it; a `VcpuKicker` interrupts it from any thread.
- `VcpuExit` and `ExceptionClass` decode exits, including the MMIO data
  aborts a VMM has to emulate.

The raw FFI declarations live in [hvf-bindings](../hvf-bindings).

## Example

```rust
use hvf::{ExceptionClass, Hvf, MemoryFlags, Reg, VcpuExit, VmConfig};

let vm = Hvf::create_vm(&VmConfig::default())?;
// `host` is a page-aligned anonymous mapping holding the guest code.
unsafe { vm.map(host, 0x4000_0000, size, MemoryFlags::READ | MemoryFlags::EXEC)? };

let mut vcpu = vm.create_vcpu(None)?;
vcpu.set_reg(Reg::PC, 0x4000_0000)?;
vcpu.set_reg(Reg::CPSR, 0x3c5)?; // EL1h, interrupts masked.

loop {
    match vcpu.run()? {
        VcpuExit::Exception(e) => match e.decode() {
            ExceptionClass::DataAbort(abort) => { /* emulate MMIO, see below */ }
            ExceptionClass::Hvc { imm } => { /* PSCI or hypercall; PC already advanced */ }
            ExceptionClass::Wfx { .. } => break,
            other => panic!("unexpected exception: {other:?}"),
        },
        VcpuExit::Canceled => { /* kicked by a VcpuKicker */ }
        VcpuExit::VtimerActivated => { /* see "Virtual timer" */ }
        other => panic!("unexpected exit: {other:?}"),
    }
}
```

The crate documentation has a complete, runnable version of this example, and
[`examples/mmio_guest.rs`](examples/mmio_guest.rs) drives a guest through a
`vm-device` MMIO bus:

```sh
cargo run -p hvf --example mmio_guest
```

## Booting Linux

[`examples/boot_linux`](examples/boot_linux) is a small VMM that boots an
arm64 Linux kernel to userspace (macOS 15.0 or newer):

```sh
cargo run --release -p hvf --features vm-memory --example boot_linux -- \
    --kernel Image --initrd initramfs.cpio.gz --cpus 4 --mem 512M
```

The VMM puts these pieces together:

| Piece | How |
|---|---|
| RAM | `vm-memory` `GuestMemoryMmap` at `0x8000_0000`, mapped with `Vm::map_guest_memory` |
| Kernel | `linux-loader`'s `PE` loader for the arm64 `Image`. The initrd follows the kernel's `image_size`, and the DTB sits in the last 2 MiB of RAM |
| Device tree | `vm-fdt`: CPUs, memory, `/chosen` (command line, initrd, RNG seed), PSCI, GICv3, the architected timer and the PL011 |
| Interrupts and timer | the in-kernel GICv3 (`Vm::create_gic`) at QEMU `virt` addresses: distributor `0x0800_0000`, redistributors `0x080a_0000` |
| Console | an emulated PL011 at `0x0900_0000` on a `vm-device` MMIO bus. It writes to stdout and reads from stdin (raw mode on a terminal), raising SPI 1 with `Gic::set_spi` |
| SMP and power | PSCI 1.0 over `HVC`: `CPU_ON`, `CPU_OFF` and `AFFINITY_INFO` with one thread per vCPU, and `SYSTEM_OFF`/`SYSTEM_RESET` to stop the VMM |
| Traps | the OS lock and DCC registers Linux touches. Everything else is RAZ/WI |

- **Kernel.** The kernel must be an uncompressed `Image`, not `Image.gz`
  or an EFI zboot `vmlinuz`. It needs `CONFIG_SERIAL_AMBA_PL011_CONSOLE`,
  `CONFIG_ARM_GIC_V3`, `CONFIG_ARM_ARCH_TIMER` and `CONFIG_ARM_PSCI_FW`, which
  arm64 `defconfig` and Debian's `cloud-arm64` kernel have. Kernels without a
  PL011 driver (Firecracker's CI kernels, for example) boot and power off,
  but print nothing.
- **Command line.** The default is `console=ttyAMA0
  earlycon=pl011,mmio32,0x9000000 panic=-1`.
- **Exit status.** The VMM exits with 0 when the guest powers off, 3 when
  it resets (for example after a panic with `panic=-1`), 130 after
  `Ctrl-A x` and 1 on errors. `--verbose` logs the memory layout, the PSCI
  calls and the vCPU power state changes.

No kernel or initramfs is included in this repository. An initramfs whose
`/init` prints `HVF-BOOT-OK` and then powers off, for example with
`reboot(LINUX_REBOOT_CMD_POWER_OFF)` or `poweroff -f`, can be checked with the
ignored test in [`tests/boot_linux.rs`](tests/boot_linux.rs). The test boots
with 1 and 4 vCPUs. Without the variables it prints `SKIP` and passes:

```sh
HVF_LINUX_KERNEL=Image HVF_LINUX_INITRD=initramfs.cpio.gz \
    cargo test -p hvf --features vm-memory --test boot_linux -- --ignored
```

## Supported platforms

- **`aarch64-apple-darwin`, macOS 11.0 or newer.** Only functions available
  since macOS 11.0 are linked directly. Newer functions are looked up at run
  time, so one binary runs on every release. Where a function is missing,
  the method that needs it returns `Error::Unavailable { api, since }`.
- **Other targets** (Linux, x86_64 macOS) build only the portable parts:
  errors, register identifiers, exit and syndrome decoding, MMIO value
  arithmetic, and the alignment and GIC layout checks. These parts are
  tested in rust-vmm's Linux CI. Everything that calls the framework is
  compiled only on `aarch64-apple-darwin`.

What each macOS release adds:

| macOS | Functionality |
|---|---|
| 11.0 | VM, memory mapping, vCPUs, registers, SIMD/FP, IRQ/FIQ lines, virtual timer, debug traps, `Hvf::max_vcpu_count` |
| 13.0 | `VmConfig::ipa_bits`, `Hvf::max_ipa_bits`, `Hvf::default_ipa_bits` (older releases always use 36 bits) |
| 15.0 | In-kernel GICv3 (`Vm::create_gic`, `Gic`, `Vcpu::gic_*`), `VmConfig::el2` and `Hvf::el2_supported` (EL2 also needs M3 or newer) |
| 15.2 | `Hvf::sme_max_svl_bytes` |
| 26.0 | 4 KiB stage-2 granule (`VmConfig::ipa_granule`, `Hvf::default_ipa_granule`) |
| 27.0 | `Vcpu::wfi_time`, `Vcpu::serror_pending` / `set_serror`, `Vcpu::invalidate_tlb` |

The minimum supported Rust version is 1.88. That is the first stable release
with naked functions, which `hvf-bindings` needs for its SIMD register setter.

## Signing

The framework refuses to create a VM unless the calling binary is signed
with the `com.apple.security.hypervisor` entitlement. Without it,
`Hvf::create_vm` fails with `HV_DENIED`. An ad-hoc signature is enough:

```sh
codesign --sign - --force --entitlements hvf/scripts/hvf.entitlements <binary>
```

In this repository, `cargo test`, `cargo run` and doctests sign their
binaries automatically. `.cargo/config.toml` sets
[`hvf/scripts/codesign-run.sh`](../scripts/codesign-run.sh) as the runner for
`aarch64-apple-darwin`.

The host probes (`Hvf::max_vcpu_count`, `max_ipa_bits`, `default_ipa_bits`,
`el2_supported`, `default_ipa_granule`, `sme_max_svl_bytes`, `gic_params`,
`gic_intid`) and `VcpuConfig` need neither the entitlement nor an existing VM.
They return the same values in unsigned binaries.

`HvError::is_unavailable()` is true for `HV_DENIED`, `HV_UNSUPPORTED` and
`HV_NO_DEVICE`. Use it to tell "hypervisor not usable here" (unsigned binary,
Intel Mac, VM without nested virtualization) apart from real failures.
`Hvf::is_supported()` reads `kern.hv_support` and does not look at the
entitlement.

## One VM per process

The framework allows one VM per process. `Hvf::create_vm` returns
`Error::VmExists` while another VM is alive.

A `Vm` is reference counted. Every `Vm` clone, every `Vcpu` and every `Gic`
keeps it alive, and the VM is destroyed when the last handle is dropped. A
`Vcpu` leaked with `mem::forget` therefore keeps the VM alive for the rest of
the process. That is a leak, not undefined behaviour.

`hv_vm_destroy` fails while a vCPU still exists. This can happen when vCPUs
were created through `hvf-bindings` directly, or when a thread that owned a
leaked vCPU has not yet been cleaned up by the framework. The crate then
records the VM as a *zombie*. The next `create_vm` retries the destroy, fails
with `Error::Hv { call: "hv_vm_destroy", .. }` (`HV_BUSY`) while the vCPU
still exists, and succeeds once the framework has released it. On macOS 27
that takes a few milliseconds after the owning thread exits.

## Threading

| Type | `Send` | `Sync` | Notes |
|---|---|---|---|
| `Vm`, `Gic` | yes | yes | Mapping, GIC SPIs and MSIs work from any thread. |
| `Vcpu` | no | no | Every vCPU call must be made on the thread that created the vCPU. Dropping it destroys the vCPU on that thread. |
| `VcpuKicker` | yes | yes | `kick()` forces the vCPU out of `run` with `VcpuExit::Canceled`. |
| `VcpuConfig` | no | no | |
| Exit, GIC layout and parameter types | yes | yes | Plain data. |

- Create each `Vcpu` on its own thread: `vm.create_vcpu(None)` in the thread
  that will run it. A thread can own only one vCPU. A second
  `create_vcpu` on the same thread fails with `Error::Hv { call:
  "hv_vcpu_create", .. }` (`HV_BUSY`).
- A kick is remembered when the vCPU is not running: the next `run` returns
  `Canceled` immediately, like KVM's `immediate_exit`.
- A kicker never affects another vCPU, even one that reuses the id of its
  destroyed vCPU. After the destroy, `kick()` returns `Ok(false)`.

## Guest memory

`Vm::map` takes a raw host pointer, like `KVM_SET_USER_MEMORY_REGION`, and is
`unsafe`. The host range must stay mapped until it is unmapped from the guest,
or until every `Vm`, `Vcpu` and `Gic` handle is dropped. Because the guest can
read and write it at any time, host code must access it only through volatile
or atomic operations (for example vm-memory's `VolatileSlice`), never through
`&` or `&mut` references.

- **Granule.** Every host address, guest address (IPA) and size passed to
  `map`, `unmap` and `protect` must be a multiple of `Vm::ipa_granule()`.
  Otherwise the call fails with `Error::Misaligned` before reaching the
  framework. The default granule is **16 KiB**, the Apple silicon page size.
  From macOS 26.0, `VmConfig { ipa_granule: Some(IpaGranule::K4), .. }`
  selects **4 KiB**, which permits 4 KiB-aligned guest layouts. Ranges must
  also fit in `Vm::ipa_bits()`; otherwise the call fails with
  `Error::OutOfRange`.
- **Unmap before remap.** Mappings are not replaced. Mapping over an already
  mapped range fails with `Error::Hv { call: "hv_vm_map", .. }`. Call
  `Vm::unmap` first. `unmap` is safe and synchronous: a vCPU that touches
  the range afterwards takes a stage-2 fault, and does not see the old pages.
- **Permissions** come only from `MemoryFlags`, not from the host protection
  of the mapping. `Vm::protect` changes them for a mapped range.
- **Dirty tracking with `protect`.** After `vm.protect(range, READ)`, a guest
  store to a page the guest had already touched exits as a permission fault
  (`DataAbort::is_permission_fault()`). A store to a page the guest had not
  touched yet exits as a translation fault. A dirty tracker must treat any
  write abort on mapped memory as a dirtying write.
- **File-backed memory.** Guest stores to a `MAP_SHARED` file mapping reach
  the file, and host writes to the file are visible to the guest. A
  `MAP_SHARED` mapping of a file opened read-only cannot be mapped at all:
  `hv_vm_map` fails with `HV_ERROR`.
- **vm-memory.** With the `vm-memory` feature, `Vm::map_guest_memory` maps
  every region of a `vm_memory::GuestMemoryBackend`, for example a
  `GuestMemoryMmap`. It validates every region first. If one mapping fails,
  it unmaps the regions it already mapped.

The `hv_vm_allocate` allocator is not wrapped. Use `mmap` or vm-memory.

## Exits and the program counter

`Vcpu::run` returns `VcpuExit::Exception` for guest exceptions that need the
VMM. `Exception::decode()` turns the syndrome into an `ExceptionClass`. Apart
from `HVC`, the PC still points at the trapping instruction when `run`
returns. The VMM must call `Vcpu::advance_pc()` after emulating the
instruction. `ExceptionClass::pc_advanced_by_hw()` reports which case applies.

| Exit | PC on exit | VMM action |
|---|---|---|
| `Hvc { imm }` | already past the `hvc` | emulate (PSCI, hypercall); **do not** advance |
| `Smc { imm }` | at the `smc` | emulate, then `advance_pc` |
| `SysReg(access)` | at the `mrs`/`msr` | emulate, then `advance_pc` |
| `DataAbort(abort)` (MMIO) | at the load or store | `mmio_write_value` + `advance_pc`, or `complete_mmio_read` |
| `Wfx { wfe }` | at the `wfi`/`wfe` | wait (see below), then `advance_pc` |
| `InstructionAbort`, `Breakpoint`, `SoftwareStep`, `Watchpoint`, `Brk` | at the instruction | VMM-specific |

The crate does not set `MPIDR_EL1` or `CPSR`. Both start at 0. Set `CPSR`
explicitly, typically to `0x3c5` (EL1h with DAIF masked), and set `MPIDR_EL1`
when the guest or the GIC needs distinct affinities.

### MMIO

A data abort on an unmapped IPA is an MMIO access. When the instruction
syndrome is valid (`abort.syndrome` is `Some`):

```rust
let access = abort.syndrome.expect("ISV=0: decode the instruction yourself");
if abort.write {
    let value = vcpu.mmio_write_value(&access)?; // Rt masked to access.len bytes
    bus.write(abort.ipa, &value.to_le_bytes()[..usize::from(access.len)]);
    vcpu.advance_pc()?;
} else {
    let raw = bus.read(abort.ipa, usize::from(access.len));
    vcpu.complete_mmio_read(&access, raw)?; // sign/zero extends, writes Rt, advances PC
}
```

`hvf::mmio_load_value` and `hvf::mmio_store_value` provide the same
arithmetic without a vCPU.

### WFI and WFE

Without the in-kernel GIC, the framework exits on `wfi` and does not wait
itself. The usual pattern is to read `CNTV_CTL_EL0` and `CNTV_CVAL_EL0`,
sleep until the timer deadline or a kick, then `advance_pc` and run again.

With the in-kernel GIC (`Vm::create_gic`), `wfi` does not exit. `run`
blocks in the framework until an interrupt is pending for the vCPU (a timer,
an SPI from `Gic::set_spi` on any thread, or an SGI from another vCPU) and
returns `Canceled` on a kick, with the PC already past the `wfi`.

On macOS 27, `wfe` does not exit in either case.

## Interrupts without the in-kernel GIC

A VMM that emulates its own interrupt controller drives the vCPU's IRQ and
FIQ lines with `Vcpu::set_pending_interrupt`. The framework **clears the
pending state after every `run`**. The VMM must therefore latch each
level-triggered line itself and re-arm it before every run:

```rust
loop {
    if irq_line_asserted() {
        vcpu.set_pending_interrupt(InterruptType::Irq, true)?;
    }
    let exit = vcpu.run()?;
    // ... handle the exit, update the line from the device models ...
}
```

During a run the line stays asserted, so a guest that unmasks interrupts in
the middle of that run still takes the IRQ.

## Virtual timer

- **Firing.** Without an in-kernel GIC, a firing EL1 virtual timer exits with
  `VcpuExit::VtimerActivated`, and the framework masks the timer
  (`vtimer_mask()` reads `true`) so it does not exit again.
- **Delivery.** The VMM raises the timer interrupt (PPI 27) in its interrupt
  controller model.
- **Unmasking.** The VMM calls `set_vtimer_mask(false)` once the guest has
  handled the interrupt. One way to detect that is to see `CNTV_CTL_EL0` no
  longer reporting ENABLE, ISTATUS and not IMASK all together.
- **Guest counter.** The guest counter is
  `mach_absolute_time() - vtimer_offset`. When pausing, resuming or restoring
  a snapshot, adjust `set_vtimer_offset` so guest time does not jump.

With the in-kernel GIC, the framework delivers the timer interrupt itself.

## In-kernel GICv3 (macOS 15.0)

`Vm::create_gic` creates a GICv3 distributor, one redistributor per vCPU,
and an optional MSI frame. `Hvf::gic_params()` reports the required sizes and
alignments and the SPI range. The layout is checked before the framework is
called (alignment, IPA range, overlap, MSI range inside the SPI range).

- **Create order.** Create the GIC before the first vCPU. Afterwards,
  `create_gic` returns `Error::GicAfterVcpu`.
- **Redistributors.** Set each vCPU's `MPIDR_EL1` before using its
  redistributor. Redistributors are assigned by affinity:
  `Vcpu::gic_redistributor_base()` is `redistributor_base + Aff0 * size`.
- **Interrupt lines.** `Gic::set_spi` and `Gic::send_msi` work from any
  thread. For `send_msi`, the doorbell address is normally `msi.base + 0x40`.
  SPIs outside the SPI range, or inside the MSI range, fail with
  `HV_BAD_ARGUMENT`.
- **Vcpu lines.** With a GIC, `Vcpu::set_pending_interrupt` and
  `pending_interrupt` fail with `HV_UNSUPPORTED`.
- **ICH and ICV.** The ICH and ICV accessors work only when EL2 is enabled.
- **Save and restore.** Take `Gic::save_state` while no vCPU runs. Call
  `Gic::restore_state` after creating every vCPU and before the first run,
  then restore the per-vCPU CPU interface registers (`gic_icc_reg`, and
  `gic_ich_reg` with EL2). The crate does not track running vCPUs, so these
  rules are not enforced. On macOS 27, a restore after a vCPU has run also
  succeeds, but Apple does not document this.

## Testing

Tests that create a VM need a host where `kern.hv_support` is 1, and a signed
binary. In this repository the runner takes care of signing. On hosts without
HVF, such as Linux, GitHub-hosted macOS runners or unsigned binaries, these
tests print `SKIP` and pass. Set `HVF_REQUIRE=1` to make a skip fail:

```sh
HVF_REQUIRE=1 cargo test -p hvf --all-features
HVF_REQUIRE=1 cargo test -p hvf --all-features --release
```

To check the skip path on a Mac, rebuild with the runner replaced by `env`.
Rebuild first (`cargo clean -p hvf`), because `codesign` signs binaries in
place:

```sh
cargo clean -p hvf
CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER=env cargo test -p hvf
```

CI coverage:

- **Linux Buildkite** builds, tests, lints and measures coverage for the
  portable code.
- **GitHub Actions** (`.github/workflows/macos.yml`) builds, lints and tests
  the workspace on macOS. Its HVF tests skip.
- **Self-hosted Apple silicon agents** (`.buildkite/pipeline.macos.yml`) run
  the HVF tests for real. Until those agents are available, maintainers run
  the commands above on an Apple silicon Mac before every release.

## License

This project is licensed under either of

- [Apache License](http://www.apache.org/licenses/LICENSE-2.0), Version 2.0
- [BSD-3-Clause License](https://opensource.org/licenses/BSD-3-Clause)
