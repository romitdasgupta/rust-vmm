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

log=$(mktemp "${TMPDIR:-/tmp}/hvf-codesign.XXXXXX")
trap 'rm -f "$log"' EXIT

if ! codesign --sign - --force --entitlements "$here/hvf.entitlements" "$bin" >"$log" 2>&1; then
    echo "codesign-run.sh: failed to sign $bin:" >&2
    cat "$log" >&2
    exit 1
fi

rm -f "$log"
trap - EXIT
exec "$bin" "$@"
