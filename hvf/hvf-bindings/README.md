# hvf-bindings

Raw Rust FFI bindings to the Apple silicon (arm64) interface of the macOS
[Hypervisor.framework](https://developer.apple.com/documentation/hypervisor).

The bindings are written by hand from the SDK headers and keep the SDK names.
Functions available since macOS 11.0 are linked directly; newer functions are
resolved at run time through `DynApi`, so one binary runs on every macOS
release that supports Apple silicon.

Most users want the safe wrappers in the [`hvf`](../hvf) crate instead.

## Supported platforms

`aarch64-apple-darwin`. The types and constants also compile on other targets
so they can be shared with portable code; the function declarations are only
available on Apple silicon macOS.

## Signing

Binaries using Hypervisor.framework must be signed with the
`com.apple.security.hypervisor` entitlement, otherwise `hv_vm_create` returns
`HV_DENIED`. In this repository `cargo test` and `cargo run` do this
automatically through the runner configured in `.cargo/config.toml`
(`hvf/scripts/codesign-run.sh`).

## License

This project is licensed under either of

- [Apache License](http://www.apache.org/licenses/LICENSE-2.0), Version 2.0
- [BSD-3-Clause License](https://opensource.org/licenses/BSD-3-Clause)
