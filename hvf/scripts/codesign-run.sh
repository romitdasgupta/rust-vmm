#!/bin/sh
# Copyright 2026 The rust-vmm authors.
# SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause
#
# Cargo runner for aarch64-apple-darwin (see .cargo/config.toml).
#
# Hypervisor.framework refuses to create a VM (HV_DENIED) unless the calling
# binary carries the com.apple.security.hypervisor entitlement. Cargo produces
# fresh, unsigned test/example/doctest binaries on every build, so this script
# ad-hoc signs the binary with that entitlement and then executes it with the
# remaining arguments.
#
# Usage: codesign-run.sh <binary> [args...]

set -eu

if [ "$#" -lt 1 ]; then
    echo "usage: $0 <binary> [args...]" >&2
    exit 2
fi

here=$(cd "$(dirname "$0")" && pwd)
bin=$1
shift

# Resolve bare command names (no '/') the same way exec would.
case "$bin" in
    */*) ;;
    *)
        resolved=$(command -v "$bin" || true)
        if [ -z "$resolved" ]; then
            echo "codesign-run.sh: $bin: command not found" >&2
            exit 127
        fi
        bin=$resolved
        ;;
esac

# Several processes may run the same binary at once (cargo-nextest runs each
# test in its own process, or two `cargo test` run side by side). Signing in
# place would let them rewrite the file under each other, so skip binaries
# that already carry the entitlement, and otherwise sign a private copy and
# rename it into place: every process then executes a fully signed file.
if ! codesign -d --entitlements - "$bin" 2>/dev/null |
    grep -q com.apple.security.hypervisor; then
    tmp="$bin.codesign.$$"
    log=$(mktemp "${TMPDIR:-/tmp}/hvf-codesign.XXXXXX")
    trap 'rm -f "$tmp" "$tmp.cstemp" "$log"' EXIT

    cp -p "$bin" "$tmp"
    if ! codesign --sign - --force --entitlements "$here/hvf.entitlements" "$tmp" >"$log" 2>&1; then
        echo "codesign-run.sh: failed to sign $bin:" >&2
        cat "$log" >&2
        exit 1
    fi
    mv -f "$tmp" "$bin"

    rm -f "$log"
    trap - EXIT
fi

exec "$bin" "$@"
