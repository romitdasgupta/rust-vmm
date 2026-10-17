# Booting Linux on Hypervisor.framework

This guide goes from `git clone` to a busybox shell in an arm64 Linux guest
running on `boot_linux`, the example VMM of the [hvf](../../README.md) crate.
Two scripts in [`scripts/`](scripts) download a pinned kernel and build an
initramfs. Neither needs root, Python or Homebrew.

The short version, from the root of the repository:

```sh
hvf/hvf/examples/boot_linux/scripts/fetch-kernel.sh
hvf/hvf/examples/boot_linux/scripts/mkinitramfs.sh
cargo run --release -p hvf --features vm-memory --example boot_linux -- \
    --kernel target/boot-linux/Image --initrd target/boot-linux/initramfs.cpio.gz
```

Type `exit` in the guest shell to power it off, or press `Ctrl-A x` to stop
the VMM.

## Contents

- [Prerequisites](#prerequisites)
- [How signing works](#how-signing-works)
- [Step 1: clone and build](#step-1-clone-and-build)
- [Step 2: download the kernel](#step-2-download-the-kernel)
- [Step 3: build the initramfs](#step-3-build-the-initramfs)
- [Step 4: boot to a shell](#step-4-boot-to-a-shell)
- [Step 5: boot without a shell, on more vCPUs](#step-5-boot-without-a-shell-on-more-vcpus)
- [Alternative: the minimal Rust /init](#alternative-the-minimal-rust-init)
- [Running the boot test](#running-the-boot-test)
- [Reference](#reference)
- [Using your own kernel](#using-your-own-kernel)
- [How the initramfs is built](#how-the-initramfs-is-built)
- [Troubleshooting](#troubleshooting)

## Prerequisites

| What | Why | How to check or get it |
|---|---|---|
| A Mac with Apple silicon (M1 or newer) | Hypervisor.framework runs arm64 guests only on Apple silicon | `uname -m` prints `arm64` |
| macOS 15.0 or newer | The VMM uses the in-kernel GICv3, which macOS 15.0 added | `sw_vers -productVersion` |
| Rust 1.88 or newer, installed with [rustup](https://rustup.rs) | Builds the VMM | `rustc --version` |
| Xcode Command Line Tools | The C compiler and linker Rust uses on macOS | `xcode-select --install` |
| `curl`, `bsdtar` (`tar`), `gzip`, `shasum`, `codesign` | Downloads, unpacking, the cpio archive, checksums and signing | Ship with macOS |
| The `aarch64-unknown-none` Rust target | Only for the [Rust /init](#alternative-the-minimal-rust-init) | `rustup target add aarch64-unknown-none` |

A Mac running as a virtual machine usually has no nested virtualization:
check that `sysctl kern.hv_support` prints `kern.hv_support: 1`.

## How signing works

Hypervisor.framework creates a VM only for a binary signed with the
`com.apple.security.hypervisor` entitlement. An unsigned binary fails with
`HV_DENIED`. An ad-hoc signature (no Apple developer account) is enough.

You do not sign anything by hand. The repository's
[`.cargo/config.toml`](../../../../.cargo/config.toml) sets
[`hvf/scripts/codesign-run.sh`](../../../scripts/codesign-run.sh) as the
runner for `aarch64-apple-darwin`. `cargo run` and `cargo test` call the
runner with the freshly built binary. It signs the binary in place with
[`hvf/scripts/hvf.entitlements`](../../../scripts/hvf.entitlements) and then
runs it. That is visible in `cargo run`'s output:

```text
     Running `.../hvf/scripts/codesign-run.sh target/release/examples/boot_linux --kernel ...`
```

Cargo looks for `.cargo/config.toml` from the **current directory** upwards,
not from the manifest. Run `cargo` from inside the repository: `cargo run
--manifest-path .../rust-vmm/Cargo.toml` from another directory runs the
binary without the runner, and a freshly built binary then fails with
`HV_DENIED`. `cargo build` never signs, and every rebuild drops the
signature. Run a binary directly only after `cargo run` has signed it, or
sign it yourself:

```sh
codesign --sign - --force --entitlements hvf/scripts/hvf.entitlements \
    target/release/examples/boot_linux
```

`codesign -d --entitlements - target/release/examples/boot_linux` shows
whether a binary has the entitlement.

## Step 1: clone and build

Until the `hvf` crates are merged upstream, `rust-vmm/rust-vmm` has no
`hvf/` directory. Clone the repository and branch that contain it instead,
and check that `hvf/hvf/examples/boot_linux` exists:

```sh
git clone -b <branch> <repository URL> rust-vmm
cd rust-vmm
ls hvf/hvf/examples/boot_linux
cargo build --release -p hvf --features vm-memory --example boot_linux
```

Once merged, that is `git clone https://github.com/rust-vmm/rust-vmm.git`.
The `rust-vmm-ci` submodule is not needed.

Expected output ends with:

```text
    Finished `release` profile [optimized] target(s) in ...
```

`--features vm-memory` is required: the example maps guest RAM with
`Vm::map_guest_memory`. Without it, Cargo reports that the example requires
the feature.

All commands below run from the root of the repository.

## Step 2: download the kernel

```sh
hvf/hvf/examples/boot_linux/scripts/fetch-kernel.sh
```

[`fetch-kernel.sh`](scripts/fetch-kernel.sh) downloads Debian 13's
`linux-image-6.12.111+deb13-cloud-arm64-unsigned` package (28 MB) and
extracts its `/boot/vmlinuz`, which is an uncompressed arm64 `Image`. The
script:

- downloads into a new temporary directory, from `deb.debian.org`, or from
  `snapshot.debian.org` once Debian has removed this version from its pool;
- checks the SHA-256 of the `.deb` and of the extracted `Image` against the
  values pinned at the top of the script, and the `Image` magic number;
- writes `target/boot-linux/Image` and the kernel configuration
  `target/boot-linux/Image.config`. `target/` is ignored by git, and
  `cargo clean` removes it.

Pass a directory to write somewhere else: `fetch-kernel.sh /path/to/dir`.
A second run does nothing if the `Image` is already there and matches.

Expected output (the `###` progress bar appears only when the output is a
terminal):

```text
Downloading https://deb.debian.org/debian/pool/main/l/linux/linux-image-6.12.111+deb13-cloud-arm64-unsigned_6.12.111-1_arm64.deb
######################################################################## 100.0%
Verified linux-image-6.12.111+deb13-cloud-arm64-unsigned_6.12.111-1_arm64.deb (sha256 88926feba4ab6db49c6ce072f099640cd14512af8b47a1fa2394b8d596f688da)
Wrote .../target/boot-linux/Image (Linux 6.12.111+deb13-cloud-arm64, sha256 7df55571ea48d32aee64c37af99c8bbb8b57b49adb0806707218e864c3541703)
```

```console
$ file target/boot-linux/Image
target/boot-linux/Image: Linux kernel ARM64 boot executable Image, little-endian, 4K pages
```

## Step 3: build the initramfs

```sh
hvf/hvf/examples/boot_linux/scripts/mkinitramfs.sh
```

[`mkinitramfs.sh`](scripts/mkinitramfs.sh) downloads Alpine Linux 3.24's
`busybox-static` package for aarch64 (650 KB), checks the SHA-256 of the
package and of the busybox binary, and packs busybox and a shell `/init`
into `target/boot-linux/initramfs.cpio.gz`. `-o FILE` writes elsewhere.

Expected output:

```text
Downloading https://dl-cdn.alpinelinux.org/alpine/v3.24/main/aarch64/busybox-static-1.37.0-r31.apk
Verified busybox-static-1.37.0-r31.apk (sha256 965777e06b94bf11981d5f4ecdfcd577879f4b0dda544294d1dd3f72b217bc75)
Wrote .../target/boot-linux/initramfs.cpio.gz (busybox /init, ... bytes)
```

The archive is about 660 KB. Its exact size changes slightly from run to
run, because the archive records the inode numbers of the staged files.

```console
$ tar -tvf target/boot-linux/initramfs.cpio.gz
drwxr-xr-x  0 0      0           0 Dec 31  1969 ./dev
crw-------  0 0      0         5,1 Dec 31  1969 ./dev/console
crw-rw-rw-  0 0      0         1,3 Dec 31  1969 ./dev/null
dr-xr-xr-x  0 0      0           0 Dec 31  1969 ./proc
dr-xr-xr-x  0 0      0           0 Dec 31  1969 ./sys
drwxrwxrwt  0 0      0           0 Dec 31  1969 ./tmp
drwx------  0 0      0           0 Dec 31  1969 ./root
drwxr-xr-x  0 0      0           0 Dec 31  1969 ./bin
-rwxr-xr-x  1 0      0     1116408 Dec 31  1969 ./bin/busybox
lrwxrwxrwx  0 0      0           7 Dec 31  1969 ./bin/sh -> busybox
-rwxr-xr-x  1 0      0         939 Dec 31  1969 ./init
```

The busybox binary is an aarch64 Linux executable. It is never run on the
Mac, only in the guest.

The `/init` script mounts `/proc`, `/sys` and devtmpfs on `/dev`, installs
the busybox applets as symlinks, prints `HVF-BOOT-OK` and `uname -a`, and
then:

- with `hvf.autopoweroff` on the kernel command line, powers off at once;
- otherwise starts an interactive shell on the console and powers off when
  the shell exits.

## Step 4: boot to a shell

```sh
cargo run --release -p hvf --features vm-memory --example boot_linux -- \
    --kernel target/boot-linux/Image --initrd target/boot-linux/initramfs.cpio.gz
```

The kernel log scrolls by; the shell prompt appears in well under a second
of guest time. Expected output, abridged:

```text
     Running `.../hvf/scripts/codesign-run.sh target/release/examples/boot_linux --kernel target/boot-linux/Image --initrd target/boot-linux/initramfs.cpio.gz`
[    0.000000] Booting Linux on physical CPU 0x0000000000 [0x610f0000]
[    0.000000] Linux version 6.12.111+deb13-cloud-arm64 (debian-kernel@lists.debian.org) ...
[    0.000000] Machine model: rust-vmm hvf boot_linux
[    0.000000] earlycon: pl11 at MMIO32 0x0000000009000000 (options '')
[    0.000000] psci: PSCIv1.0 detected in firmware.
[    0.000000] Kernel command line: console=ttyAMA0 earlycon=pl011,mmio32,0x9000000 panic=-1
[    0.000000] GICv3: CPU0: found redistributor 0 region 0:0x00000000080a0000
[    0.000000] arch_timer: cp15 timer(s) running at 24.00MHz (virt).
[    0.004549] smp: Brought up 1 node, 1 CPU
[    0.019371] 9000000.pl011: ttyAMA0 at MMIO 0x9000000 (irq = 13, base_baud = 0) is a PL011 rev1
[    0.036827] Trying to unpack rootfs image as initramfs...
[    0.085340] Run /init as init process
HVF-BOOT-OK: hello from busybox userspace
Linux hvf 6.12.111+deb13-cloud-arm64 #1 SMP Debian 6.12.111-1 (2026-09-28) aarch64 Linux
Type 'exit' or press Ctrl-D to power off, or Ctrl-A x to stop the VMM.
~ #
```

The terminal is in raw mode while the VMM runs, so every key, `Ctrl-C`
included, goes to the guest. The shell runs on `/dev/ttyAMA0` as its
controlling terminal, so `Ctrl-C` interrupts the foreground command and job
control works:

```text
~ # grep -c ^processor /proc/cpuinfo
1
~ # tty
/dev/ttyAMA0
~ # sleep 30
^C
~ # exit
[    1.154863] reboot: Power down
boot_linux: the guest powered off
```

`exit` (or `Ctrl-D`, or `poweroff -f`) powers the guest off and the VMM
exits with status 0. `Ctrl-A x` stops the VMM at any time with status 130:

```text
~ # boot_linux: stopped by Ctrl-A x
```

To drive the shell from a script (`expect`, `script`, CI), read
[the troubleshooting entry about `^[[6n`](#troubleshooting) first: the
prompt asks the terminal a question that a script should answer.

## Step 5: boot without a shell, on more vCPUs

`hvf.autopoweroff` makes the busybox `/init` power off right after its
greeting. `--cmdline` replaces the whole default command line, so repeat
the console settings:

```sh
cargo run --release -p hvf --features vm-memory --example boot_linux -- \
    --kernel target/boot-linux/Image --initrd target/boot-linux/initramfs.cpio.gz \
    --cpus 4 --mem 1G \
    --cmdline "console=ttyAMA0 earlycon=pl011,mmio32,0x9000000 panic=-1 hvf.autopoweroff"
echo "exit status: $?"
```

Expected output, abridged:

```text
[    0.005204] CPU1: Booted secondary processor 0x0000000001 [0x610f0000]
[    0.005653] smp: Brought up 1 node, 4 CPUs
[    0.007901] SMP: Total of 4 processors activated.
...
HVF-BOOT-OK: hello from busybox userspace
Linux hvf 6.12.111+deb13-cloud-arm64 #1 SMP Debian 6.12.111-1 (2026-09-28) aarch64 Linux
[    0.077417] reboot: Power down
boot_linux: the guest powered off
exit status: 0
```

This mode needs no terminal: it also works with stdin redirected from
`/dev/null` or in a script.

## Alternative: the minimal Rust /init

`--init rust` builds the initramfs from a tiny `no_std` Rust program
instead of busybox. It replaces Step 3 only: you still need the kernel from
[Step 2](#step-2-download-the-kernel), but busybox is not downloaded. The
program in [`init/`](init) uses raw system calls, prints `HVF-BOOT-OK` and
the `uname` fields, and powers off. The initramfs is about 2 KB; its exact
size varies by a few bytes between toolchains and runs.

```sh
rustup target add aarch64-unknown-none
hvf/hvf/examples/boot_linux/scripts/mkinitramfs.sh --init rust \
    -o target/boot-linux/initramfs-rust.cpio.gz
cargo run --release -p hvf --features vm-memory --example boot_linux -- \
    --kernel target/boot-linux/Image --initrd target/boot-linux/initramfs-rust.cpio.gz
```

Expected output, abridged:

```text
Building the Rust /init in .../hvf/hvf/examples/boot_linux/init
    Finished `release` profile [optimized] target(s) in ...
Wrote .../target/boot-linux/initramfs-rust.cpio.gz (rust /init, ... bytes)
...
[    0.085377] Run /init as init process
HVF-BOOT-OK: hello from the Rust /init
uname: Linux (none) 6.12.111+deb13-cloud-arm64 #1 SMP Debian 6.12.111-1 (2026-09-28) aarch64
Powering off.
[    0.086060] reboot: Power down
boot_linux: the guest powered off
```

`init/` is a standalone crate with its own `[workspace]` table, so it is not
part of the rust-vmm workspace and `cargo test --workspace` does not build
it. Its `.cargo/config.toml` selects the `aarch64-unknown-none` target and
the linker flags for a static executable; Cargo reads that file only when it
runs inside `init/`. The script builds it into
`target/boot-linux/init/` and packs the binary from there. To work on it
directly:

```sh
cd hvf/hvf/examples/boot_linux/init
cargo build --release
cargo clippy -- -D warnings -D clippy::undocumented_unsafe_blocks
```

A build run this way goes to `init/target/` (ignored by git), not to
`target/boot-linux/init/`, so `mkinitramfs.sh --init rust` does not pick it
up; the script always builds its own copy.

Use `-o` as above to keep both initramfs files; without it, both variants
write `target/boot-linux/initramfs.cpio.gz`.

## Running the boot test

[`tests/boot_linux.rs`](../../tests/boot_linux.rs) boots the kernel with 1
and then 4 vCPUs and checks that the console shows `HVF-BOOT-OK`, that all
vCPUs came up and that the guest powered off. It is `#[ignore]`d and reads
the kernel and initramfs from environment variables. Cargo runs tests in the
package directory (`hvf/hvf`), so give absolute paths.

With the Rust `/init`, which always powers off:

```sh
HVF_LINUX_KERNEL=$PWD/target/boot-linux/Image \
HVF_LINUX_INITRD=$PWD/target/boot-linux/initramfs-rust.cpio.gz \
    cargo test --release -p hvf --features vm-memory --test boot_linux -- --ignored
```

With busybox, add `hvf.autopoweroff` through `HVF_LINUX_CMDLINE`. Without
it, the shell waits for input that never comes and the test hangs:

```sh
HVF_LINUX_KERNEL=$PWD/target/boot-linux/Image \
HVF_LINUX_INITRD=$PWD/target/boot-linux/initramfs.cpio.gz \
HVF_LINUX_CMDLINE="console=ttyAMA0 panic=-1 hvf.autopoweroff" \
    cargo test --release -p hvf --features vm-memory --test boot_linux -- --ignored
```

Expected output:

```text
test test_boot_linux_to_userspace ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 18 filtered out; finished in 0.25s
```

Add `--nocapture` after `--ignored` to see the console output of both
boots. Without the variables, the test prints `SKIP` and passes.

## Reference

### Flags

```text
boot_linux --kernel <Image> [--initrd <file>] [--cmdline <args>]
           [--mem <size>] [--cpus <n>] [--verbose]
```

| Flag | Default | Meaning |
|---|---|---|
| `--kernel <Image>` | required | Uncompressed arm64 Linux `Image` |
| `--initrd <file>` | none | Initramfs, for example a gzip'd newc cpio archive |
| `--cmdline <args>` | `console=ttyAMA0 earlycon=pl011,mmio32,0x9000000 panic=-1` | Kernel command line; replaces the default entirely |
| `--mem <size>` | `512M` | RAM size, a multiple of 2 MiB, with an optional `K`, `M` or `G` suffix. A bare number is MiB |
| `--cpus <n>` | `1` | Number of vCPUs, from 1 to the host's limit, which the error for a too large value names (64 on the Mac this guide was tested on) |
| `--verbose`, `-v` | off | Log the memory layout, PSCI calls and vCPU power state changes to stderr |
| `--help`, `-h` | | Print the usage |

`--opt=value` works as well as `--opt value`. RAM starts at guest address
`0x8000_0000`; the kernel goes at its start, the initrd after the kernel and
the device tree in the last 2 MiB.

### Kernel command line

| Parameter | Effect |
|---|---|
| `console=ttyAMA0` | The PL011 is the console, and `/dev/console` for init. Without it, nothing appears after early boot |
| `earlycon=pl011,mmio32,0x9000000` | Kernel messages from the very start, before the PL011 driver loads |
| `panic=-1` | Reboot at once on a panic. The VMM exits with status 3 instead of hanging |
| `hvf.autopoweroff` | The busybox `/init` powers off after its greeting instead of starting a shell |
| `rdinit=/bin/sh` | Skip `/init` and run the busybox shell as PID 1. Nothing is mounted and only shell builtins work until you run `/bin/busybox --install -s`; leave with `busybox poweroff -f` |
| `quiet` or `loglevel=N` | Fewer kernel messages |

### Keys

| Keys | Effect |
|---|---|
| `Ctrl-A x` | Stop the VMM (exit status 130) |
| `Ctrl-A Ctrl-A` | Send one `Ctrl-A` to the guest |
| anything else | Goes to the guest unchanged, `Ctrl-C` and `Ctrl-Z` included |

### Exit status

| Status | Meaning | Message on stderr |
|---|---|---|
| 0 | The guest powered off (PSCI `SYSTEM_OFF`) | `boot_linux: the guest powered off` |
| 1 | An error, for example a missing file or `HV_DENIED` | `boot_linux: <error>` |
| 2 | Bad command line arguments | `boot_linux: <error>` and the usage |
| 3 | The guest reset (PSCI `SYSTEM_RESET`), for example after a panic with `panic=-1` | `boot_linux: the guest requested a reset` |
| 130 | `Ctrl-A x` | `boot_linux: stopped by Ctrl-A x` |

### Verbose output

`--verbose` prefixes the VMM's own messages with `[boot_linux]` on stderr.
The complete output of a boot with the Rust `/init` on 2 vCPUs
(`--cpus 2 --verbose`, stdout left out):

```text
[boot_linux] RAM 0x80000000..0xa0000000, kernel 0x80000000..0x82000000, initrd Some((82000000, 8200079a)), DTB 0x9fe00000 (1772 bytes), 2 vCPUs
[boot_linux] cmdline: console=ttyAMA0 earlycon=pl011,mmio32,0x9000000 panic=-1
[boot_linux] vCPU 1: id 1, MPIDR 0x80000001, redistributor Ok(80c0000), GICR_TYPER Ok(100000110)
[boot_linux] vCPU 0: id 0, MPIDR 0x80000000, redistributor Ok(80a0000), GICR_TYPER Ok(0)
[boot_linux] vCPU 0: on at 0x80000000, x0 = 0x9fe00000
[boot_linux] vCPU 0: PSCI 0x84000000(0x0, 0x0, 0x0) -> Return(10000)
[boot_linux] vCPU 0: PSCI 0x84000006(0x0, 0x0, 0x0) -> Return(2)
[boot_linux] vCPU 0: PSCI 0x8400000a(0x80000000, 0x0, 0x0) -> Return(ffffffffffffffff)
[boot_linux] vCPU 0: PSCI 0x8400000a(0xc4000001, 0x0, 0x0) -> Return(0)
[boot_linux] vCPU 0: PSCI 0x8400000a(0xc400000e, 0x0, 0x0) -> Return(ffffffffffffffff)
[boot_linux] vCPU 0: PSCI 0x8400000a(0xc4000012, 0x0, 0x0) -> Return(ffffffffffffffff)
[boot_linux] vCPU 0: PSCI 0xc4000003(0x1, 0x81890498, 0x0) -> Return(0)
[boot_linux] vCPU 1: on at 0x81890498, x0 = 0x0
[boot_linux] vCPU 0: PSCI 0x84000008(0x0, 0x0, 0x0) -> SystemOff
[boot_linux] guest stopped: Ok(PowerOff)
```

The end of the initrd depends on the size of the initramfs, and the lines of
different vCPUs can come in a different order. The first `PSCI` calls are
the kernel asking for the PSCI version (`0x84000000`), the migration type
(`0x84000006`) and which functions exist (`0x8400000a`, `PSCI_FEATURES`); a
return value of `ffffffffffffffff` (`NOT_SUPPORTED`) there is normal.

## Using your own kernel

### Requirements

The kernel must be an **uncompressed arm64 `Image`**, and these options
must be built in (`=y`, not `=m`; there are no modules in the initramfs):

| Option | Needed for |
|---|---|
| `CONFIG_SERIAL_AMBA_PL011`, `CONFIG_SERIAL_AMBA_PL011_CONSOLE` | The console |
| `CONFIG_SERIAL_EARLYCON` | `earlycon=pl011,...` |
| `CONFIG_ARM_GIC_V3` | Interrupts |
| `CONFIG_ARM_ARCH_TIMER` | The timer |
| `CONFIG_ARM_PSCI_FW` | Starting secondary vCPUs and powering off |
| `CONFIG_SMP` | More than one vCPU |
| `CONFIG_BLK_DEV_INITRD`, `CONFIG_RD_GZIP` | A gzip'd initramfs |
| `CONFIG_DEVTMPFS`, `CONFIG_PROC_FS`, `CONFIG_SYSFS` | What the busybox `/init` mounts |
| `CONFIG_BINFMT_ELF`, `CONFIG_BINFMT_SCRIPT` | Running busybox and the `/init` script |

`make ARCH=arm64 defconfig` has all of them, and so has Debian's
`cloud-arm64` flavour. Check a configuration with:

```sh
grep -E '^CONFIG_(SERIAL_AMBA_PL011|SERIAL_AMBA_PL011_CONSOLE|SERIAL_EARLYCON|ARM_GIC_V3|ARM_ARCH_TIMER|ARM_PSCI_FW|SMP|BLK_DEV_INITRD|RD_GZIP|DEVTMPFS|PROC_FS|SYSFS|BINFMT_ELF|BINFMT_SCRIPT)=' .config
```

It must print 14 lines, one for each option in the table, and every line
must end in `=y`. A missing line means the option is not set at all.
For the downloaded kernel, the configuration is
`target/boot-linux/Image.config`. A distribution kernel usually installs it
as `/boot/config-<version>`; a kernel built with `CONFIG_IKCONFIG` contains
it (`scripts/extract-ikconfig Image` in a kernel tree).

### Telling the kernel formats apart

```console
$ file Image
Image: Linux kernel ARM64 boot executable Image, little-endian, 4K pages
```

That is the format the VMM needs. An uncompressed `Image` also has the
bytes `ARMd` (`"ARM\x64"`) at offset 56:

```console
$ dd if=Image bs=1 skip=56 count=4 2>/dev/null; echo
ARMd
```

| `file` says | What it is | What to do |
|---|---|---|
| `Linux kernel ARM64 boot executable Image` | An uncompressed `Image` | Use it |
| `gzip compressed data` | `Image.gz`, or a gzip'd `vmlinuz` | `gunzip -c Image.gz > Image` |
| `PE32+ executable ...`, and no `ARMd` at offset 56 | An EFI zboot image (`vmlinuz.efi`, `CONFIG_EFI_ZBOOT=y`) | Use `arch/arm64/boot/Image` from the build instead, or a kernel built without `CONFIG_EFI_ZBOOT` |
| `ELF 64-bit LSB ..., ARM aarch64` | `vmlinux`, the unstripped ELF kernel | Use `arch/arm64/boot/Image` |

The VMM rejects the wrong formats with a clear message (see
[Troubleshooting](#troubleshooting)).

### Building a kernel

Build on Linux (a container or a Linux VM works); macOS is not a supported
build host for the kernel:

```sh
make ARCH=arm64 defconfig
make ARCH=arm64 -j"$(nproc)" Image                 # on an arm64 Linux host
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j"$(nproc)" Image  # cross
```

The result is `arch/arm64/boot/Image`. Copy it to the Mac and pass it to
`--kernel`.

## How the initramfs is built

The kernel expects the initramfs as a *newc* cpio archive. It must contain
the device node `/dev/console` (character device 5, 1), because the kernel
opens it as init's stdin, stdout and stderr before it runs `/init`.
Creating a device node on macOS needs root, so `mkinitramfs.sh` never
creates one. Instead, `bsdtar` builds the archive from an *mtree* manifest
that describes every entry, device nodes included:

```text
#mtree
/set uid=0 gid=0 time=0.0
./dev type=dir mode=0755
./dev/console type=char mode=0600 device=native,5,1
./dev/null type=char mode=0666 device=native,1,3
...
./bin/busybox type=file mode=0755 contents=busybox
./bin/sh type=link mode=0777 link=busybox
./init type=file mode=0755 contents=init
```

```sh
bsdtar -cf initramfs.cpio --format newc @manifest
gzip -9n initramfs.cpio
```

`contents=` names the file on the Mac that supplies the data; ownership,
modes and device numbers come from the manifest. The kernel unpacks the
nodes as they are described: in the guest, `umount -l /dev; ls -l /dev`
shows the two nodes from the archive under devtmpfs.

Most kernels also carry a small built-in initramfs with `/dev/console`
(unless `CONFIG_INITRAMFS_SOURCE` replaces it), so a missing node often goes
unnoticed. The archive includes it anyway. As a second safety net, the Rust
`/init` mounts devtmpfs and opens `/dev/console` itself when it starts
without a stdout.

### Updating the pinned downloads

The versions and SHA-256 sums are at the top of each script.

- **Kernel.** Pick a newer `linux-image-*-cloud-arm64-unsigned` package from
  <https://packages.debian.org/trixie/linux-image-cloud-arm64>, download it,
  and update `KVER`, `DEB`, `DEB_SHA256` and `IMAGE_SHA256`. For the
  snapshot URL, `https://snapshot.debian.org/mr/file/<sha1 of the .deb>/info`
  gives the `first_seen` timestamp of the `debian` archive.
- **busybox.** Alpine keeps only the newest build of a package in each
  release. When the download returns 404, look up the current
  `busybox-static` at <https://pkgs.alpinelinux.org/packages?name=busybox-static&branch=v3.24&arch=aarch64>
  and update `APK`, `APK_SHA256` and `BUSYBOX_SHA256`.

A mismatch stops the script before it writes anything:

```text
fetch-kernel.sh: error: SHA-256 mismatch for linux-image-6.12.111+deb13-cloud-arm64-unsigned_6.12.111-1_arm64.deb
  expected ...
  got      ...
```

## Troubleshooting

**`boot_linux: Hypervisor.framework is unavailable (HV_DENIED (0xfae94007): binary lacks com.apple.security.hypervisor); is the binary signed with the com.apple.security.hypervisor entitlement?`**
The binary is not signed with the entitlement. Run it through `cargo run`
from inside the repository, or sign it by hand (see
[How signing works](#how-signing-works)). Typical causes: running
`target/release/examples/boot_linux` after a `cargo build` that rebuilt it,
running `cargo` from outside the repository, or overriding
`CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER`.

**`HV_UNSUPPORTED` or `HV_NO_DEVICE` in the same message.** The host cannot
run VMs: an Intel Mac, or macOS running in a VM without nested
virtualization. `sysctl kern.hv_support` prints 0.

**`boot_linux: this example needs the in-kernel GICv3 of macOS 15.0 or newer`.**
The host runs macOS 11 to 14. The `hvf` crate itself works there, but this
VMM relies on the in-kernel interrupt controller. Update macOS.

**Nothing at all on the console, but the VMM exits after a while or runs
on.** The kernel prints nowhere the VMM can see. If `/init` powers off by
itself (the Rust `/init`, or busybox with `hvf.autopoweroff`), the guest
still boots and the VMM exits at once with status 0, printing only
`boot_linux: the guest powered off`. A clean exit with no kernel log at all
points to the console settings:
- The kernel has no PL011 driver, or has it as a module. Firecracker's CI
  kernels, for example, boot and power off without printing anything. Check
  the options in [Requirements](#requirements).
- `--cmdline` lacks `console=ttyAMA0`, or names another console such as
  `console=ttyS0`. Remember that `--cmdline` replaces the whole default.
- Add `earlycon=pl011,mmio32,0x9000000` to see messages from the first
  instruction on, and `--verbose` to see the PSCI calls that prove the guest
  is running.

**The boot hangs.** Run with `--verbose` and
`earlycon=pl011,mmio32,0x9000000`. Look at the last kernel message and the
last `[boot_linux]` line: a secondary vCPU that never comes up, a PSCI call
that fails, or a missing device. `loglevel=8` or `ignore_loglevel` show all
kernel messages, `initcall_debug` shows each driver's initialization.
`Ctrl-A x` stops the VMM. A busybox boot under `cargo test` without
`hvf.autopoweroff` also "hangs": it waits in the shell.

**`Warning: unable to open an initial console.`** The initramfs has no
`/dev/console` and the kernel has no built-in initramfs that supplies one.
`/init` then has no stdin, stdout or stderr, so its output is lost. Build
the initramfs with `mkinitramfs.sh`, which adds the node, or add it to your
own archive (`nod /dev/console 0600 0 0 c 5 1` in a `gen_init_cpio` list,
`mknod -m 600 dev/console c 5 1` as root on Linux). Alternatively, let
`/init` mount devtmpfs and open `/dev/console` itself, as the Rust `/init`
does.

**`Kernel panic - not syncing: VFS: Unable to mount root fs on unknown-block(0,0)`**
and exit status 3. The kernel found no initramfs with an `/init`: `--initrd`
is missing, the file is not a (gzip'd) newc cpio archive, it has no `/init`,
or `rdinit=` names a file that does not exist.

**`Failed to execute /init (error -8)`, then `Kernel panic - not syncing: No working init found`.**
`/init` is not something the kernel can run: a binary for another
architecture (a macOS binary, for example), or a script whose interpreter
is missing.

**`Kernel panic - not syncing: Attempted to kill init!`** `/init` exited.
PID 1 must never exit; end it with `poweroff -f` or
`reboot(LINUX_REBOOT_CMD_POWER_OFF)`.

**`boot_linux: the kernel is gzip-compressed (Image.gz); decompress it first`,
`... is an EFI zboot image ...` or `... is not an arm64 Image (no "ARM\x64" magic)`.**
See [Telling the kernel formats apart](#telling-the-kernel-formats-apart).

**`the aarch64-unknown-none target is not installed`.** Run
`rustup target add aarch64-unknown-none`, which `mkinitramfs.sh --init rust`
needs.

**`could not download ... from any mirror`, or a SHA-256 mismatch.** Check
the network, then see
[Updating the pinned downloads](#updating-the-pinned-downloads).

**`^[[6n` after the prompt, or slow commands, when a script drives the
shell.** Each time the busybox shell prints its prompt, it asks the terminal
for the cursor position with `ESC [ 6 n`. A real terminal answers at once
and shows nothing, so at the keyboard you never notice. A program that
drives the console through a pty (`expect`, `script`, a CI job) sees the
bytes `ESC[6n` right after `~ # ` and does not answer. With `script` fed
from a pipe that only leaves the stray bytes in the log. With `expect`
waiting for `"~ # "`, each command ran only when that `expect` reached its
timeout, so every command took about as long as `set timeout`.

Wait for the prompt together with the query and answer it, here with row
24, column 5 (the prompt `~ # ` is 4 characters wide):

```tcl
proc prompt {} {
    expect {
        -ex "~ # \033\[6n" { send "\033\[24;5R" }
        timeout { puts "no prompt"; exit 1 }
    }
}
prompt
send "uname -a\r"
prompt
send "exit\r"
expect eof
```

The commands then run at once. Answer with a row other than 1: busybox
can take `ESC[1;5R` for a function key (`Ctrl-F3`) and lose the input that
follows. Setting `TERM=dumb` does not help; this busybox sends the query
anyway. If the run needs no shell, boot with `hvf.autopoweroff` (see
[Step 5](#step-5-boot-without-a-shell-on-more-vcpus)) or use the Rust
`/init` instead.

**The terminal behaves oddly after the VMM was killed.** The VMM restores
the terminal when it exits normally, but not after `kill -9`. Type `reset`
(or `stty sane`) and press Enter.
