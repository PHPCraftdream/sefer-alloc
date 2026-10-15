//! R6-T22 (finding R6-MS-1) — foreign address sharing the 4 MiB key window
//! must be rejected BEFORE any pointer arithmetic on the segment base.
//!
//! ## The mechanism
//!
//! `AllocCore` routes caller addresses to segments via `os::segment_base_of_ptr`,
//! which extracts a SEGMENT-aligned (4 MiB) key from the address. Under
//! `exact-span-large` (and biased Large configurations), a Large segment's
//! physical reservation is only `round_up(header + size, PAGE)` — typically a
//! few hundred KiB — while the key window it owns in the address table is a
//! full 4 MiB wide. Any address in the same 4 MiB window resolves to the same
//! registered `base`, even though the bytes past the reservation were never
//! mapped by this allocator.
//!
//! ## The defect
//!
//! `canonical_block_of` (and the `realloc_inplace_fast_path_known_base`
//! fast path) computed `ptr.addr() - base.addr()` and handed the offset to
//! `Node::deref` — which performs `base.add(offset)` — BEFORE any span check.
//! A foreign address inside the key window but past the reservation therefore
//! formed `base + off`, an out-of-bounds pointer on a live allocation. Under
//! Miri's `-Zmiri-strict-provenance` this is formal UB (out-of-bounds
//! pointer arithmetic in `compute_offset`), even though the value is never
//! dereferenced before the later membership/offset checks reject it.
//!
//! ## The fix
//!
//! `canonical_block_of` now bounds the offset against the segment's actual
//! usable span (`span_usable_at` for Large, `os::SEGMENT` otherwise) and
//! returns `None` BEFORE calling `Node::deref`. `Node::deref`'s `add`
//! precondition ("offset < segment length") is thereby really established by
//! every caller.
//!
//! ## Counterfactual (RED on the old code)
//!
//! With the offset-bound check reverted, `Node::deref` executes
//! `base.add(SEGMENT - PAGE)` (~4 MiB past a SMALL_MAX-bounded reservation). Miri reports
//! UB: `pointer arithmetic`/`compute_offset` out of bounds of the allocation,
//! and this test fails. GREEN: both `realloc` and `dealloc` on the foreign
//! address are clean rejects (`null` return / silent no-op) with no UB, and
//! the control `dealloc` of the REAL pointer still succeeds (proving the
//! reject path did not consume the live block's free credit).

#![cfg(all(
    feature = "exact-span-large",
    feature = "alloc-core",
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

const SEGMENT: usize = SegmentLayout::SEGMENT;
const PAGE: usize = SegmentLayout::PAGE;

/// A size that classifies as Large under EVERY feature combination: the
/// small-class table's `SMALL_MAX` varies by feature (`medium-classes` ->
/// 1 MiB, `medium-classes-wide` -> 1.75 MiB, both included in
/// `--all-features`), so a hardcoded KiB literal silently stops being Large
/// once it grows past it (see the `just_above_small_max` precedent in
/// `tests/exact_span_large.rs:39-50`). `SMALL_MAX + PAGE` is always above the
/// last small class, hence `class_for(..) == None` -> a genuine Large request.
fn large_size() -> usize {
    SegmentLayout::SMALL_MAX + PAGE
}

#[test]
fn foreign_window_key_rejects_without_pointer_arithmetic_ub() {
    let mut ac = AllocCore::new().expect("AllocCore::new");

    // `large_size()`, align 8: a genuine Large request under
    // `exact-span-large` for ANY feature set (far below a 4 MiB SEGMENT).
    let size = large_size();
    let large_layout = Layout::from_size_align(size, 8).unwrap();
    let p = ac.alloc(large_layout);
    assert!(
        !p.is_null(),
        "OOM allocating a Large block under exact-span-large"
    );

    // Non-vacuity: the block really is an exact-span Large segment — its
    // usable span must have shrunk far below the 4 MiB key window it occupies
    // in the address table (otherwise `foreign` below would actually be in
    // bounds and the test proves nothing).
    let span_usable = ac.dbg_span_usable_of(p);
    assert!(
        span_usable < SEGMENT,
        "Large alloc's usable span ({span_usable}) must be below one \
         SEGMENT ({SEGMENT}) under exact-span-large for this test to be meaningful"
    );

    // Foreign address: same 4 MiB key window as `p` (the header's payload
    // offset is tiny, so `segment_base_of_ptr` resolves the SAME base), but
    // `SEGMENT - PAGE` past the base — far beyond the exact-span reservation
    // (`span = round_up(hdr + SMALL_MAX + PAGE, PAGE) ≈ SMALL_MAX + 2*PAGE`,
    // at most ~1.8 MiB under `medium-classes-wide`, well below ~4 MiB - PAGE)
    // and therefore unmapped territory. Miri allows
    // `wrapping_add` under `-Zmiri-strict-provenance` (it is the sanctioned
    // witness-address construction, unlike `add` on an allocation's pointer).
    let foreign = p.wrapping_add(SEGMENT - PAGE);
    assert_ne!(foreign, core::ptr::null_mut());

    // realloc with a foreign key: must return null (clean reject), never
    // reach the in-place fast path's `Node::deref`.
    // SAFETY: `foreign` is an INTENTIONALLY foreign address key — the
    // ownership contract is deliberately violated here to exercise the
    // R6-MS-1 reject path (defense-in-depth against UB in the resolver).
    let r = unsafe { ac.realloc(foreign, large_layout, size + PAGE) };
    assert!(
        r.is_null(),
        "realloc on a foreign window-key address must return null"
    );

    // dealloc with a foreign key: must be a silent no-op.
    // SAFETY: same deliberately-foreign address key as above (R6-MS-1
    // scenario); the deallocator is expected to reject it without side
    // effects on the live `p` allocation.
    unsafe { ac.dealloc(foreign, large_layout) };

    // Control: the real pointer must still be live and freeable. If the
    // foreign-key rejects above had accidentally consumed `p`'s free credit,
    // this would be a double-free (caught by Miri / the allocator's own
    // abort-on-double-free defence).
    // SAFETY: `p` is a live Large allocation from this `AllocCore` with
    // exactly this layout, freed exactly once here.
    unsafe { ac.dealloc(p, large_layout) };
}
