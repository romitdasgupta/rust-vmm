# Changelog

## Upcoming Release

### Added

- Initial release: safe wrappers over the arm64 Hypervisor.framework API.
  - `Hvf`: host probes and creation of the per-process `Vm`.
  - `Vm`: map, unmap and protect guest memory with granule and IPA range
    checks, and create vCPUs.
  - `Vcpu` (bound to its thread) and `VcpuKicker`: run, registers, SIMD/FP
    registers, IRQ/FIQ lines, virtual timer, debug traps, SError and TLB
    invalidation, and MMIO completion helpers.
  - `VcpuExit` and `ExceptionClass`: decoding of exits and exception
    syndromes, including MMIO data aborts.
  - `Gic`: the in-kernel GICv3 (macOS 15.0), with distributor,
    redistributor, CPU interface and MSI registers, plus state save and
    restore.
  - `VmConfig`: IPA size, EL2 and 4 KiB stage-2 granule selection.
  - Feature `vm-memory`: `Vm::map_guest_memory` for any
    `vm_memory::GuestMemoryBackend`.
- Functions newer than macOS 11.0 are resolved at run time. Where the
  running release lacks one, the method returns `Error::Unavailable`.
- Errors, register identifiers, exit decoding and MMIO arithmetic build on
  every target.
- Example `boot_linux`: boots an arm64 Linux `Image` to userspace on the
  in-kernel GICv3. It uses a PL011 console, PSCI SMP and power off, and
  `vm-memory`, `linux-loader`, `vm-device` and `vm-fdt`. An ignored test
  boots a kernel named by `HVF_LINUX_KERNEL` and `HVF_LINUX_INITRD`.
