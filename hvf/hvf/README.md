# hvf

Safe wrappers over the macOS
[Hypervisor.framework](https://developer.apple.com/documentation/hypervisor)
on Apple silicon, in the spirit of
[kvm-ioctls](../../kvm/kvm-ioctls).

- `Hvf` probes the host and creates the (single, per-process) `Vm`.
- `Vm` maps guest memory and creates `Vcpu`s and the optional in-kernel GICv3.
- `Vcpu` runs guest code and accesses registers. It is bound to the thread
  that created it; a `VcpuKicker` interrupts it from any thread.
- `VcpuExit` and `ExceptionClass` decode exits, including MMIO data aborts.

## Supported platforms

`aarch64-apple-darwin`, macOS 11.0 or newer. Features of newer releases are
resolved at run time and report `Error::Unavailable` when missing. The
portable parts (errors, register identifiers, exit decoding) build on every
target.

## Signing

Binaries using Hypervisor.framework must be signed with the
`com.apple.security.hypervisor` entitlement. In this repository `cargo test`
and `cargo run` sign automatically through the runner configured in
`.cargo/config.toml` (`hvf/scripts/codesign-run.sh`). Elsewhere, sign with:

```sh
codesign --sign - --force --entitlements hvf/scripts/hvf.entitlements <binary>
```

## License

This project is licensed under either of

- [Apache License](http://www.apache.org/licenses/LICENSE-2.0), Version 2.0
- [BSD-3-Clause License](https://opensource.org/licenses/BSD-3-Clause)
