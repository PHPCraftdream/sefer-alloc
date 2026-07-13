//! R2-3 regression: `RemoteFreeRing::over_test_buffer` / `init_test_buffer`
//! reject null / misaligned bases via a RELEASE-surviving guard.
//!
//! Both surfaces are `#[doc(hidden)] pub unsafe fn` (task #101 / R4-MS-3) and
//! accept an arbitrary `*mut u8`. Before the R2-3 fix they validated NOTHING —
//! a downstream safe caller could pass a null or misaligned base and the ring
//! would construct a view (and, for `init_test_buffer`, write the cursor/slot
//! bytes) over it. The R2-3 release-surviving `assert!` on null +
//! 4-byte-alignment was the first layer; task #101 added the `unsafe fn`
//! boundary so the unverifiable validity/size/lifetime contract lives in the
//! signature, not in prose.
//!
//! ## RED→GREEN
//!
//! Before the fix the functions had no guard, so a misaligned base returned a
//! view (the exploit path open) — RED. After the fix the `assert!` panics —
//! GREEN. The guard is always-on (`assert!`, not `debug_assert!`), so this
//! distinguishes in BOTH debug and release (unlike the `debug_assert!`→
//! `assert!` upgrades used for the dbg_*/gen_*/RunStack surfaces, which are
//! only release-distinguishable because `debug_assert!` already panicked in
//! debug).

#![cfg(feature = "alloc-xthread")]

use sefer_alloc::alloc_core::remote_free_ring::{RemoteFreeRing, FOOTPRINT};

/// `over_test_buffer` rejects a non-null, misaligned base (the common misuse).
#[test]
fn over_test_buffer_rejects_misaligned_base() {
    // 0x1001: non-null, NOT 4-byte-aligned. Before the fix this returned a ring
    // view over a bogus base with no validation at all.
    let bogus = 0x1001usize as *mut u8;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: intentionally passing an invalid base to exercise the guard;
        // the release `assert!` fires before any memory access.
        unsafe {
            let _ = RemoteFreeRing::over_test_buffer(bogus);
        }
    }));
    assert!(
        r.is_err(),
        "over_test_buffer must panic on a misaligned base (R2-3); got {r:?}"
    );
}

/// `over_test_buffer` rejects a null base.
#[test]
fn over_test_buffer_rejects_null_base() {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: intentionally passing a null base to exercise the guard; the
        // release `assert!` fires before any memory access.
        unsafe {
            let _ = RemoteFreeRing::over_test_buffer(core::ptr::null_mut());
        }
    }));
    assert!(
        r.is_err(),
        "over_test_buffer must panic on a null base (R2-3)"
    );
}

/// `init_test_buffer` rejects a null base (it carries the same guard).
#[test]
fn init_test_buffer_rejects_null_base() {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: intentionally passing a null base to exercise the guard; the
        // release `assert!` fires before any memory access.
        unsafe {
            RemoteFreeRing::init_test_buffer(core::ptr::null_mut());
        }
    }));
    assert!(
        r.is_err(),
        "init_test_buffer must panic on a null base (R2-3)"
    );
}

/// Non-regression: a VALID `FOOTPRINT`-sized, 4-byte-aligned buffer is accepted
/// by both surfaces. This mirrors `tests/remote_ring_unit.rs`'s `ring_buffer()`
/// setup, which already asserts the ring's own 4-byte-alignment invariant
/// externally — the new in-function `assert!` is the release-surviving twin of
/// that external check, so a documented-use buffer must still pass.
#[test]
fn valid_aligned_buffer_is_accepted() {
    let mut buf = vec![0u8; FOOTPRINT].into_boxed_slice();
    let base = buf.as_mut_ptr();
    // The System allocator aligns this to >= word size, satisfying the ring's
    // 4-byte requirement (the exact invariant `ring_buffer()` asserts).
    assert!(
        (base as usize).is_multiple_of(4),
        "test buffer must be 4-byte aligned"
    );
    // SAFETY: `base` points to `FOOTPRINT` writable, 4-byte-aligned, exclusively-
    // owned bytes that live for the ring's use (the boxed `buf`).
    unsafe {
        RemoteFreeRing::init_test_buffer(base);
        let _ring = RemoteFreeRing::over_test_buffer(base);
    }
    // No assertion beyond not panicking: both calls accepted the valid buffer.
    let _ = &mut buf; // keep `buf` alive past the view construction
}
