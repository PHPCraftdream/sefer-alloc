//! R4-2 / code_quality_review #2 — `AllocCore::refill_class` release-count
//! truthfulness.
//!
//! ## The defect
//!
//! `refill_class(class_idx, want, out)` filled `out` via
//! `out.iter_mut().take(want)` and returned `want`. The `out.len() >= want`
//! precondition was checked ONLY by a `debug_assert!`, which compiles to
//! nothing in a release build. In a release build called with
//! `out.len() < want`, the `.take(want)` slice iterator is bounds-safe and
//! silently iterates only `out.len()` slots — but the function still returned
//! `want`. So the caller was told more slots were initialised than were
//! actually written: a lying return value, reachable from any release caller
//! that violates the (debug-only) precondition.
//!
//! ## The fix
//!
//! Clamp `take = want.min(out.len())` and return `take`. The return value now
//! always equals the number of slots actually written, in every build profile.
//! The `debug_assert!` is kept as a contract signal for debug callers.
//!
//! ## Profile-dependent RED nuance
//!
//! The defect cannot be exercised with a literal RED run under the default
//! debug `cargo test`: the `debug_assert!` fires first (which is itself a real
//! bug-catcher — it panics before the lying return is reached). The bug only
//! manifests in `--release`, where `debug_assert!` is compiled out. The
//! boundary regression test below is therefore gated `#[cfg(not(debug_assertions))]`
//! so the default debug suite stays green, and it runs (and would catch the
//! old bug) under `cargo test --release`. The companion invariant test runs in
//! both profiles on the contracted (`want <= out.len()`) range and anchors the
//! SAME invariant — `return_value <= out.len()` — that the fix enforces.
//!
//! Per project convention: tests live in `tests/`, not inline.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

/// Invariant anchor (runs in BOTH debug and release): in the contracted range
/// `want <= out.len()`, the return value never exceeds `out.len()`. This holds
/// for both the old and the fixed code on this range, so it is not a RED→GREEN
/// signal for the specific bug — it anchors the truthfulness invariant the fix
/// universalises to the `want > out.len()` case.
#[test]
fn t_refill_return_never_exceeds_outlen_contracted() {
    let mut core = AllocCore::new().expect("AllocCore::new");
    let c = class_for(&core, 16, 8);

    // `want == out.len()` (the normal, well-contracted call).
    let n = 8;
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n, "want == out.len(): return must equal want");
    assert!(
        got <= out_len(&buf),
        "return ({got}) must never exceed out.len() ({})",
        out_len(&buf)
    );
    for &p in &buf {
        assert!(!p.is_null());
    }

    // `want < out.len()` (caller over-provisioned the buffer).
    let cap = 16;
    let mut buf2 = vec![core::ptr::null_mut::<u8>(); cap];
    let want = 4;
    let got2 = core.refill_class(c, want, &mut buf2);
    assert_eq!(got2, want, "want < out.len(): return must equal want");
    assert!(got2 <= out_len(&buf2));

    // Tidy up so the allocator state is clean.
    let layout = Layout::from_size_align(16, 8).unwrap();
    for &p in buf.iter().chain(buf2.iter().take(want)) {
        core.dealloc(p, layout);
    }
}

fn out_len<T>(s: &[T]) -> usize {
    s.len()
}

/// Literal boundary regression test (release-only). Exercises the exact case
/// the bug lives in: `want > out.len()`. In a debug build the `debug_assert!`
/// precondition would panic here (so this test is `#[cfg(not(debug_assertions))]`
/// to keep the default debug suite green); in `--release` it runs and verifies
/// the fix.
///
/// RED (old code, release): `take(want)` iterates only `out.len()` slots but
/// the function returns `want` (= 8), which is `> out.len()` (= 4) → the
/// `got <= out.len()` assertion fails. GREEN (new code, release):
/// `take = want.min(out.len()) = 4`, the function returns `4 == out.len()` →
/// assertion holds. The return value is provably `<= out.len()` by
/// construction after the fix.
#[cfg(not(debug_assertions))]
#[test]
fn t_refill_return_clamped_when_want_exceeds_outlen() {
    let mut core = AllocCore::new().expect("AllocCore::new");
    let c = class_for(&core, 16, 8);

    let cap = 4;
    let want = 8; // deliberately exceeds the buffer capacity
    debug_assert!(cap < want, "test setup invariant");
    let mut buf = vec![core::ptr::null_mut::<u8>(); cap];

    let got = core.refill_class(c, want, &mut buf);

    // The fix's core guarantee: the return value can never exceed the number
    // of slots actually writable. Before the fix this was `want` (8) > 4.
    assert!(
        got <= out_len(&buf),
        "refill_class return ({got}) must never exceed out.len() ({}) \
         even when want ({want}) > out.len()",
        out_len(&buf),
    );
    // After the clamp, exactly `cap` slots are written (no OOM for a 16B class
    // with a 4-slot pull).
    assert_eq!(got, cap, "clamped return must equal min(want, out.len())");
    for &p in &buf {
        assert!(!p.is_null(), "every written slot must be non-null");
    }

    // The slots beyond the buffer were never touched (impossible by slice
    // bounds); clean up exactly the `got` written pointers.
    let layout = Layout::from_size_align(16, 8).unwrap();
    for &p in buf.iter().take(got) {
        core.dealloc(p, layout);
    }
}
