# Contributing to hvf-bindings

The bindings are written by hand from the Hypervisor.framework headers of the
macOS SDK. There is no generator; two declarative macros keep the
declarations short and record every item in tables that a test checks against
the SDK.

## Layout

| File | Contents | Compiled on |
|---|---|---|
| `src/types.rs` | Base types, return codes, exit structures, opaque objects | every target |
| `src/regs.rs` | `hv_reg_t`, `hv_simd_fp_reg_t`, `hv_sys_reg_t`, feature registers | every target |
| `src/gic.rs` | GIC interrupt IDs and register identifiers | every target |
| `src/sme.rs` | SME `Z` and `P` register identifiers | every target |
| `src/functions.rs` | Every arm64 function | macOS on aarch64 |
| `src/simd.rs` | Trampoline for `hv_vcpu_set_simd_fp_reg` | macOS on aarch64 |
| `src/os.rs` | `os_release`, `dlsym` | macOS on aarch64 |

Names are identical to the SDK. The x86_64 Hypervisor.framework API, and the
header items that no public function uses (`hv_data_abort_notification_t`,
`hv_vm_mem_access_msg_t`), are not bound.

## Adding constants

`OS_ENUM(name, type, ...)` becomes a group in the module's `hv_consts!`
invocation:

```rust
hv_consts! {
    /// Documentation of the type.
    pub type hv_tlbi_op_t = u16 {
        /// Documentation of the value.
        @since(27, 0) HV_TLBI_OP_RVAE1IS = 0x11,
    }
}
```

- Write every value explicitly, even where C relies on implicit numbering.
- Add `@since(MAJOR, MINOR)` when the header gives the enumerator, or its enum
  type, an `API_AVAILABLE(macos(..))` newer than 11.0. Enumerators without
  one take the availability of the functions that use them.
- Use `consts NAME_t { .. }` to add values to a type declared in another
  module. An example is the feature registers in `regs.rs`, whose type is in
  `types.rs`.
- Return codes are written as `0xfae9_4001_u32 as i32`, because
  `hv_return_t` is signed.

## Adding functions

Add one entry to `hv_fns!` in `src/functions.rs`. Copy the availability from
`API_AVAILABLE`, and the argument names from the header:

```rust
hv_fns! {
    /// Documentation.
    (27, 0) fn hv_vcpu_get_serror(vcpu: hv_vcpu_t, pending: *mut bool) -> hv_return_t;
}
```

- Use `u8`/`u16`/`u32`/`u64` for `uintN_t`, `i32` for `int32_t`, `usize`
  for `size_t`, `bool` for `bool`, and `c_void` for `void`. A C array
  parameter such as `uint64_t values[8]` becomes a pointer. A C `const T *`
  becomes `*const T`.
- Functions available since macOS 11.0 are linked directly. Newer ones get a
  `DynApi` field, resolved with `dlsym` at run time, and a hard-linked
  declaration in `direct`. The `hvf` crate only uses `DynApi`.
- Do not bind a function that passes a vector type by value. Stable Rust
  cannot express that ABI, so it needs a trampoline like the one in
  `src/simd.rs`.

## Checking against the SDK

`tests/sdk_check.rs` compiles C code generated from the binding tables with
`xcrun clang` and compares it with the installed SDK:

- the value of every constant;
- the size and signedness of every scalar type;
- the layout and field types of every struct;
- the exact C prototype of every function (a function pointer
  initialization that `-Wincompatible-function-pointer-types` turns into an
  error on any mismatch);
- the type name and `API_AVAILABLE` of every constant and function;
- that every constant and function in the headers is bound, unless it is
  listed in `tests/sdk_skip.txt`.

Run it whenever you change a binding or install a new SDK:

```sh
HVF_SDK_CHECK=1 cargo test -p hvf-bindings --test sdk_check
```

`HVF_SDK_CHECK=1` turns a missing `xcrun` into a failure instead of a skip.
Items newer than the installed SDK are not checked, so run the check with the
newest SDK before submitting.

To confirm the check notices a change, alter one value or argument type
locally, check that the test fails and names that item, then revert:

```sh
sed -i '' 's/HV_SYS_REG_MIDR_EL1 = 0xc000/HV_SYS_REG_MIDR_EL1 = 0xc001/' src/regs.rs
HVF_SDK_CHECK=1 cargo test -p hvf-bindings --test sdk_check  # fails: "HV_SYS_REG_MIDR_EL1"
git checkout src/regs.rs
```

`tests/layout.rs` repeats the size, alignment and offset checks without the
SDK, so they also run on Linux CI.

## Updating for a new SDK

1. Diff the old and new SDK `Hypervisor.framework/Headers` directories.
2. Add the new items with their `@since`/availability, as described above.
3. Run the SDK check. Anything still missing is reported as
   `in the SDK but not bound`. Either bind it, or list it in
   `tests/sdk_skip.txt` with a comment explaining why.
4. Update the constant and function counts in `tests/layout.rs`, and add a
   `CHANGELOG.md` entry.

## Tests that use the hypervisor

The unit tests in `src/functions.rs` and `src/simd.rs` create a real VM. The
binary must be signed with the `com.apple.security.hypervisor` entitlement. In
this repository the cargo runner in `.cargo/config.toml` does this
automatically. Without the entitlement these tests print `SKIP` and pass,
unless `HVF_REQUIRE=1` is set:

```sh
HVF_REQUIRE=1 cargo test -p hvf-bindings
HVF_REQUIRE=1 cargo test -p hvf-bindings --release
```

## Style

Every `unsafe` block needs a `// SAFETY:` comment
(`clippy::undocumented_unsafe_blocks` is denied). Every public item needs
documentation. `hv_consts!` adds a line to each constant, so a short
description of the value is enough.

Commits need a `Signed-off-by` line (`git commit -s`), like the rest of
rust-vmm.
