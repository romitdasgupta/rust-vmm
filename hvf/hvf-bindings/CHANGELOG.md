# Changelog

## Upcoming Release

### Added

- Initial release: hand-written bindings to the arm64 Hypervisor.framework
  API of the macOS 27.0 SDK, with SDK names.
  - Types, return codes and exit structures.
  - All `OS_ENUM` values as integer constants: registers, system registers,
    feature registers, GIC interrupt ids and register identifiers, SME
    registers and TLBI operations.
  - Opaque `OS_OBJECT` types and `os_release`.
  - All 92 functions. Functions available since macOS 11.0 are linked
    directly. Newer ones are resolved at run time through `DynApi`, and
    hard-linked declarations are also provided in `direct`. Each item's
    availability is recorded in `since`.
- `hv_vcpu_set_simd_fp_reg_by_ref`, a trampoline for
  `hv_vcpu_set_simd_fp_reg`, which takes a vector argument by value.
- Types and constants are available on every target. Functions are
  available on `aarch64-apple-darwin`.
- `tests/sdk_check.rs`, which verifies every binding against the installed
  SDK headers with `xcrun clang`.
