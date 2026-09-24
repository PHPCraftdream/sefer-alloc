//! R2-05 (independent src review round 2, task #2007) — miri
//! strict-provenance regression for the `AllocCore` `dbg_*` diagnostic
//! accessors.
//!
//! Before this fix, every accessor below computed `base =
//! os::segment_base_of_ptr(ptr)` (preserving `ptr`'s own provenance via
//! `map_addr`), checked ONLY that `base`'s ADDRESS matched a live segment
//! (`SegmentTable::contains_base_ro`), and then read allocator metadata
//! through `base` itself. Under Rust's strict-provenance model, matching an
//! address does not grant provenance: a pointer built from a DIFFERENT
//! allocation's provenance (or from none at all, via
//! `ptr::without_provenance_mut`) that merely happens to share a live
//! segment's numeric address is not valid to dereference, even though the
//! membership check passes. `SegmentTable::canonical_base_of` fixes this by
//! returning the table's own STORED pointer (the one `register` wrote,
//! carrying the allocator's real provenance) instead of a bool, and every
//! accessor now reads through THAT pointer, never through the caller-derived
//! address.
//!
//! This test constructs exactly that hazard: it allocates a real block,
//! derives a PROVENANCE-LESS pointer with the identical numeric address via
//! `ptr::without_provenance_mut`, and calls each fixed accessor with it.
//! Under `-Zmiri-strict-provenance` (this project's default miri job, see
//! `scripts/miri.mjs`), a pre-fix accessor dereferencing the provenance-less
//! pointer would be rejected as undefined behavior; the fixed accessors
//! must run clean and return the SAME values a call with the real,
//! provenance-carrying pointer would.
//!
//! Deliberately covers only the accessors reachable under `alloc-core
//! internals` alone (no `numa-aware` / `page-map-diag` / `virgin-zero-skip`
//! / lazy-commit feature combinations) — miri is ~1000x slower than native,
//! so this stays a small, fast, focused target per the project's
//! short-scenario miri policy, matching every other entry in
//! `scripts/miri.mjs`'s `MATRIX`. The remaining accessors (`dbg_node_id_for`,
//! `dbg_page_map_class_for`, `dbg_payload_virgin_for`,
//! `dbg_committed_payload_end_for`, `dbg_owner_id_for`,
//! `dbg_directory_bit_for_ptr`) share the IDENTICAL fix
//! (`SegmentTable::canonical_base_of` / the `.find`-not-`.any` pattern) and
//! are covered functionally (not under miri) by
//! `tests/regression_r2_05_diag_provenance.rs`.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

/// Builds a real `AllocCore`, allocates one block, and returns
/// `(core, real_ptr, provenance_less_ptr)` where the latter shares
/// `real_ptr`'s numeric address but carries NO provenance at all
/// (`ptr::without_provenance_mut`) — the exact shape of a caller-derived
/// pointer the review's hazard describes, taken to its extreme (a
/// legitimate-but-foreign pointer with correct provenance over a DIFFERENT
/// allocation is a weaker version of the same hazard; the provenance-less
/// case is what miri's strict-provenance model can mechanically catch).
fn alloc_one_and_derive_provenance_less() -> (AllocCore, *mut u8, *mut u8) {
    let mut core = AllocCore::new().expect("primordial segment reservation");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let real_ptr = core.alloc(layout);
    assert!(!real_ptr.is_null());
    let addr = real_ptr.expose_provenance();
    let stale = core::ptr::without_provenance_mut::<u8>(addr);
    (core, real_ptr, stale)
}

#[test]
fn dbg_kind_byte_of_sound_under_provenance_less_input() {
    let (core, real_ptr, stale) = alloc_one_and_derive_provenance_less();
    assert_eq!(
        core.dbg_kind_byte_of(real_ptr),
        core.dbg_kind_byte_of(stale)
    );
}

#[test]
fn dbg_kind_at_tag_sound_under_provenance_less_input() {
    let (core, real_ptr, stale) = alloc_one_and_derive_provenance_less();
    assert_eq!(core.dbg_kind_at_tag(real_ptr), core.dbg_kind_at_tag(stale));
}

#[test]
fn dbg_large_size_of_sound_under_provenance_less_input() {
    let (core, real_ptr, stale) = alloc_one_and_derive_provenance_less();
    assert_eq!(
        core.dbg_large_size_of(real_ptr),
        core.dbg_large_size_of(stale)
    );
}

#[test]
fn dbg_span_usable_of_sound_under_provenance_less_input() {
    let (core, real_ptr, stale) = alloc_one_and_derive_provenance_less();
    assert_eq!(
        core.dbg_span_usable_of(real_ptr),
        core.dbg_span_usable_of(stale)
    );
}

#[test]
fn dbg_reserved_capacity_of_sound_under_provenance_less_input() {
    let (core, real_ptr, stale) = alloc_one_and_derive_provenance_less();
    assert_eq!(
        core.dbg_reserved_capacity_of(real_ptr),
        core.dbg_reserved_capacity_of(stale)
    );
}

#[test]
fn dbg_segment_id_of_sound_under_provenance_less_input() {
    let (core, real_ptr, stale) = alloc_one_and_derive_provenance_less();
    assert_eq!(
        core.dbg_segment_id_of(real_ptr),
        core.dbg_segment_id_of(stale)
    );
}

#[test]
fn dbg_freelist_head_for_sound_under_provenance_less_input() {
    let (core, real_ptr, stale) = alloc_one_and_derive_provenance_less();
    // Class index is irrelevant here (both calls target the same segment's
    // same class) -- 0 is always in range.
    assert_eq!(
        core.dbg_freelist_head_for(real_ptr, 0),
        core.dbg_freelist_head_for(stale, 0)
    );
}

#[test]
fn dbg_is_free_for_sound_under_provenance_less_input() {
    let (core, real_ptr, stale) = alloc_one_and_derive_provenance_less();
    assert_eq!(core.dbg_is_free_for(real_ptr), core.dbg_is_free_for(stale));
}

/// Out-of-range input: an address that is NOT a live segment at all must be
/// rejected with zero memory access (return the documented "foreign"
/// sentinel), not just "not panic under provenance" -- this is the
/// acceptance criterion's other half.
#[test]
fn dbg_kind_byte_of_panics_cleanly_on_a_genuinely_foreign_address() {
    let core = AllocCore::new().expect("primordial segment reservation");
    // A provenance-less pointer at a fixed, SEGMENT-aligned address no
    // segment could plausibly occupy in this tiny single-allocation test.
    let foreign = core::ptr::without_provenance_mut::<u8>(0x1000_0000);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        core.dbg_kind_byte_of(foreign)
    }));
    assert!(
        result.is_err(),
        "dbg_kind_byte_of must panic (not silently read) on a foreign address"
    );
}
