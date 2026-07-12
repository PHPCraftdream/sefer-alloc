//! R2-3 regression: `RemoteFreeRing::over_test_buffer` / `init_test_buffer`
//! reject null / misaligned bases via a RELEASE-surviving guard.
//!
//! Both surfaces are `#[doc(hidden)] pub` and accept an arbitrary `*mut u8`.
//! Before this fix they validated NOTHING — a downstream safe caller could pass
//! a null or misaligned base and the ring would construct a view (and, for
//! `init_test_buffer`, write the cursor/slot bytes) over it with no `unsafe`
//! keyword (round2 finding R2-3 / cleanup#2).
//!
//! ## Why a runtime guard (not `unsafe fn`)
//!
//! This module is `#![forbid(unsafe_code)]` (it is NOT a named seam like
//! `heap_registry`, where T1 could use `pub unsafe fn`). The T1 pattern
//! therefore cannot apply here; a release-surviving `assert!` on the documented
//! null + 4-byte-alignment preconditions is the soundness fix. The
//! `FOOTPRINT`-writability half of the contract is not runtime-checkable and
//! stays the caller's responsibility (documented on the functions), exactly as
//! for the `Node` seam primitives the ring delegates to.
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
        let _ = RemoteFreeRing::over_test_buffer(bogus);
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
        let _ = RemoteFreeRing::over_test_buffer(core::ptr::null_mut());
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
        RemoteFreeRing::init_test_buffer(core::ptr::null_mut());
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
    RemoteFreeRing::init_test_buffer(base);
    let _ring = RemoteFreeRing::over_test_buffer(base);
    // No assertion beyond not panicking: both calls accepted the valid buffer.
    let _ = &mut buf; // keep `buf` alive past the view construction
}
