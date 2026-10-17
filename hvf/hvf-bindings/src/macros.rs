// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Declarative macros used to write the bindings.
//!
//! The Hypervisor.framework headers describe their API with two kinds of
//! macros, and this module provides one Rust counterpart for each:
//!
//! * `hv_consts!` mirrors `OS_ENUM(name, type, ...)`: an integer type alias
//!   plus a set of constants of that type. It also records every constant in a
//!   `__CONSTS` table so the SDK check can compare each value with the C
//!   header.
//! * `hv_fns!` mirrors `API_AVAILABLE(macos(X.Y))` function declarations.
//!   Functions available since macOS 11.0 (the first Apple silicon release)
//!   are linked directly. Newer functions are resolved at run time through
//!   `DynApi`, so one binary runs on every macOS release.
//!
//! Both macros are crate-internal. Their expansions are what forms the public
//! API of `hvf-bindings`.

/// Metadata about one function declared through `hv_fns!`.
///
/// This is used by the SDK conformance check to compare every Rust
/// declaration with the C prototype in the SDK headers. It is not meant for
/// general use.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FnSig {
    /// C name of the function.
    pub name: &'static str,
    /// First macOS release (major, minor) that provides the function.
    pub since: (u16, u16),
    /// The Rust return type, as written in the declaration.
    pub ret: &'static str,
    /// The Rust argument types, as written in the declaration.
    pub args: &'static [&'static str],
}

/// One entry of a `__CONSTS` table emitted by `hv_consts!`:
/// `(name, C type, value, since)`.
#[doc(hidden)]
pub type ConstEntry = (&'static str, &'static str, i128, (u16, u16));

/// Converts an optional `(major, minor)` availability to a tuple, defaulting
/// to macOS 11.0.
#[doc(hidden)]
macro_rules! __hv_since {
    () => {
        (11u16, 0u16)
    };
    ($maj:literal, $min:literal) => {
        ($maj as u16, $min as u16)
    };
}

/// Emits one constant, adding an availability note to its documentation when
/// it is newer than macOS 11.0.
#[doc(hidden)]
macro_rules! __hv_const {
    ([$($maj:literal, $min:literal)?] $(#[$m:meta])* $name:ident: $ty:ident = $val:expr) => {
        $(#[$m])*
        #[doc = ""]
        #[doc = concat!("SDK constant `", stringify!($name), "` of type [`", stringify!($ty), "`].")]
        $(
            #[doc = ""]
            #[doc = concat!("Available since macOS ", stringify!($maj), ".", stringify!($min), ".")]
        )?
        pub const $name: $ty = $val;
    };
}

/// Declares integer type aliases and their constants, mirroring the SDK's
/// `OS_ENUM`.
///
/// The body is a sequence of groups. Each group is either
///
/// * `pub type NAME_t = INT { ENTRIES }`, which declares the alias and the
///   constants, or
/// * `consts NAME_t { ENTRIES }`, which adds constants to an alias declared
///   elsewhere.
///
/// `ENTRIES` is a comma separated list of `NAME = VALUE`. Each entry may carry
/// doc comments and, after them, an `@since(MAJOR, MINOR)` marker for
/// constants introduced after macOS 11.0:
///
/// ```ignore
/// hv_consts! {
///     /// Return code of all functions.
///     pub type hv_return_t = i32 {
///         /// Success.
///         HV_SUCCESS = 0,
///         HV_BUSY = 0xfae94002_u32 as i32,
///     }
///     /// Feature register.
///     pub type hv_feature_reg_t = u32 {
///         HV_FEATURE_REG_ID_AA64DFR0_EL1 = 0,
///         @since(27, 0) HV_FEATURE_REG_ID_AA64ISAR2_EL1 = 14,
///     }
/// }
/// ```
///
/// One invocation is allowed per module: besides the items above it emits a
/// hidden `pub const __CONSTS: &[ConstEntry]` listing every constant as
/// `(name, C type name, value, since)`. The C type name is the alias name,
/// which is identical to the SDK typedef.
macro_rules! hv_consts {
    // Group declaring a new alias: emit it, then handle the constants as a
    // `consts` group.
    (@munch [$($acc:tt)*]
        $(#[$tm:meta])*
        pub type $tname:ident = $tty:ty { $($body:tt)* }
        $($rest:tt)*
    ) => {
        $(#[$tm])*
        pub type $tname = $tty;
        hv_consts!(@munch [$($acc)*] consts $tname { $($body)* } $($rest)*);
    };
    // Group adding constants to an alias declared elsewhere.
    (@munch [$($acc:tt)*]
        consts $tname:ident {
            $(
                $(#[$cm:meta])*
                $(@since($cmaj:literal, $cmin:literal))?
                $cname:ident = $cval:expr
            ),* $(,)?
        }
        $($rest:tt)*
    ) => {
        $(
            __hv_const!([$($cmaj, $cmin)?] $(#[$cm])* $cname: $tname = $cval);
        )*
        hv_consts!(@munch [
            $($acc)*
            $(
                (
                    stringify!($cname),
                    stringify!($tname),
                    $cname as i128,
                    __hv_since!($($cmaj, $cmin)?),
                ),
            )*
        ] $($rest)*);
    };
    // All groups consumed: emit the metadata table.
    (@munch [$($acc:tt)*]) => {
        /// Every constant declared in this module, for the SDK check.
        #[doc(hidden)]
        pub const __CONSTS: &[$crate::macros::ConstEntry] = &[$($acc)*];
    };
    ($($body:tt)*) => {
        hv_consts!(@munch [] $($body)*);
    };
}

/// Declares Hypervisor.framework functions, mirroring the SDK's
/// `API_AVAILABLE(macos(X.Y))` declarations.
///
/// Each entry has the form
///
/// ```ignore
/// hv_fns! {
///     /// Creates a VM instance for the current process.
///     (11, 0) fn hv_vm_create(config: hv_vm_config_t) -> hv_return_t;
///     /// Creates a VM configuration object.
///     (13, 0) fn hv_vm_config_create() -> hv_vm_config_t;
/// }
/// ```
///
/// where the leading tuple is the first macOS release providing the function.
/// The invocation emits:
///
/// * for `(11, 0)` entries, a hard-linked declaration in an `unsafe extern
///   "C"` block of the invoking module, linked against `Hypervisor.framework`;
/// * for newer entries, a hard-linked declaration in `pub mod direct`. Using
///   one of these makes the binary fail to load on older macOS releases, so
///   they are only meant for code that already requires that release;
/// * for every entry, a function pointer type in `pub mod pfn` and the
///   availability tuple in `pub mod since`;
/// * `pub struct DynApi`, with one `Option<pfn::NAME>` field per entry newer
///   than macOS 11.0, and `DynApi::load_with` / `DynApi::load` (the latter on
///   macOS/aarch64 only, using `dlsym(RTLD_DEFAULT, ..)`);
/// * a hidden `pub const __FNS: &[FnSig]` listing every entry, for the SDK
///   check.
///
/// One invocation is allowed per module.
// Only invoked by the macOS/aarch64 `functions` module (and the tests).
#[cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(unused_macros)
)]
macro_rules! hv_fns {
    // Entry available since macOS 11.0: hard-linked.
    (@munch [$($base:tt)*] [$($newer:tt)*] [$($all:tt)*]
        $(#[$m:meta])*
        (11, 0) fn $name:ident($($arg:ident: $aty:ty),* $(,)?) -> $ret:ty;
        $($rest:tt)*
    ) => {
        hv_fns!(@munch
            [$($base)* { $(#[$m])* $name($($arg: $aty),*) -> $ret }]
            [$($newer)*]
            [$($all)* { (11, 0) $(#[$m])* $name($($arg: $aty),*) -> $ret }]
            $($rest)*);
    };
    // Entry newer than macOS 11.0: resolved at run time.
    (@munch [$($base:tt)*] [$($newer:tt)*] [$($all:tt)*]
        $(#[$m:meta])*
        ($maj:literal, $min:literal) fn $name:ident($($arg:ident: $aty:ty),* $(,)?) -> $ret:ty;
        $($rest:tt)*
    ) => {
        hv_fns!(@munch
            [$($base)*]
            [$($newer)* { ($maj, $min) $(#[$m])* $name($($arg: $aty),*) -> $ret }]
            [$($all)* { ($maj, $min) $(#[$m])* $name($($arg: $aty),*) -> $ret }]
            $($rest)*);
    };
    // All entries consumed.
    (@munch
        [$({ $(#[$bm:meta])* $bname:ident($($barg:ident: $baty:ty),*) -> $bret:ty })*]
        [$({ ($nmaj:literal, $nmin:literal) $(#[$nm:meta])* $nname:ident($($narg:ident: $naty:ty),*) -> $nret:ty })*]
        [$({ ($amaj:literal, $amin:literal) $(#[$am:meta])* $aname:ident($($aarg:ident: $aaty:ty),*) -> $aret:ty })*]
    ) => {
        #[cfg_attr(target_vendor = "apple", link(name = "Hypervisor", kind = "framework"))]
        unsafe extern "C" {
            $(
                $(#[$bm])*
                #[doc = ""]
                #[doc = concat!("Binding for `", stringify!($bname), "`. Available since macOS 11.0.")]
                pub fn $bname($($barg: $baty),*) -> $bret;
            )*
        }

        /// Hard-linked declarations of functions newer than macOS 11.0.
        ///
        /// Calling any of these makes the final binary refuse to load on macOS
        /// releases older than the one that introduced the function. Prefer
        /// [`DynApi`], which resolves them at run time.
        pub mod direct {
            #[allow(unused_imports)]
            use super::*;

            #[cfg_attr(target_vendor = "apple", link(name = "Hypervisor", kind = "framework"))]
            unsafe extern "C" {
                $(
                    $(#[$nm])*
                    #[doc = ""]
                    #[doc = concat!(
                        "Binding for `", stringify!($nname), "`. Available since macOS ",
                        stringify!($nmaj), ".", stringify!($nmin), "."
                    )]
                    pub fn $nname($($narg: $naty),*) -> $nret;
                )*
            }
        }

        /// Function pointer types, one per declared function.
        #[allow(non_camel_case_types)]
        pub mod pfn {
            #[allow(unused_imports)]
            use super::*;

            $(
                #[doc = concat!(
                    "Pointer to `", stringify!($aname), "` (macOS ",
                    stringify!($amaj), ".", stringify!($amin), ")."
                )]
                pub type $aname = unsafe extern "C" fn($($aaty),*) -> $aret;
            )*
        }

        /// The first macOS release `(major, minor)` providing each function.
        #[allow(non_upper_case_globals)]
        pub mod since {
            $(
                #[doc = concat!("Availability of `", stringify!($aname), "`.")]
                pub const $aname: (u16, u16) = __hv_since!($amaj, $amin);
            )*
        }

        /// Run-time resolved entry points of every function newer than
        /// macOS 11.0.
        ///
        /// A field is `None` when the running system does not provide the
        /// function.
        #[derive(Clone, Copy, Debug, Default)]
        pub struct DynApi {
            $(
                #[doc = concat!(
                    "`", stringify!($nname), "`, available since macOS ",
                    stringify!($nmaj), ".", stringify!($nmin), "."
                )]
                pub $nname: Option<pfn::$nname>,
            )*
        }

        impl DynApi {
            /// Builds the table by calling `resolve` with the C name of each
            /// function. `resolve` returns a null pointer for unavailable
            /// functions.
            ///
            /// # Safety
            ///
            /// Every non-null pointer returned by `resolve` must be the address
            /// of a function with exactly the C prototype of the named
            /// Hypervisor.framework function, that stays valid for as long as
            /// the returned table is used.
            #[allow(unused_variables, unused_mut, clippy::missing_transmute_annotations)]
            pub unsafe fn load_with<F>(mut resolve: F) -> Self
            where
                F: FnMut(&core::ffi::CStr) -> *mut core::ffi::c_void,
            {
                DynApi {
                    $(
                        $nname: {
                            const NAME: &core::ffi::CStr =
                                match core::ffi::CStr::from_bytes_with_nul(
                                    concat!(stringify!($nname), "\0").as_bytes(),
                                ) {
                                    Ok(name) => name,
                                    Err(_) => panic!("function name contains a NUL byte"),
                                };
                            let ptr = resolve(NAME);
                            if ptr.is_null() {
                                None
                            } else {
                                // SAFETY: the caller guarantees that a non-null
                                // pointer is the address of a function with the
                                // C prototype of `NAME`. `pfn::NAME` is generated
                                // from the same tokens as that declaration, and a
                                // data pointer and a function pointer have the
                                // same size and representation on all supported
                                // targets.
                                Some(unsafe {
                                    core::mem::transmute::<*mut core::ffi::c_void, pfn::$nname>(ptr)
                                })
                            }
                        },
                    )*
                }
            }

            /// Resolves every function newer than macOS 11.0 with
            /// `dlsym(RTLD_DEFAULT, name)`.
            ///
            /// The framework itself is always loaded, because the macOS 11.0
            /// functions are hard-linked.
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            pub fn load() -> Self {
                let resolve = |name: &core::ffi::CStr| {
                    // SAFETY: `name` is a valid NUL-terminated string and
                    // `RTLD_DEFAULT` is a valid pseudo-handle.
                    unsafe { $crate::os::dlsym($crate::os::RTLD_DEFAULT, name.as_ptr()) }
                };
                // SAFETY: `dlsym` returns either null or the address of the
                // exported symbol of that name. The exported Hypervisor.framework
                // symbols have the C prototypes the `pfn` types are generated
                // from (this is checked against the SDK headers by the
                // `sdk_check` test), and the framework is never unloaded since
                // it is a hard-linked dependency.
                unsafe { Self::load_with(resolve) }
            }
        }

        /// Every function declared in this module, for the SDK check.
        #[doc(hidden)]
        pub const __FNS: &[$crate::macros::FnSig] = &[
            $(
                $crate::macros::FnSig {
                    name: stringify!($aname),
                    since: __hv_since!($amaj, $amin),
                    ret: stringify!($aret),
                    args: &[$(stringify!($aaty)),*],
                },
            )*
        ];
    };
    ($($body:tt)*) => {
        hv_fns!(@munch [] [] [] $($body)*);
    };
}

#[cfg(test)]
mod tests {
    use super::{ConstEntry, FnSig};

    mod consts {
        hv_consts! {
            /// A test alias.
            pub type test_enum_t = u32 {
                /// First.
                TEST_A = 0,
                TEST_B = 1 << 3,
                /// Newer.
                @since(27, 0)
                TEST_C = 0x20,
            }
            /// A signed alias.
            pub type test_ret_t = i32 {
                TEST_OK = 0,
                TEST_ERR = 0xfae94002_u32 as i32,
            }
            consts test_enum_t {
                @since(15, 2) TEST_D = 0xc000 + 5,
            }
            /// An alias with no constants.
            pub type test_empty_t = u16 {}
        }
    }

    mod empty_consts {
        hv_consts! {}
    }

    #[test]
    fn test_consts_values_and_types() {
        use consts::*;

        let a: test_enum_t = TEST_A;
        let r: test_ret_t = TEST_ERR;
        let e: test_empty_t = 0;
        assert_eq!(a, 0u32);
        assert_eq!(TEST_B, 8u32);
        assert_eq!(TEST_C, 0x20u32);
        assert_eq!(TEST_D, 0xc005u32);
        assert_eq!(TEST_OK, 0i32);
        assert_eq!(r as u32, 0xfae9_4002);
        assert_eq!(e, 0u16);
    }

    #[test]
    fn test_consts_table() {
        let expected: &[ConstEntry] = &[
            ("TEST_A", "test_enum_t", 0, (11, 0)),
            ("TEST_B", "test_enum_t", 8, (11, 0)),
            ("TEST_C", "test_enum_t", 0x20, (27, 0)),
            ("TEST_OK", "test_ret_t", 0, (11, 0)),
            ("TEST_ERR", "test_ret_t", -0x0516_bffe, (11, 0)),
            ("TEST_D", "test_enum_t", 0xc005, (15, 2)),
        ];
        assert_eq!(consts::__CONSTS, expected);
        assert_eq!(consts::TEST_ERR as i128, -0x0516_bffe);
        assert!(empty_consts::__CONSTS.is_empty());
    }

    // Test-only declarations: not all generated items are exercised.
    #[allow(dead_code)]
    mod fns {
        // Use libc symbols that exist on every host for the hard-linked
        // entries, so they can actually be called.
        hv_fns! {
            /// Returns the process id.
            (11, 0) fn getpid() -> i32;
            (11, 0) fn abs(v: i32,) -> i32;
            /// Not a real symbol: never called through `direct`.
            (13, 0) fn hvtest_add(a: u32, b: u32) -> u32;
            (27, 0) fn hvtest_missing(p: *mut u64, q: *const u8) -> i32;
            /// Real libc symbol, resolved dynamically in `test_dyn_load`.
            (15, 2) fn getppid() -> i32;
        }
    }

    #[allow(dead_code)]
    mod no_fns {
        hv_fns! {}
    }

    extern "C" fn add_impl(a: u32, b: u32) -> u32 {
        a.wrapping_add(b)
    }

    #[test]
    fn test_fns_hard_linked() {
        // SAFETY: `getpid` and `abs` are libc functions with these prototypes.
        unsafe {
            assert!(fns::getpid() > 0);
            assert_eq!(fns::abs(-7), 7);
        }
        let f: fns::pfn::getpid = fns::getpid;
        // SAFETY: as above.
        assert_eq!(unsafe { f() }, unsafe { fns::getpid() });
    }

    #[test]
    fn test_fns_table() {
        let expected: &[FnSig] = &[
            FnSig {
                name: "getpid",
                since: (11, 0),
                ret: "i32",
                args: &[],
            },
            FnSig {
                name: "abs",
                since: (11, 0),
                ret: "i32",
                args: &["i32"],
            },
            FnSig {
                name: "hvtest_add",
                since: (13, 0),
                ret: "u32",
                args: &["u32", "u32"],
            },
            FnSig {
                name: "hvtest_missing",
                since: (27, 0),
                ret: "i32",
                args: &["*mut u64", "*const u8"],
            },
            FnSig {
                name: "getppid",
                since: (15, 2),
                ret: "i32",
                args: &[],
            },
        ];
        assert_eq!(fns::__FNS, expected);
        assert!(no_fns::__FNS.is_empty());

        assert_eq!(fns::since::getpid, (11, 0));
        assert_eq!(fns::since::hvtest_add, (13, 0));
        assert_eq!(fns::since::hvtest_missing, (27, 0));
        assert_eq!(fns::since::getppid, (15, 2));
    }

    #[test]
    fn test_dyn_load_with() {
        let mut seen = std::vec::Vec::new();
        // SAFETY: the only non-null pointer returned is `add_impl`, which has
        // the prototype declared for `hvtest_add`.
        let api = unsafe {
            fns::DynApi::load_with(|name| {
                let name = name.to_str().unwrap();
                seen.push(std::string::String::from(name));
                if name == "hvtest_add" {
                    add_impl as *const () as *mut core::ffi::c_void
                } else {
                    core::ptr::null_mut()
                }
            })
        };
        // Only entries newer than macOS 11.0 are resolved, in order.
        assert_eq!(seen, ["hvtest_add", "hvtest_missing", "getppid"]);
        let add = api.hvtest_add.unwrap();
        // SAFETY: `add` points at `add_impl`.
        assert_eq!(unsafe { add(40, 2) }, 42);
        assert!(api.hvtest_missing.is_none());
        assert!(api.getppid.is_none());

        let empty = fns::DynApi::default();
        assert!(empty.hvtest_add.is_none());
        assert!(empty.hvtest_missing.is_none());
        assert!(empty.getppid.is_none());
        // SAFETY: the resolver always returns null.
        let none = unsafe { no_fns::DynApi::load_with(|_| core::ptr::null_mut()) };
        assert_eq!(std::format!("{none:?}"), "DynApi");
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn test_dyn_load_dlsym() {
        let api = fns::DynApi::load();
        assert!(api.hvtest_add.is_none());
        assert!(api.hvtest_missing.is_none());
        let getppid = api.getppid.expect("dlsym(getppid)");
        // SAFETY: `getppid` was resolved from libSystem by name and has the
        // declared prototype.
        assert!(unsafe { getppid() } >= 0);
        assert!(std::format!("{api:?}").contains("getppid"));
    }
}
