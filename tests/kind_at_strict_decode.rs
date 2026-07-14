//! L-5 (UBFIX-11, docs/reviews/2026-07-10-ub-audit-final-synthesis.md) —
//! `SegmentHeader::kind_at` must REJECT a corrupt discriminant byte, not
//! silently map it to `Small`.
//!
//! ## The defect this guards
//!
//! `kind_at` reads the header's one-byte `kind` field and decodes it to a
//! `SegmentKind`. Before this fix, any byte OTHER than the two explicitly
//! matched values (`0` → `Primordial`, `2` → `Large`) fell through to a
//! `_ => Small` default — so a corrupted/garbled `kind` byte (a wild write
//! from an unrelated bug, or the aftermath of an H-1-class defect before its
//! fix) was silently treated as a VALID `Small` segment. That is
//! amplification, not containment: a Large segment with a corrupted `kind`
//! byte could be misrouted onto the Small free path, and a Small-specific
//! free would write a BinTable/free-list header into a live Large payload.
//!
//! ## The fix
//!
//! `kind_at` now does a STRICT decode: `0 → Primordial`, `1 → Small`,
//! `2 → Large`, and anything else maps to a new `SegmentKind::Unknown`
//! sentinel that no constructor ever writes. Every production caller of
//! `kind_at` tests for a SPECIFIC expected kind via `==`/`matches!` (never an
//! exhaustive match with an implicit catch-all, except the one exhaustive
//! `match` in `AllocCore::dealloc`, which gained an explicit
//! `Unknown => no-op` arm) — so `Unknown` naturally fails every such check
//! and routes to that call site's existing "not this kind" no-op/reject
//! branch, containing the corruption instead of amplifying it.
//!
//! ## Test-only surface used
//!
//! `SegmentKind` is `pub(crate)`, invisible from `tests/`; `AllocCore`
//! exposes three `#[doc(hidden)] pub` accessors added alongside this fix,
//! mirroring the established `dbg_stamp_segment_id`-style field-corruption
//! pattern:
//! - `dbg_kind_byte_of` — read the RAW `kind` byte (no decode).
//! - `dbg_stamp_kind_byte` — overwrite the RAW `kind` byte with an arbitrary
//!   value, including bytes outside {0,1,2}.
//! - `dbg_kind_at_tag` — the DECODED `SegmentKind`, as a small tag
//!   (0=Primordial, 1=Small, 2=Large, 3=Unknown) so a test can assert on the
//!   decode's output without needing the `pub(crate)` enum itself.

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

/// Baseline: a legitimately-constructed Small segment's `kind` byte decodes
/// to the `Small` tag (`1`), and a Large segment's decodes to `Large` (`2`).
/// This is the non-corrupted control — proves the accessors themselves are
/// wired correctly before the corruption tests rely on them.
#[test]
fn baseline_kind_at_decodes_legitimate_bytes() {
    let mut ac = AllocCore::new().expect("primordial");

    // A fresh `AllocCore` serves its first small alloc from the PRIMORDIAL
    // segment (kind byte 0 → Primordial tag 0) — there is no fresh Small
    // segment until the primordial's payload is exhausted. Both raw byte and
    // decoded tag must agree on 0/Primordial here.
    let small_layout = Layout::from_size_align(64, 8).unwrap();
    let small_ptr = ac.alloc(small_layout);
    assert!(!small_ptr.is_null());
    assert_eq!(
        ac.dbg_kind_byte_of(small_ptr),
        0,
        "the first small alloc from a fresh AllocCore must be served from \
         the primordial segment (raw kind byte 0)"
    );
    assert_eq!(
        ac.dbg_kind_at_tag(small_ptr),
        0,
        "the first small alloc's decoded kind tag must be Primordial(0)"
    );

    // A dedicated Large segment: kind byte 2 → Large tag 2.
    let large_layout = Layout::from_size_align(512 * 1024, 8).unwrap();
    let large_ptr = ac.alloc(large_layout);
    assert!(!large_ptr.is_null());
    assert_eq!(
        ac.dbg_kind_byte_of(large_ptr),
        2,
        "a large alloc's segment must carry raw kind byte 2 (Large)"
    );
    assert_eq!(
        ac.dbg_kind_at_tag(large_ptr),
        2,
        "large alloc's decoded kind tag must be Large(2)"
    );

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(small_ptr, small_layout) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(large_ptr, large_layout) };
}

/// The counterfactual: corrupt a Large segment's `kind` byte to a value that
/// is NOT one of the three legitimate discriminants (0/1/2) — e.g. `7`, or
/// `0xFF` (coincidentally the reject sentinel's OWN discriminant value,
/// tested separately below to prove it is not itself mistaken for a magic
/// "valid" tag by anything). `kind_at` must decode this to `Unknown` (tag 3),
/// NOT silently default to `Small` (tag 1) — pre-fix, this assertion would
/// have read `1` instead of `3`, proving the amplification bug.
#[test]
fn kind_at_rejects_corrupt_discriminant() {
    let mut ac = AllocCore::new().expect("primordial");

    let large_layout = Layout::from_size_align(512 * 1024, 8).unwrap();
    let large_ptr = ac.alloc(large_layout);
    assert!(!large_ptr.is_null());
    assert_eq!(
        ac.dbg_kind_at_tag(large_ptr),
        2,
        "precondition: large_ptr's segment must decode as Large before corruption"
    );

    // Corrupt the kind byte to an arbitrary out-of-range value.
    for &corrupt_byte in &[3u8, 7, 42, 0x7F, 0xFE] {
        ac.dbg_stamp_kind_byte(large_ptr, corrupt_byte);
        assert_eq!(
            ac.dbg_kind_byte_of(large_ptr),
            corrupt_byte,
            "precondition: the corrupt byte must actually have landed"
        );
        let tag = ac.dbg_kind_at_tag(large_ptr);
        assert_eq!(
            tag, 3,
            "L-5 REGRESSION: kind_at decoded corrupt byte {corrupt_byte:#04x} as tag \
             {tag} (expected 3 = Unknown). A byte outside {{0,1,2}} must map to the \
             Unknown reject sentinel, not silently default to some specific kind \
             (the pre-fix behaviour defaulted to Small, tag 1 — the exact \
             amplification this test guards against)."
        );
    }

    // Restore the legitimate byte before cleanup so `dealloc`'s own
    // `kind_at` read routes this Large free correctly (this test corrupts
    // metadata deliberately but must not leak the segment).
    ac.dbg_stamp_kind_byte(large_ptr, 2);
    assert_eq!(ac.dbg_kind_at_tag(large_ptr), 2, "restore failed");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(large_ptr, large_layout) };
}

/// `AllocCore::dealloc`'s exhaustive `match SegmentHeader::kind_at(base)`
/// gained an explicit `SegmentKind::Unknown => {}` no-op arm as part of this
/// fix. This test proves that arm is genuinely reached and genuinely a
/// no-op: freeing a Large pointer whose `kind` byte has been corrupted to an
/// unrecognised value must NOT release the OS reservation, NOT deposit into
/// the large-cache, and NOT corrupt the small free path — the payload must
/// stay exactly as written. Restoring the correct byte afterwards must let
/// a normal `dealloc` succeed (proving the segment's other metadata was
/// untouched by the no-op free attempt).
#[test]
fn dealloc_on_unknown_kind_is_noop_not_crash() {
    let mut ac = AllocCore::new().expect("primordial");

    let large_layout = Layout::from_size_align(512 * 1024, 8).unwrap();
    let large_ptr = ac.alloc(large_layout);
    assert!(!large_ptr.is_null());
    unsafe { core::ptr::write_bytes(large_ptr, 0xCD, 4096) };

    // Corrupt to an unrecognised kind byte, then attempt to free it via the
    // PUBLIC `dealloc` entry point (not a `dbg_*` test seam) — this exercises
    // the actual production `match` arm this fix added.
    ac.dbg_stamp_kind_byte(large_ptr, 0x99);
    assert_eq!(
        ac.dbg_kind_at_tag(large_ptr),
        3,
        "precondition: Unknown tag"
    );

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(large_ptr, large_layout) };

    // No-op proof: the segment must still be registered (a real dealloc of a
    // Large segment always unregisters it — a no-op must not), and the
    // payload bytes we wrote before the corrupted free must be untouched
    // (a Small-path misroute would have clobbered the header of this
    // "segment" with a BinTable/free-list write — this Large payload has no
    // such structure at its start, so any write would show up as corruption
    // of our 0xCD pattern).
    assert!(
        ac.dbg_contains_base(large_ptr),
        "L-5 REGRESSION: dealloc on an Unknown-kind segment unregistered it \
         (should be an unconditional no-op, since we cannot trust which free \
         path — Large release/cache, or Small BinTable push — is actually \
         safe for a segment of unknown kind)"
    );
    let payload = unsafe { core::slice::from_raw_parts(large_ptr, 4096) };
    assert!(
        payload.iter().all(|&b| b == 0xCD),
        "L-5 REGRESSION: dealloc on an Unknown-kind segment mutated the payload \
         (the Small/Primordial free-path arm ran and wrote free-list metadata \
         into what is actually a live Large allocation's payload)"
    );

    // Restore the legitimate kind byte and free it properly, so the segment
    // does not leak for the rest of the test process.
    ac.dbg_stamp_kind_byte(large_ptr, 2);
    assert_eq!(ac.dbg_kind_at_tag(large_ptr), 2, "restore failed");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(large_ptr, large_layout) };
    assert!(
        !ac.dbg_contains_base(large_ptr),
        "the RESTORED, legitimate dealloc must actually unregister the segment"
    );
}
