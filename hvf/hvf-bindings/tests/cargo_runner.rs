// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Checks the environment that test binaries start with.
//!
//! In this repository Cargo runs aarch64-apple-darwin binaries through
//! `hvf/scripts/codesign-run.sh`, a `/bin/sh` script. System Integrity
//! Protection removes every `DYLD_*` variable before a protected binary such
//! as `/bin/sh` starts, so the script has to restore the
//! `DYLD_FALLBACK_LIBRARY_PATH` that Cargo sets. Without it, any binary
//! linked against the toolchain's dylibs (for example with
//! `-C prefer-dynamic`) fails to start.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

#[test]
fn test_dyld_fallback_library_path_reaches_tests() {
    // Only Cargo (directly or through the runner) sets the variable, so skip
    // when the binary was started some other way.
    if std::env::var_os("CARGO").is_none() {
        println!("SKIP: not started by cargo");
        return;
    }
    let path = std::env::var("DYLD_FALLBACK_LIBRARY_PATH")
        .expect("DYLD_FALLBACK_LIBRARY_PATH is not set: the cargo runner dropped it");
    assert!(
        path.split(':')
            .any(|dir| dir.ends_with("/lib/rustlib/aarch64-apple-darwin/lib")),
        "DYLD_FALLBACK_LIBRARY_PATH lacks the toolchain's libraries: {path}"
    );
    // Test binaries live in <target>/<profile>/deps.
    let deps = std::fs::canonicalize(std::env::current_exe().unwrap().parent().unwrap()).unwrap();
    assert!(
        path.split(':')
            .any(|dir| std::fs::canonicalize(dir).is_ok_and(|dir| dir == deps)),
        "DYLD_FALLBACK_LIBRARY_PATH lacks {}: {path}",
        deps.display()
    );
}
