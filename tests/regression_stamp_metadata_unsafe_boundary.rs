//! Regression (R6-CQ-2, round5 `code_quality_review` #2, CRITICAL): the
//! doc-hidden `AllocCore::dbg_stamp_segment_id` / `dbg_stamp_kind_byte` unsafe
//! boundary.
//!
//! Both hooks deliberately overwrite load-bearing allocator metadata (the
//! segment's stamped `segment_id` / the raw `kind` discriminant byte) with
//! arbitrary caller-controlled values. `#[doc(hidden)]` only hides them from
//! generated docs — it does NOT restrict Rust reachability — so as *safe*
//! `pub fn`s fully-safe Rust could stamp a value inconsistent with the
//! segment's true field and then drive a later safe `dealloc`/`Drop` that
//! mis-routes the segment: a `Large` segment whose `kind` byte was stamped to
//! the `Small` discriminant gets freed down the `Small` path, writing a
//! `BinTable`/free-list header into the live `Large` payload; a `segment_id`
//! stamped to another live segment's id corrupts `SegmentTable`'s O(1) slot
//! lookup for BOTH segments.
//!
//! This is the same "writes raw / load-bearing metadata" class that made
//! `dbg_unregister`/`dbg_recycle` (task #101 / R4-MS-3) and `dbg_push_to_ring`
//! (R6-MS-4) `unsafe fn` in this crate — these two hooks were simply missed in
//! that pass (the classification drift is noted in
//! `src/alloc_core/alloc_core_small_diag.rs`, whose own `unsafe fn`
//! `dbg_corrupt_freelist_head_next` cites these two as the field-corruption
//! pattern it mirrors).
//!
//! Both hooks are now `pub unsafe fn` with a `# Safety` contract naming the
//! restore-before-further-use / teardown-via-test-seam obligation. This module
//! pins the compile boundary and proves the honoring restore path still works.
//!
//! ## Counterfactual (RED without the fix)
//!
//! Pre-fix, calling either hook did not require an `unsafe {}` block. With the
//! signatures now `pub unsafe fn`, removing the `unsafe {}` wrapper at any call
//! site below is a compile error (E0133, "call to unsafe function requires
//! unsafe function or block"). This was confirmed during this task's
//! development by temporarily deleting one wrapper and observing `cargo check`
//! go red on the call site; restoring it went green. (The boundary is also
//! exercised at every real caller — `tests/segment_table_o1.rs`,
//! `tests/segment_table_recycle.rs`, `tests/kind_at_strict_decode.rs` — each
//! now wrapped in `unsafe {}` with a per-site `// SAFETY:` comment.)

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

/// `dbg_stamp_segment_id`'s `unsafe fn` boundary is load-bearing: the call
/// site below compiles only inside `unsafe {}`. The honoring restore path
/// (stamp a value the segment does not own, then RESTORE the true id before
/// any routing `dealloc`) also proves the boundary change did not break the
/// legitimate test-only field-corruption pattern.
#[test]
fn dbg_stamp_segment_id_is_unsafe_fn_boundary_and_restore_honoring_path_works() {
    let mut ac = AllocCore::new().expect("primordial");
    let large = Layout::from_size_align(512 * 1024, 8).unwrap();
    let p = ac.alloc(large);
    assert!(!p.is_null(), "large alloc failed");

    let true_id = ac.dbg_segment_id_of(p);

    // Stamp a value the segment does NOT own. The `unsafe` wrapper is
    // load-bearing: with the signature `pub unsafe fn`, removing this wrapper
    // is a compile error (E0133), verified during development.
    //
    // SAFETY (R6-CQ-2): `p` is a live allocation owned by `ac`. The stamped
    // `true_id.wrapping_add(1)` is RESTORED to `p`'s true `true_id` below
    // before any routing allocator operation (`dealloc`) touches `p`; between
    // this stamp and that restore NO `alloc`/`dealloc`/`realloc`/`Drop` routes
    // on the stamped value (nothing reads it). Restore-before-further-use per
    // the `# Safety` contract.
    unsafe { ac.dbg_stamp_segment_id(p, true_id.wrapping_add(1)) };

    // Restore the true id, then free through the public `dealloc` — proving the
    // honoring restore path leaves the allocator consistent/usable.
    //
    // SAFETY (R6-CQ-2): `p` is a live allocation owned by `ac`; `true_id` is
    // `p`'s true segment id (captured above), so this stamp RESTORES the field
    // to its correct value before the `dealloc` below.
    unsafe { ac.dbg_stamp_segment_id(p, true_id) };

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `p` is a live
    // allocation made with the matching layout, freed exactly once here.
    unsafe { ac.dealloc(p, large) };

    assert!(
        !ac.dbg_contains_base(p),
        "restored id + dealloc must unregister the segment"
    );
}

/// `dbg_stamp_kind_byte`'s `unsafe fn` boundary is load-bearing: the call
/// site below compiles only inside `unsafe {}`. The honoring restore path
/// (stamp an out-of-range byte, read it back, then RESTORE the true
/// discriminant before `dealloc`) also proves the legitimate strict-decode
/// test pattern still works.
#[test]
fn dbg_stamp_kind_byte_is_unsafe_fn_boundary_and_restore_honoring_path_works() {
    let mut ac = AllocCore::new().expect("primordial");
    let large = Layout::from_size_align(512 * 1024, 8).unwrap();
    let p = ac.alloc(large);
    assert!(!p.is_null(), "large alloc failed");
    assert_eq!(
        ac.dbg_kind_at_tag(p),
        2,
        "precondition: a 512 KiB alloc must be a Large segment"
    );

    let true_byte = ac.dbg_kind_byte_of(p);

    // Stamp an out-of-range byte (decodes to `Unknown`). The `unsafe` wrapper
    // is load-bearing: with the signature `pub unsafe fn`, removing this
    // wrapper is a compile error (E0133), verified during development.
    //
    // SAFETY (R6-CQ-2): `p` is a live allocation owned by `ac`. While the byte
    // holds `0x99`, only the READ-ONLY `dbg_kind_at_tag` accessor below touches
    // the segment — no routing `alloc`/`dealloc`/`realloc`/`Drop` runs. The
    // byte is RESTORED to the true `Large` discriminant below before `dealloc`.
    // Restore-before-further-use per the `# Safety` contract.
    unsafe { ac.dbg_stamp_kind_byte(p, 0x99) };
    assert_eq!(
        ac.dbg_kind_at_tag(p),
        3,
        "out-of-range byte must decode to Unknown (tag 3), proving the stamp landed"
    );

    // Restore the true discriminant, then free through the public `dealloc`.
    //
    // SAFETY (R6-CQ-2): `p` is a live allocation owned by `ac`; `true_byte` is
    // `p`'s true `kind` byte (captured above), so this stamp RESTORES the byte
    // to its correct value before the `dealloc` below.
    unsafe { ac.dbg_stamp_kind_byte(p, true_byte) };

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `p` is a live
    // allocation made with the matching layout, freed exactly once here.
    unsafe { ac.dealloc(p, large) };

    assert!(
        !ac.dbg_contains_base(p),
        "restored byte + dealloc must unregister the segment"
    );
}
