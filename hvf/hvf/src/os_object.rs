// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Owner of a framework `OS_OBJECT` reference.

use std::fmt;
use std::marker::PhantomData;
use std::ptr::NonNull;

/// An owned reference to an `os_object` of type `T` (for example
/// `hv_vcpu_config_s`), released with `os_release` on drop.
///
/// It is neither `Send` nor `Sync`, so no reasoning about the thread safety of
/// the framework objects is needed.
#[allow(dead_code)]
pub(crate) struct OsObject<T> {
    ptr: NonNull<T>,
    _not_send_sync: PhantomData<*mut ()>,
}

#[allow(dead_code)]
impl<T> OsObject<T> {
    /// Takes ownership of `ptr`, or returns `None` if it is null.
    ///
    /// # Safety
    ///
    /// A non-null `ptr` must be an `os_object` reference the caller owns (for
    /// example the result of a `*_create()` function), not used for anything
    /// but the returned value afterwards.
    pub(crate) unsafe fn from_raw(_ptr: *mut T) -> Option<Self> {
        todo!()
    }

    /// The raw object pointer, valid while `self` is alive.
    pub(crate) fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T> fmt::Debug for OsObject<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OsObject").field(&self.ptr).finish()
    }
}
