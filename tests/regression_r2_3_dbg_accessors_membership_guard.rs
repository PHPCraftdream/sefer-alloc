//! R2-3 regression: the `dbg_*` segment-header accessors reject a foreign
//! segment via a RELEASE-surviving membership guard.
//!
//! `dbg_segment_id_of` / `dbg_stamp_segment_id` / `dbg_kind_byte_of` /
//! `dbg_stamp_kind_byte` / `dbg_kind_at_tag` / `dbg_large_size_of` are
//! `#[doc(hidden)] pub` accessors that derive a segment base from an arbitrary
//! `*mut u8` and read/write the header at that base. They carried only a
//! `debug_assert!(self.table.contains_base_ro(base))` guard (commit 68f323b,
//! L-9b), which COMPILES OUT in release builds — so in a release build the raw
//! header access was unguarded (round2 finding R2-3 / cleanup#2).
//!
//! ## The fix
//!
//! The `debug_assert!` is now a release-surviving `assert!`, so a foreign
//! pointer — one whose segment is NOT registered in THIS `AllocCore`'s table —
//! panics in every build, before any raw header read/write. This module is
//! `#![forbid(unsafe_code)]`, so the `heap_registry`-style `unsafe fn`
//! discipline (T1, commit ce887e5) cannot apply — a real runtime guard is the
//! soundness fix, and it is strictly stronger than `unsafe fn` here: it
//! ACTIVELY rejects a foreign pointer rather than merely marking the call
//! `unsafe`.
//!
//! ## RED→GREEN
//!
//! In a RELEASE build: before the fix `debug_assert!` compiled out, so
//! `ac1.dbg_segment_id_of(ac2_ptr)` returned a value (ac2's segment is mapped,
//! so the raw read did not crash) — RED (the guard absent). After the fix the
//! `assert!` panics — GREEN. In a DEBUG build both old and new panic, so the
//! distinction is release-only — run with `--release` to observe RED→GREEN.
//!
//! Only the four READERS are exercised on a foreign pointer (the two writers
//! `dbg_stamp_segment_id` / `dbg_stamp_kind_byte` would corrupt `ac2`'s header
//! on the pre-fix release path; they share the identical `assert!` guard, so
//! the readers are representative).

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

/// Every reading `dbg_*` accessor panics when given a pointer whose segment is
/// owned by a DIFFERENT `AllocCore` (foreign base not in this core's table).
#[test]
fn dbg_accessors_reject_foreign_segment() {
    let mut ac1 = AllocCore::new().expect("primordial ac1");
    let mut ac2 = AllocCore::new().expect("primordial ac2");

    let layout = Layout::from_size_align(64, 8).unwrap();
    let ac1_ptr = ac1.alloc(layout);
    let ac2_ptr = ac2.alloc(layout);
    assert!(!ac1_ptr.is_null());
    assert!(!ac2_ptr.is_null());

    // Precondition: ac1 owns its own segment but NOT ac2's (disjoint tables).
    assert!(
        ac1.dbg_contains_base(ac1_ptr),
        "ac1 must own ac1_ptr's segment"
    );
    assert!(
        !ac1.dbg_contains_base(ac2_ptr),
        "ac1 must NOT own ac2_ptr's segment (foreign — the guard must fire)"
    );

    // Each reading accessor must panic on the foreign pointer. (The two writers
    // `dbg_stamp_segment_id` / `dbg_stamp_kind_byte` share the identical `assert!`
    // guard, so the four readers are representative — exercising the writers on a
    // foreign pointer would corrupt `ac2`'s header on the pre-fix release path.)
    macro_rules! expect_panic {
        ($name:literal, $body:expr) => {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body));
            assert!(
                r.is_err(),
                concat!(
                    $name,
                    " must panic on a foreign segment (R2-3 release guard); got {r:?}"
                ),
                r = r
            );
        };
    }
    expect_panic!("dbg_segment_id_of", {
        let _ = ac1.dbg_segment_id_of(ac2_ptr);
    });
    expect_panic!("dbg_kind_byte_of", {
        let _ = ac1.dbg_kind_byte_of(ac2_ptr);
    });
    expect_panic!("dbg_kind_at_tag", {
        let _ = ac1.dbg_kind_at_tag(ac2_ptr);
    });
    expect_panic!("dbg_large_size_of", {
        let _ = ac1.dbg_large_size_of(ac2_ptr);
    });

    // Non-regression: the OWN pointer is accepted (guard must not reject a
    // legitimately-owned segment).
    let _ = ac1.dbg_segment_id_of(ac1_ptr);
    let _ = ac1.dbg_kind_byte_of(ac1_ptr);
    let _ = ac1.dbg_kind_at_tag(ac1_ptr);
    let _ = ac1.dbg_large_size_of(ac1_ptr);

    ac1.dealloc(ac1_ptr, layout);
    ac2.dealloc(ac2_ptr, layout);
}
