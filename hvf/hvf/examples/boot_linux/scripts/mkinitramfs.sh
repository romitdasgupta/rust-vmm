#!/bin/sh
# Copyright 2026 The rust-vmm authors.
# SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause
#
# Builds an initramfs for the boot_linux example.
#
# Usage: mkinitramfs.sh [--init busybox|rust] [-o OUT]
#
#   --init busybox  (default) Download Alpine's static busybox for aarch64
#                   (pinned by SHA-256) and add a shell /init. The /init
#                   prints HVF-BOOT-OK and `uname -a`, then starts an
#                   interactive shell, or powers off at once when the kernel
#                   command line contains hvf.autopoweroff.
#   --init rust     Build the no_std Rust /init in ../init (needs the
#                   aarch64-unknown-none target). It prints HVF-BOOT-OK and
#                   the uname fields, then powers off. Nothing is downloaded.
#   -o OUT          Output file (default: <repo>/target/boot-linux/initramfs.cpio.gz)
#
# No root is needed. bsdtar (macOS tar) writes the newc cpio archive from an
# mtree manifest, which declares the /dev/console and /dev/null device nodes
# the archive needs without creating them on the host. Nothing downloaded is
# executed on the host: busybox is an aarch64 Linux binary and only runs in
# the guest.

set -eu
# `cd` with a relative path consults an exported CDPATH, and then prints the
# directory (or fails), which breaks the `$(cd ... && pwd)` lines below.
unset CDPATH

# --- Pins. To move to a newer busybox, update all of these together. -------
APK=busybox-static-1.37.0-r31.apk
APK_SHA256=965777e06b94bf11981d5f4ecdfcd577879f4b0dda544294d1dd3f72b217bc75
BUSYBOX_SHA256=999cb969d09093a71716cfc747bb53cdada3f332c05eb5046c56e0f66a4d6d22
# Alpine keeps only the newest build of a package in each release branch.
# When a rebuild replaces r31, these URLs return 404 and the pins above need
# an update (see README.md).
APK_URLS="
https://dl-cdn.alpinelinux.org/alpine/v3.24/main/aarch64/$APK
https://mirrors.edge.kernel.org/alpine/v3.24/main/aarch64/$APK
"
# ---------------------------------------------------------------------------

prog=mkinitramfs.sh

die() {
    echo "$prog: error: $*" >&2
    exit 1
}

usage() {
    echo "usage: $prog [--init busybox|rust] [-o OUT]"
}

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../../.." && pwd)
init=busybox
out=$repo/target/boot-linux/initramfs.cpio.gz

while [ "$#" -gt 0 ]; do
    case "$1" in
        --init)
            [ "$#" -ge 2 ] || die "--init needs a value (busybox or rust)"
            init=$2
            shift 2
            ;;
        --init=*)
            init=${1#--init=}
            shift
            ;;
        -o | --output)
            [ "$#" -ge 2 ] || die "$1 needs a file name"
            out=$2
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
done
case "$init" in
    busybox | rust) ;;
    *) die "unknown --init $init (expected busybox or rust)" ;;
esac

for tool in bsdtar gzip dd od; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool not found in PATH"
done

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        die "neither shasum nor sha256sum found in PATH"
    fi
}

# Succeeds if $1 is an ELF file for aarch64 (e_machine 183 at offset 18).
is_aarch64_elf() {
    [ "$(dd if="$1" bs=1 count=4 2>/dev/null | od -An -tx1 | tr -d ' \n')" = 7f454c46 ] &&
        [ "$(dd if="$1" bs=1 skip=18 count=2 2>/dev/null | od -An -tx1 | tr -d ' \n')" = b700 ]
}

work=$(mktemp -d "${TMPDIR:-/tmp}/hvf-mkinitramfs.XXXXXX")
trap 'rm -rf "$work"' EXIT
# The files that go into the archive, under the names the manifest uses.
stage=$work/stage
mkdir "$stage"

# Directories and device nodes common to both variants. The kernel opens
# /dev/console for init before it runs it. Without the node, init starts
# with no stdin, stdout or stderr and the kernel warns "unable to open an
# initial console". (Most kernels also carry a built-in initramfs with
# /dev/console, but one built with CONFIG_INITRAMFS_SOURCE may not.)
# "device=native,MAJOR,MINOR" is how libarchive's mtree reader takes a
# device number; the newc header then holds MAJOR and MINOR as given.
manifest=$work/manifest
cat >"$manifest" <<'EOF'
#mtree
/set uid=0 gid=0 time=0.0
./dev type=dir mode=0755
./dev/console type=char mode=0600 device=native,5,1
./dev/null type=char mode=0666 device=native,1,3
./proc type=dir mode=0555
./sys type=dir mode=0555
./tmp type=dir mode=01777
./root type=dir mode=0700
EOF

fetch_busybox() {
    mkdir "$work/dl" "$work/apk"
    apk=$work/dl/$APK
    got=
    for url in $APK_URLS; do
        echo "Downloading $url"
        if curl -fL --proto '=https' --tlsv1.2 --retry 3 -sS -o "$apk" "$url"; then
            got=$url
            break
        fi
        echo "$prog: download failed, trying the next mirror" >&2
    done
    [ -n "$got" ] || die "could not download $APK from any mirror (has Alpine
  replaced it with a newer build? Then update the pins in $prog.)"

    sum=$(sha256 "$apk")
    [ "$sum" = "$APK_SHA256" ] || die "SHA-256 mismatch for $APK
  expected $APK_SHA256
  got      $sum"
    echo "Verified $APK (sha256 $sum)"

    # An .apk is a series of gzip'd tar streams, which bsdtar reads as one.
    bsdtar -xf "$apk" -C "$work/apk" bin/busybox.static ||
        die "cannot extract bin/busybox.static from $APK"
    bb=$work/apk/bin/busybox.static
    is_aarch64_elf "$bb" || die "bin/busybox.static is not an aarch64 ELF file"
    sum=$(sha256 "$bb")
    [ "$sum" = "$BUSYBOX_SHA256" ] || die "SHA-256 mismatch for busybox.static
  expected $BUSYBOX_SHA256
  got      $sum"
    cp "$bb" "$stage/busybox"
}

case "$init" in
    busybox)
        command -v curl >/dev/null 2>&1 || die "curl not found in PATH"
        fetch_busybox
        # The /init script. It runs as PID 1 with /dev/console on fds 0-2.
        cat >"$stage/init" <<'EOF'
#!/bin/busybox sh
# /init of the boot_linux busybox initramfs (made by mkinitramfs.sh).
/bin/busybox mkdir -p /sbin /usr/bin /usr/sbin
/bin/busybox --install -s
export PATH=/sbin:/usr/sbin:/bin:/usr/bin HOME=/root
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
hostname hvf
echo "HVF-BOOT-OK: hello from busybox userspace"
uname -a
case " $(cat /proc/cmdline) " in
*" hvf.autopoweroff "*)
    poweroff -f
    ;;
esac
# Run the shell on the real console device (ttyAMA0) as its controlling
# terminal, so that job control and Ctrl-C work. /dev/console cannot be one.
# The last console in the list is /dev/console.
tty=$(cat /sys/class/tty/console/active 2>/dev/null)
tty=${tty##* }
echo "Type 'exit' or press Ctrl-D to power off, or Ctrl-A x to stop the VMM."
cd /root || cd /
if [ -n "$tty" ] && [ -c "/dev/$tty" ]; then
    setsid -c sh -l <"/dev/$tty" >"/dev/$tty" 2>&1
else
    sh -l
fi
poweroff -f
EOF
        cat >>"$manifest" <<'EOF'
./bin type=dir mode=0755
./bin/busybox type=file mode=0755 contents=busybox
./bin/sh type=link mode=0777 link=busybox
./init type=file mode=0755 contents=init
EOF
        ;;
    rust)
        command -v cargo >/dev/null 2>&1 ||
            die "cargo not found in PATH (install Rust with rustup)"
        if command -v rustup >/dev/null 2>&1 &&
            ! rustup target list --installed | grep -qx aarch64-unknown-none; then
            die "the aarch64-unknown-none target is not installed; run:
  rustup target add aarch64-unknown-none"
        fi
        crate=$(cd "$here/../init" && pwd)
        target_dir=$repo/target/boot-linux/init
        echo "Building the Rust /init in $crate"
        # Cargo reads ../init/.cargo/config.toml (the target and linker
        # flags) only when it runs from that directory.
        (cd "$crate" && CARGO_TARGET_DIR=$target_dir cargo build --release 2>&1) ||
            die "cargo build of $crate failed"
        bin=$target_dir/aarch64-unknown-none/release/hvf-boot-init
        is_aarch64_elf "$bin" || die "$bin is not an aarch64 ELF file"
        cp "$bin" "$stage/init"
        echo "./init type=file mode=0755 contents=init" >>"$manifest"
        ;;
esac

# Write the archive. bsdtar resolves contents= relative to the current
# directory, so run it in the staging directory. The kernel's initramfs
# unpacker takes gzip (CONFIG_RD_GZIP), which every common kernel has.
case "$out" in
    /*) ;;
    *) out=$(pwd)/$out ;;
esac
mkdir -p "$(dirname "$out")"
(cd "$stage" && bsdtar -cf "$work/initramfs.cpio" --format newc "@$manifest") ||
    die "bsdtar cannot write the cpio archive"
gzip -9n <"$work/initramfs.cpio" >"$out.tmp" || die "cannot write $out.tmp"
mv -f "$out.tmp" "$out"
echo "Wrote $out ($init /init, $(wc -c <"$out" | tr -d ' ') bytes)"
