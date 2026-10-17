// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Checks the bindings against the installed SDK headers with `xcrun clang`.
//!
//! The Rust side of every check comes from the tables the binding macros emit
//! (`__ALL_CONSTS`, `__FNS`) and from `size_of`/`offset_of`, so nothing is
//! written twice. The checks are:
//!
//! * `test_values_types_and_layout`: a C file of `_Static_assert`s comparing
//!   every constant's value, every scalar type's size and signedness, and
//!   every struct's layout with the headers, plus one function pointer
//!   initialization per function, which clang rejects
//!   (`-Wincompatible-function-pointer-types`) unless the C prototype
//!   matches the Rust declaration exactly.
//! * `test_completeness_and_availability`: every constant and function in
//!   the preprocessed headers is bound (or listed in `tests/sdk_skip.txt`),
//!   with the type name and macOS availability the headers declare.
//!
//! Only items available in the installed SDK are checked, so an older SDK
//! (for example on a hosted CI runner) checks a subset. If `xcrun` is missing
//! the tests print `SKIP` and pass, unless `HVF_SDK_CHECK=1` is set.

#![cfg(target_os = "macos")]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::mem::{align_of, offset_of, size_of};
use std::path::{Path, PathBuf};
use std::process::Command;

use hvf_bindings::*;

/// The name of the SIMD setter, which is reached through a trampoline and
/// therefore has no `hv_fns!` entry. Functions are only bound on aarch64.
#[cfg(target_arch = "aarch64")]
const SIMD_SETTER: &str = "hv_vcpu_set_simd_fp_reg";

struct Sdk {
    path: String,
    version: (u16, u16),
}

impl Sdk {
    /// The clang target triple for this SDK's macOS release.
    fn target(&self) -> String {
        format!("arm64-apple-macos{}.{}", self.version.0, self.version.1)
    }

    fn has(&self, since: (u16, u16)) -> bool {
        since <= self.version
    }
}

fn xcrun(args: &[&str]) -> Option<String> {
    let out = Command::new("xcrun").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
}

fn parse_version(v: &str) -> (u16, u16) {
    let mut parts = v.trim().split('.').map(|p| p.parse::<u16>());
    let major = parts.next().and_then(Result::ok);
    let minor = parts.next().unwrap_or(Ok(0)).ok();
    match (major, minor) {
        (Some(major), Some(minor)) => (major, minor),
        _ => panic!("cannot parse version {v:?}"),
    }
}

/// Finds the SDK, or returns `None` (after printing why) if there is none and
/// the check is not required.
fn sdk() -> Option<Sdk> {
    let found = xcrun(&["--sdk", "macosx", "--show-sdk-path"]).zip(xcrun(&[
        "--sdk",
        "macosx",
        "--show-sdk-version",
    ]));
    match found {
        Some((path, version)) => Some(Sdk {
            path,
            version: parse_version(&version),
        }),
        None if std::env::var_os("HVF_SDK_CHECK").is_some_and(|v| v == "1") => {
            panic!("HVF_SDK_CHECK=1 but `xcrun --show-sdk-path` failed")
        }
        None => {
            eprintln!("SKIP: no macOS SDK found with xcrun");
            None
        }
    }
}

fn work_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hvf-sdk-check");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `xcrun clang` for the SDK on `source` (written to `file`) with
/// `args`, and returns its standard output. Panics with clang's diagnostics if
/// it fails.
fn clang(sdk: &Sdk, file: &str, source: &str, args: &[&str]) -> String {
    let path = work_dir().join(file);
    std::fs::write(&path, source).unwrap();
    let target = sdk.target();
    let out = Command::new("xcrun")
        .args(["clang", "-target", &target, "-isysroot", &sdk.path])
        .args(args)
        .arg(&path)
        .output()
        .expect("failed to run xcrun clang");
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let errors = stderr.lines().filter(|l| l.contains(": error:")).count();
        panic!(
            "clang rejected {} ({} errors):\n{}",
            path.display(),
            errors,
            stderr
        );
    }
    String::from_utf8(out.stdout).unwrap()
}

fn all_consts() -> impl Iterator<Item = &'static ConstEntry> {
    __ALL_CONSTS.iter().flat_map(|t| t.iter())
}

/// Translates a Rust type, as `stringify!` renders it in `__FNS`, to C.
fn c_type(rust: &str) -> String {
    let rust = rust.trim();
    if let Some(rest) = rust.strip_prefix('*') {
        let rest = rest.trim_start();
        if let Some(pointee) = rest.strip_prefix("mut ") {
            return format!("{} *", c_type(pointee));
        }
        if let Some(pointee) = rest.strip_prefix("const ") {
            return format!("const {} *", c_type(pointee));
        }
        panic!("unsupported pointer type {rust:?}");
    }
    match rust {
        "()" | "c_void" => "void",
        "bool" => "bool",
        "u8" => "uint8_t",
        "u16" => "uint16_t",
        "u32" => "uint32_t",
        "u64" => "uint64_t",
        "i32" => "int32_t",
        "usize" => "size_t",
        t if t.starts_with("hv_") && t.ends_with("_t") => t,
        t => panic!("no C mapping for Rust type {t:?}; extend `c_type`"),
    }
    .to_owned()
}

/// `(name, size, signed)` of every scalar type alias. Every type named in
/// `__ALL_CONSTS` must be listed (checked below).
fn scalar_types() -> Vec<(&'static str, usize, bool)> {
    macro_rules! types {
        ($($t:ident),* $(,)?) => {
            vec![$((stringify!($t), size_of::<$t>(), <$t>::MIN != 0)),*]
        };
    }
    types![
        hv_return_t,
        hv_ipa_t,
        hv_vcpu_t,
        hv_exception_syndrome_t,
        hv_exception_address_t,
        hv_memory_flags_t,
        hv_allocate_flags_t,
        hv_exit_reason_t,
        hv_interrupt_type_t,
        hv_ipa_granule_t,
        hv_cache_type_t,
        hv_feature_reg_t,
        hv_tlbi_op_t,
        hv_reg_t,
        hv_simd_fp_reg_t,
        hv_sys_reg_t,
        hv_sme_z_reg_t,
        hv_sme_p_reg_t,
        hv_gic_intid_t,
        hv_gic_distributor_reg_t,
        hv_gic_redistributor_reg_t,
        hv_gic_icc_reg_t,
        hv_gic_ich_reg_t,
        hv_gic_icv_reg_t,
        hv_gic_msi_reg_t,
    ]
}

/// The first macOS release that declares the scalar type `name`: that of its
/// oldest constant. Types without constants (`hv_ipa_t`, ...) exist in every
/// SDK the bindings support.
fn type_since(name: &str) -> Option<(u16, u16)> {
    all_consts().filter(|c| c.1 == name).map(|c| c.3).min()
}

/// The `_Static_assert`s for the layouts of the structs `sdk` declares, with
/// the Rust values.
fn layout_asserts(sdk: &Sdk, c: &mut String) {
    /// Every SDK the bindings support.
    const BASE: (u16, u16) = (11, 0);
    /// The SME state types.
    const SME: (u16, u16) = (15, 2);
    /// The GIC configuration and state objects.
    const GIC: (u16, u16) = (15, 0);

    let mut check = |expr: String| {
        writeln!(c, "_Static_assert({expr}, \"layout: {expr}\");").unwrap();
    };
    macro_rules! layout {
        ($t:ident { $($f:ident),* }) => {
            check(format!("sizeof({}) == {}", stringify!($t), size_of::<$t>()));
            check(format!("_Alignof({}) == {}", stringify!($t), align_of::<$t>()));
            $(
                check(format!(
                    "offsetof({}, {}) == {}",
                    stringify!($t),
                    stringify!($f),
                    offset_of!($t, $f)
                ));
            )*
        };
    }
    layout!(hv_vcpu_exit_exception_t {
        syndrome,
        virtual_address,
        physical_address
    });
    layout!(hv_vcpu_exit_t { reason, exception });
    layout!(hv_simd_fp_uchar16_t {});
    if sdk.has(SME) {
        layout!(hv_vcpu_sme_state_t {
            streaming_sve_mode_enabled,
            za_storage_enabled
        });
        layout!(hv_sme_zt0_uchar64_t {});
    }

    // Field types (the offsets above only pin down their positions).
    for (since, ty, field, c_ty) in [
        (BASE, "hv_vcpu_exit_t", "reason", "hv_exit_reason_t"),
        (
            BASE,
            "hv_vcpu_exit_t",
            "exception",
            "hv_vcpu_exit_exception_t",
        ),
        (
            BASE,
            "hv_vcpu_exit_exception_t",
            "syndrome",
            "hv_exception_syndrome_t",
        ),
        (
            BASE,
            "hv_vcpu_exit_exception_t",
            "virtual_address",
            "hv_exception_address_t",
        ),
        (
            BASE,
            "hv_vcpu_exit_exception_t",
            "physical_address",
            "hv_ipa_t",
        ),
        (
            SME,
            "hv_vcpu_sme_state_t",
            "streaming_sve_mode_enabled",
            "bool",
        ),
        (SME, "hv_vcpu_sme_state_t", "za_storage_enabled", "bool"),
    ] {
        if sdk.has(since) {
            check(format!(
                "__builtin_types_compatible_p(__typeof__((({ty} *)0)->{field}), {c_ty})"
            ));
        }
    }

    // The vector types: element type and count, which the Rust newtypes
    // represent as a byte array.
    check("__builtin_types_compatible_p(hv_simd_fp_uchar16_t, rs_uchar16)".into());
    if sdk.has(SME) {
        check("__builtin_types_compatible_p(hv_sme_zt0_uchar64_t, rs_uchar64)".into());
    }

    // The opaque objects are pointers to the structs of the same name.
    for (name, since) in [
        ("hv_vm_config", BASE),
        ("hv_vcpu_config", BASE),
        ("hv_gic_config", GIC),
        ("hv_gic_state", GIC),
    ] {
        if sdk.has(since) {
            check(format!(
                "__builtin_types_compatible_p({name}_t, struct {name}_s *)"
            ));
        }
    }
}

#[test]
fn test_values_types_and_layout() {
    let Some(sdk) = sdk() else { return };
    let mut c = String::new();
    c.push_str(
        "#include <Hypervisor/Hypervisor.h>\n\
         #include <stddef.h>\n\
         typedef __attribute__((ext_vector_type(16))) uint8_t rs_uchar16;\n\
         typedef __attribute__((ext_vector_type(64))) uint8_t rs_uchar64;\n\n",
    );

    // Scalar types, except those newer than the SDK.
    let types = scalar_types();
    let mut checked_types = 0;
    for &(name, size, signed) in &types {
        if type_since(name).is_some_and(|since| !sdk.has(since)) {
            continue;
        }
        checked_types += 1;
        writeln!(
            c,
            "_Static_assert(sizeof({name}) == {size} && (({name})-1 < ({name})0) == {}, \"type {name}\");",
            u8::from(signed)
        )
        .unwrap();
    }
    let type_names: BTreeSet<_> = types.iter().map(|t| t.0).collect();

    // Constants.
    let mut consts = 0;
    for &(name, ty, value, since) in all_consts() {
        assert!(
            type_names.contains(ty),
            "{name}: type {ty} not in scalar_types()"
        );
        if sdk.has(since) {
            writeln!(c, "_Static_assert({name} == ({ty})({value}), \"{name}\");").unwrap();
            consts += 1;
        }
    }

    layout_asserts(&sdk, &mut c);

    // Function prototypes. The SIMD setter's by-value vector argument is what
    // the trampoline assumes.
    #[cfg(target_arch = "aarch64")]
    {
        c.push('\n');
        for f in __FNS.iter().filter(|f| sdk.has(f.since)) {
            let args: Vec<String> = f.args.iter().map(|a| c_type(a)).collect();
            let args = if args.is_empty() {
                "void".to_owned()
            } else {
                args.join(", ")
            };
            writeln!(
                c,
                "__attribute__((unused)) static {} (*chk_{})({}) = {};",
                c_type(f.ret),
                f.name,
                args,
                f.name
            )
            .unwrap();
        }
        writeln!(
            c,
            "_Static_assert(__builtin_types_compatible_p(__typeof__({SIMD_SETTER}), \
             hv_return_t (hv_vcpu_t, hv_simd_fp_reg_t, hv_simd_fp_uchar16_t)), \"simd abi\");"
        )
        .unwrap();
    }

    clang(
        &sdk,
        "checks.c",
        &c,
        &[
            "-fsyntax-only",
            "-Werror",
            "-Wincompatible-function-pointer-types",
            "-Wincompatible-pointer-types",
            "-Wconstant-conversion",
        ],
    );
    assert!(consts > 0);
    eprintln!(
        "checked {consts} constants, {checked_types} types against SDK {:?}",
        sdk.version
    );
}

/// An item found in the preprocessed headers.
#[derive(Debug)]
struct HeaderItem {
    /// The `typedef` name of the enclosing enum, if it is a typedef.
    ty: Option<String>,
    /// `API_AVAILABLE(macos(..))` of the item, or of its enclosing enum.
    since: Option<(u16, u16)>,
}

/// Returns the first `availability(macos,introduced=X)` in `text`.
fn availability(text: &str) -> Option<(u16, u16)> {
    const KEY: &str = "availability(macos,introduced=";
    let start = text.find(KEY)? + KEY.len();
    let end = start + text[start..].find(')')?;
    Some(parse_version(&text[start..end]))
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Returns the identifier at the start of `text`.
fn leading_ident(text: &str) -> &str {
    let text = text.trim_start();
    &text[..text.find(|c| !is_ident(c)).unwrap_or(text.len())]
}

/// Returns the byte offsets of the keyword `word` in `text`.
fn find_word(text: &str, word: &str) -> Vec<usize> {
    text.match_indices(word)
        .filter(|&(i, _)| {
            let before = text[..i].chars().next_back();
            let after = text[i + word.len()..].chars().next();
            !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Splits `text` at commas that are not nested in parentheses.
fn split_top_level(text: &str) -> Vec<&str> {
    let (mut depth, mut start, mut parts) = (0i32, 0, Vec::new());
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Parses the enumerators and function declarations out of the preprocessed
/// umbrella header.
fn parse_header(pp: &str) -> (BTreeMap<String, HeaderItem>, BTreeMap<String, HeaderItem>) {
    let flat = pp.replace('\n', " ");
    let (mut consts, mut fns) = (BTreeMap::new(), BTreeMap::new());
    for stmt in flat.split(';') {
        if let Some(&pos) = find_word(stmt, "enum").first() {
            let (Some(open), Some(close)) = (stmt.find('{'), stmt.rfind('}')) else {
                continue;
            };
            if open < pos {
                continue;
            }
            // `API_AVAILABLE` may precede the typedef or follow its name.
            let enum_since = availability(&stmt[..pos]).or(availability(&stmt[close..]));
            let ty = find_word(stmt, "typedef")
                .first()
                .map(|_| leading_ident(&stmt[close + 1..]).to_owned())
                .filter(|t| !t.is_empty());
            for item in split_top_level(&stmt[open + 1..close]) {
                // The umbrella header also pulls in libSystem headers; only
                // the framework's own enumerators are of interest.
                let name = leading_ident(item);
                if !name.starts_with("HV_") {
                    continue;
                }
                let item_since = availability(item).or(enum_since);
                consts.insert(
                    name.to_owned(),
                    HeaderItem {
                        ty: ty.clone(),
                        since: item_since,
                    },
                );
            }
        } else if find_word(stmt, "typedef").is_empty() {
            // A function declaration: the first `hv_*` identifier followed by
            // an opening parenthesis.
            let name = find_word_prefixed(stmt, "hv_")
                .into_iter()
                .find(|&(_, end)| stmt[end..].trim_start().starts_with('('));
            if let Some((start, end)) = name {
                fns.insert(
                    stmt[start..end].to_owned(),
                    HeaderItem {
                        ty: None,
                        since: availability(stmt),
                    },
                );
            }
        }
    }
    (consts, fns)
}

/// Returns `(start, end)` of every identifier in `text` starting with
/// `prefix`.
fn find_word_prefixed(text: &str, prefix: &str) -> Vec<(usize, usize)> {
    text.match_indices(prefix)
        .filter(|&(i, _)| !text[..i].chars().next_back().is_some_and(is_ident))
        .map(|(i, _)| {
            let len = text[i..].find(|c| !is_ident(c)).unwrap_or(text.len() - i);
            (i, i + len)
        })
        .collect()
}

/// Reads `tests/sdk_skip.txt`: SDK items deliberately not bound, one per
/// line, with `#` comments.
fn skip_list() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sdk_skip.txt");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| l.split('#').next().unwrap().trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn test_completeness_and_availability() {
    let Some(sdk) = sdk() else { return };
    let pp = clang(
        &sdk,
        "umbrella.c",
        "#include <Hypervisor/Hypervisor.h>\n",
        &["-E", "-P"],
    );
    let (header_consts, header_fns) = parse_header(&pp);
    let skip = skip_list();
    let mut errors = Vec::new();

    for name in &skip {
        if !header_consts.contains_key(name) && !header_fns.contains_key(name) {
            errors.push(format!("{name}: listed in sdk_skip.txt but not in the SDK"));
        }
    }

    // Constants.
    let mut bound = BTreeSet::new();
    for &(name, ty, _, since) in all_consts().filter(|e| sdk.has(e.3)) {
        bound.insert(name.to_owned());
        let Some(item) = header_consts.get(name) else {
            errors.push(format!("{name}: not in the SDK headers"));
            continue;
        };
        if let Some(header_ty) = &item.ty
            && header_ty != ty
        {
            errors.push(format!("{name}: type {ty}, SDK says {header_ty}"));
        }
        // Enumerators without an availability attribute (on themselves or
        // their enum) take the availability of the functions using them.
        if let Some(header_since) = item.since
            && header_since != since
        {
            errors.push(format!(
                "{name}: available since {since:?}, SDK says {header_since:?}"
            ));
        }
    }
    for name in header_consts.keys() {
        if !bound.contains(name) && !skip.contains(name) {
            errors.push(format!("{name}: in the SDK but not bound"));
        }
    }

    // Functions.
    #[cfg(target_arch = "aarch64")]
    {
        let mut bound = BTreeSet::from([SIMD_SETTER.to_owned()]);
        for f in __FNS.iter().filter(|f| sdk.has(f.since)) {
            bound.insert(f.name.to_owned());
            match header_fns.get(f.name) {
                None => errors.push(format!("{}: not in the SDK headers", f.name)),
                Some(HeaderItem { since: Some(s), .. }) if *s != f.since => errors.push(format!(
                    "{}: available since {:?}, SDK says {s:?}",
                    f.name, f.since
                )),
                Some(_) => {}
            }
        }
        for name in header_fns.keys() {
            if !bound.contains(name) && !skip.contains(name) {
                errors.push(format!("{name}: in the SDK but not bound"));
            }
        }
        assert_eq!(
            header_fns.get(SIMD_SETTER).and_then(|f| f.since),
            Some((11, 0))
        );
    }

    assert!(
        errors.is_empty(),
        "{} mismatches with SDK {:?}:\n{}",
        errors.len(),
        sdk.version,
        errors.join("\n")
    );
    assert!(!header_consts.is_empty() && !header_fns.is_empty());
    eprintln!(
        "SDK {:?}: {} constants and {} functions match",
        sdk.version,
        header_consts.len(),
        header_fns.len()
    );
}

#[test]
fn test_c_type_mapping() {
    assert_eq!(c_type("*mut u64"), "uint64_t *");
    assert_eq!(
        c_type("*const hv_vcpu_sme_state_t"),
        "const hv_vcpu_sme_state_t *"
    );
    assert_eq!(c_type("*mut *mut c_void"), "void * *");
    assert_eq!(c_type("* mut * mut hv_vcpu_exit_t"), "hv_vcpu_exit_t * *");
    assert_eq!(c_type("usize"), "size_t");
    assert_eq!(c_type("hv_return_t"), "hv_return_t");
    assert_eq!(parse_version("27.0"), (27, 0));
    assert_eq!(parse_version("15.2\n"), (15, 2));
    assert_eq!(parse_version("26"), (26, 0));
}

#[test]
fn test_parse_header() {
    let pp = "enum {\n HV_A = 0,\n HV_B = (1 << 2) | 1,\n};\ntypedef int hv_x_t;\n\
              __attribute__((availability(macos,introduced=15.0)))\n\
              typedef enum : uint16_t { HV_C = 1, HV_D __attribute__((availability(macos,introduced=27.0))) = 2, } hv_e_t;\n\
              typedef enum : uint32_t { HV_F, } hv_f_t __attribute__((availability(macos,introduced=15.2)));\n\
              extern __attribute__((availability(macos,introduced=13.0))) hv_return_t hv_g(hv_f_t f, uint64_t *v);\n\
              typedef struct { hv_x_t x; } hv_s_t;\n\
              #pragma clang assume_nonnull begin extern hv_return_t hv_h(void);\n";
    let (consts, fns) = parse_header(pp);
    let c = |n: &str| (consts[n].ty.as_deref(), consts[n].since);
    assert_eq!(consts.len(), 5);
    assert_eq!(c("HV_A"), (None, None));
    assert_eq!(c("HV_B"), (None, None));
    assert_eq!(c("HV_C"), (Some("hv_e_t"), Some((15, 0))));
    assert_eq!(c("HV_D"), (Some("hv_e_t"), Some((27, 0))));
    assert_eq!(c("HV_F"), (Some("hv_f_t"), Some((15, 2))));
    assert_eq!(fns.keys().collect::<Vec<_>>(), ["hv_g", "hv_h"]);
    assert_eq!(fns["hv_g"].since, Some((13, 0)));
    assert_eq!(fns["hv_h"].since, None);
}
