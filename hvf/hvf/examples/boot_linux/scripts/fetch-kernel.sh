#!/bin/sh
# Copyright 2026 The rust-vmm authors.
# SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause
#
# Downloads a pinned Debian arm64 kernel and writes it as an uncompressed
# arm64 Image for the boot_linux example.
#
# Usage: fetch-kernel.sh [OUTDIR]
#
# OUTDIR defaults to target/boot-linux in the repository. The script writes
# OUTDIR/Image (and OUTDIR/Image.config, the kernel's .config). It needs only
# tools that ship with macOS: curl, bsdtar (tar), shasum and dd.
#
# The kernel is Debian 13 (trixie)'s cloud-arm64 flavour, which has the
# PL011 console, GICv3, the architected timer and PSCI built in. Its
# vmlinuz is an uncompressed Image. Both the .deb and the Image are pinned
# by SHA-256, so a changed download is rejected. Nothing downloaded is
# executed.

set -eu
# `cd` with a relative path consults an exported CDPATH, and then prints the
# directory (or fails), which breaks the `$(cd ... && pwd)` lines below.
unset CDPATH

# --- Pins. To move to a newer kernel, update all of these together. -------
KVER=6.12.111+deb13-cloud-arm64
DEB=linux-image-6.12.111+deb13-cloud-arm64-unsigned_6.12.111-1_arm64.deb
DEB_SHA256=88926feba4ab6db49c6ce072f099640cd14512af8b47a1fa2394b8d596f688da
IMAGE_SHA256=7df55571ea48d32aee64c37af99c8bbb8b57b49adb0806707218e864c3541703
# deb.debian.org is a fast CDN, but drops the package once trixie moves to a
# newer kernel. snapshot.debian.org keeps every package Debian ever
# published, so it is the fallback that keeps this script working.
DEB_URLS="
https://deb.debian.org/debian/pool/main/l/linux/$DEB
https://snapshot.debian.org/archive/debian/20261011T144745Z/pool/main/l/linux/$DEB
"
# ---------------------------------------------------------------------------

prog=fetch-kernel.sh

die() {
    echo "$prog: error: $*" >&2
    exit 1
}

usage() {
    echo "usage: $prog [OUTDIR]  (default OUTDIR: <repo>/target/boot-linux)"
}

case "${1:-}" in
    -h | --help)
        usage
        exit 0
        ;;
    -*)
        usage >&2
        exit 2
        ;;
esac
[ "$#" -le 1 ] || {
    usage >&2
    exit 2
}

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../../.." && pwd)
outdir=${1:-$repo/target/boot-linux}

for tool in curl bsdtar dd od; do
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

# An arm64 Image has the magic "ARM\x64" at offset 56
# (Documentation/arch/arm64/booting.rst).
is_arm64_image() {
    [ "$(dd if="$1" bs=1 skip=56 count=4 2>/dev/null)" = "ARMd" ]
}

image=$outdir/Image
if [ -f "$image" ] && [ "$(sha256 "$image")" = "$IMAGE_SHA256" ]; then
    echo "$image is up to date (Linux $KVER)"
    exit 0
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/hvf-fetch-kernel.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir "$work/dl" "$work/deb" "$work/data"

deb=$work/dl/$DEB
# Show curl's progress bar only when stderr is a terminal. In a log it is
# noise, so print only errors there.
if [ -t 2 ]; then
    progress=--progress-bar
else
    progress=-sS
fi
got=
for url in $DEB_URLS; do
    echo "Downloading $url"
    if curl -fL --proto '=https' --tlsv1.2 --retry 3 "$progress" \
        -o "$deb" "$url"; then
        got=$url
        break
    fi
    echo "$prog: download failed, trying the next mirror" >&2
done
[ -n "$got" ] || die "could not download $DEB from any mirror"

sum=$(sha256 "$deb")
[ "$sum" = "$DEB_SHA256" ] || die "SHA-256 mismatch for $DEB
  expected $DEB_SHA256
  got      $sum"
echo "Verified $DEB (sha256 $sum)"

# A .deb is an ar archive with data.tar.* inside; bsdtar reads both.
bsdtar -xf "$deb" -C "$work/deb" || die "cannot unpack $DEB"
set -- "$work"/deb/data.tar.*
[ -f "$1" ] || die "no data.tar.* in $DEB"
bsdtar -xf "$1" -C "$work/data" "./boot/vmlinuz-$KVER" "./boot/config-$KVER" ||
    die "cannot extract /boot/vmlinuz-$KVER from $DEB"

kernel=$work/data/boot/vmlinuz-$KVER
is_arm64_image "$kernel" ||
    die "/boot/vmlinuz-$KVER is not an uncompressed arm64 Image"
sum=$(sha256 "$kernel")
[ "$sum" = "$IMAGE_SHA256" ] || die "SHA-256 mismatch for the extracted Image
  expected $IMAGE_SHA256
  got      $sum"

mkdir -p "$outdir"
cp "$work/data/boot/config-$KVER" "$outdir/Image.config"
cp "$kernel" "$image.tmp"
mv -f "$image.tmp" "$image"
echo "Wrote $image (Linux $KVER, sha256 $sum)"
