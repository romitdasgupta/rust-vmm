// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Owner of a framework `OS_OBJECT` reference.

use std::fmt;
use std::marker::PhantomData;
use std::ptr::NonNull;

use hvf_bindings::os_release;

/// An owned reference to an `os_object` of type `T` (for example
/// `hv_vcpu_config_s`), released with `os_release` on drop.
///
/// It is neither `Send` nor `Sync`, so no reasoning about the thread safety of
/// the framework objects is needed.
pub(crate) struct OsObject<T> {
    ptr: NonNull<T>,
    _not_send_sync: PhantomData<*mut ()>,
}

impl<T> OsObject<T> {
    /// Takes ownership of `ptr`, or returns `None` if it is null.
    ///
    /// # Safety
    ///
    /// A non-null `ptr` must be an `os_object` reference the caller owns (for
    /// example the result of a `*_create()` function), not used for anything
    /// but the returned value afterwards.
    pub(crate) unsafe fn from_raw(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|ptr| OsObject {
            ptr,
            _not_send_sync: PhantomData,
        })
    }

    /// The raw object pointer, valid while `self` is alive.
    pub(crate) fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T> Drop for OsObject<T> {
    fn drop(&mut self) {
        // SAFETY: `from_raw` guarantees that `ptr` is an owned `os_object`
        // reference; it is released exactly once, here.
        unsafe { os_release(self.ptr.as_ptr().cast()) };
    }
}

impl<T> fmt::Debug for OsObject<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OsObject").field(&self.ptr).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use hvf_bindings::{hv_vcpu_config_create, hv_vcpu_config_s};

    #[test]
    fn test_from_raw_null() {
        // SAFETY: a null pointer is never dereferenced or released.
        let obj = unsafe { OsObject::<hv_vcpu_config_s>::from_raw(std::ptr::null_mut()) };
        assert!(obj.is_none());
    }

    #[test]
    fn test_create_and_release() {
        // Creating and releasing configurations needs no entitlement. Doing
        // it repeatedly exercises the `Drop` path.
        for _ in 0..16 {
            // SAFETY: `hv_vcpu_config_create` has no preconditions and returns
            // an owned reference (or null), which `from_raw` takes over.
            let obj = unsafe { OsObject::from_raw(hv_vcpu_config_create()) }
                .expect("hv_vcpu_config_create returned NULL");
            assert!(!obj.as_ptr().is_null());
            assert!(format!("{obj:?}").starts_with("OsObject("));
        }
    }
}
