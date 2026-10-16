# hvf-bindings

Raw Rust FFI bindings to the Apple silicon (arm64) interface of the macOS
[Hypervisor.framework](https://developer.apple.com/documentation/hypervisor).

Most users want the safe wrappers in the [`hvf`](../hvf) crate instead.

## Contents

The bindings are written by hand from the SDK headers, with no bindgen and no
build script, and the crate has no dependencies. Every item keeps its SDK
name:

- **Types and return codes.** `hv_return_t` and the `HV_*` codes, `hv_vcpu_t`,
  `hv_ipa_t`, the exit structures, and SIMD/FP and SME vector types.
- **`OS_ENUM` types** are integer type aliases with one `pub const` per value,
  for example `hv_reg_t` / `HV_REG_X0` and `hv_sys_reg_t` /
  `HV_SYS_REG_MPIDR_EL1`. Apple adds values to these types in new releases,
  so they are not Rust enums.
- **GIC values.** GIC interrupt ids, and the distributor, redistributor, CPU
  interface and MSI register identifiers.
- **`OS_OBJECT` types** (VM, vCPU and GIC configurations, GIC state) are
  pointers to opaque structs. Release them with `os_release`.
- **Functions.** Every arm64 function of the macOS 27.0 SDK: 92
  declarations, plus a trampoline for the SIMD register setter (see below).

The x86_64 Hypervisor.framework API is not bound.

## Linking and availability

- **macOS 11.0 functions** are linked directly through
  `#[link(name = "Hypervisor", kind = "framework")]`.
- **Newer functions** (macOS 12.1 to 27.0) are resolved at run time:
  - `DynApi::load()` looks every one of them up with `dlsym`. A field is
    `None` when the running macOS release lacks that function, so one binary
    loads and runs on every release.
  - `since::NAME` gives the release that introduced each function, and each
    constant's documentation names its release.
  - Hard-linked declarations of the newer functions are in the `direct`
    module, for programs that require a newer macOS anyway.

`hv_vcpu_set_simd_fp_reg` takes a 128-bit vector by value, which stable Rust
cannot declare. Call `hv_vcpu_set_simd_fp_reg_by_ref` instead. It is a
two-instruction trampoline that loads the value into `q0` and tail-calls the
framework.

## Supported platforms

- **`aarch64-apple-darwin`**: everything.
- **Other targets**: the types, constants and register encodings, so that
  portable code (and Linux CI) can use them. The function declarations,
  `DynApi::load`, the SIMD trampoline and the `os` helpers are only compiled
  for `aarch64-apple-darwin`.

The minimum supported Rust version is 1.88, the first stable release with
naked functions.

## Verification against the SDK

`tests/sdk_check.rs` generates C from the binding tables and compiles it
against the installed SDK with `xcrun clang`. It checks:

- every constant value;
- scalar sizes and signedness;
- struct layouts;
- every function prototype;
- every item's availability;
- that nothing in the headers is missing.

```sh
HVF_SDK_CHECK=1 cargo test -p hvf-bindings --test sdk_check
```

Items newer than the installed SDK are skipped. `tests/layout.rs` repeats the
layout checks without the SDK, so they also run on Linux. See
[CONTRIBUTING.md](CONTRIBUTING.md) for how to add bindings and how to update
them for a new SDK.

## Signing

Binaries using Hypervisor.framework must be signed with the
`com.apple.security.hypervisor` entitlement. Otherwise `hv_vm_create`
returns `HV_DENIED`. In this repository, `cargo test` and `cargo run` sign
automatically through the runner configured in `.cargo/config.toml`
([`hvf/scripts/codesign-run.sh`](../scripts/codesign-run.sh)). Elsewhere,
sign with:

```sh
codesign --sign - --force --entitlements hvf/scripts/hvf.entitlements <binary>
```

The unit tests that create a VM print `SKIP` when the hypervisor cannot be
used. Set `HVF_REQUIRE=1` to make them fail instead.

## License

This project is licensed under either of

- [Apache License](http://www.apache.org/licenses/LICENSE-2.0), Version 2.0
- [BSD-3-Clause License](https://opensource.org/licenses/BSD-3-Clause)
