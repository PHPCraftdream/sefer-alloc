// R31-8 (task #488) — deterministic, native (cross-platform) scan-cost
// isolation microjudge, driving a BARE `AllocCore` directly (not through
// `SeferAlloc`/the per-thread registry — see the "why AllocCore, not
// SeferAlloc" note below), for the base 8-slot large-cache vs the extended
// 8+32=40-slot large-cache.
//
// WHY THIS EXISTS. The real-process narrow A/B
// (`examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`) now
// proves a MATCHED pre-timing state (task #488's own fix) and measures a
// REAL, reproducible ON-slower signal at N=1/2/4 (t well past crit, sign
// 20/20 at every N — see the report). But that harness's timed region still
// includes the full `alloc_large`/`dealloc` dispatch path end to end (best-
// fit scan + header rewrite + registry register/unregister + magic-store),
// not the scan bound in isolation — a real result, but not a DECOMPOSED one.
// This microjudge isolates JUST the best-fit scan
// (`AllocCore::alloc_large`'s `for i in 0..self.large_cache_scan_bound()`
// loop, `src/alloc_core/alloc_core_large.rs:217-227`) at a FIXED, WORST-CASE
// relative target position, so a genuine O(8)-vs-O(40) scan-bound cost (if
// one exists) cannot hide behind — or be confused with — any setup-state
// asymmetry the real-process gate's matched-state proof already closed.
//
// WHY `AllocCore` DIRECTLY, NOT `SeferAlloc`. `AllocCore::new()` (feature
// `alloc-core`, the established `#[doc(hidden)]` test-only export pattern —
// see `tests/alloc_core_batch.rs` for existing precedent) constructs a bare,
// unregistered allocator core with NO TLS heap-claim, NO registry slot, NO
// per-thread bootstrap indirection — every one of those is a REAL cost this
// microjudge deliberately excludes, because the entry-point-honesty rule
// (CLAUDE.md's R31-14b/entry-point rule) requires stating explicitly which
// layer is under test: THIS microjudge tests the `AllocCore::alloc_large`
// scan loop in isolation, NOT the real `#[global_allocator]` chain end to
// end (that is what the real-process A/B in the sibling file already
// covers, at the correct production-shipping layer, per that same rule).
// The two measurements are DELIBERATELY complementary, not substitutes for
// each other — see this report's own text for what each one does and does
// not prove.
//
// WORST-CASE SCAN POSITION DESIGN. `alloc_large`'s best-fit loop
// (`src/alloc_core/alloc_core_large.rs:215-227`) cannot short-circuit on the
// first fitting slot — it must scan every occupied slot up to
// `large_cache_scan_bound()` to find the SMALLEST fitting entry (best-fit,
// not first-fit). To force the FULL scan bound to be walked on every timed
// iteration (the true worst case for a linear scan, and the case most
// favorable to a slot-count-proportional cost actually showing up):
//   1. Populate every slot BUT ONE with a "decoy" entry too SMALL to satisfy
//      the target request (`slot.usable_size < usable`) — best-fit's
//      `usable_size >= usable` guard rejects every decoy outright, so the
//      scan must still visit all of them (no way to know a later slot isn't
//      a better fit without visiting it), but never assigns `hit_idx` to a
//      decoy.
//   2. Populate the LAST slot (fixed index: `LARGE_CACHE_SLOTS - 1` = 7 for
//      the base cache, `LARGE_CACHE_SLOTS + LARGE_CACHE_EXTENDED_SLOTS - 1`
//      = 39 for the extended cache) with the ONE entry that fits the target
//      request exactly. Being scanned LAST means the loop cannot exit early
//      even in a hypothetical future first-fit-with-early-exit
//      implementation — today's best-fit implementation always scans the
//      full bound regardless of position, so this is a conservative,
//      implementation-agnostic worst case.
// Each timed iteration's `alloc()` call takes that one fitting slot (a
// `large_cache_slot_take`), then the IMMEDIATELY FOLLOWING `dealloc()` puts
// an entry of the SAME usable size back — `large_cache_find_free_slot`
// finds the just-vacated LAST index as the (only) free slot (`iter()
// .position(|s| s.is_none())` — the only `None` in the array), so the
// fitting entry is redeposited at the SAME fixed last-index position for
// the next iteration. The occupancy shape is therefore STABLE across every
// iteration of the timed loop by construction — no re-setup needed between
// iterations, and no drift in scan position over the run.

// R31-8 (task #488): this microjudge drives `AllocCore` directly via its
// public `alloc`/`dealloc` API (the same `#[doc(hidden)]` test-only surface
// `tests/alloc_core_batch.rs` already uses) — `dealloc` is `unsafe fn`
// (raw-pointer contract: caller attests the pointer/layout pair was
// produced by a matching `alloc` call and is freed exactly once). Every
// call site below satisfies that contract by construction: `p` is always
// the immediately-preceding `alloc`'s return value, freed with the exact
// same `Layout` used to produce it, exactly once. Each binary that
// `include!`s this file adds its own `#[allow(unsafe_code)]` at the one
// `unsafe { core.dealloc(...) }` block below (an inner `#![allow(...)]`
// cannot be used here — `include!` splices into the enclosing module, not a
// new crate root).

use std::alloc::Layout;
use std::hint::black_box;
use std::time::Instant;

use sefer_alloc::SegmentLayout;

const ALIGN: usize = 8;

fn layout_for(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

fn alloc_one(core: &mut AllocCore, size: usize) -> *mut u8 {
    let p = core.alloc(layout_for(size));
    assert!(!p.is_null(), "alloc({size}) failed -- probe is invalid");
    p
}

#[allow(unsafe_code)] // see this file's module-doc note above
fn dealloc_one(core: &mut AllocCore, p: *mut u8, size: usize) {
    // SAFETY: `p` was returned by the immediately preceding
    // `alloc_one(core, size)` call with the SAME `size`, and is freed here
    // exactly once.
    unsafe { core.dealloc(p, layout_for(size)) };
}

/// One Large size, `SEGMENT`-multiple-rounded, `mult` segments large --
/// mirrors the sibling narrow-AB workload's own segment-multiple derivation
/// so every produced size lands in the dedicated-segment Large path (never
/// Small/Medium), matching the real gate's size regime.
fn large_size(mult: usize) -> usize {
    mult * SegmentLayout::SEGMENT
}

/// Populate `core`'s large cache to the worst-case scan-bound shape
/// described in this file's module doc: `decoy_count` too-small entries
/// (sizes `1..=decoy_count` segments, all strictly smaller than
/// `target_size`) at slots `0..decoy_count`, then the ONE fitting entry
/// (exactly `target_size`) at the fixed LAST slot index
/// (`decoy_count`, since slots are filled in order by
/// `large_cache_find_free_slot`'s first-free-slot scan on an initially
/// empty cache). Returns `target_size` (the caller times repeated
/// `alloc(target_size)`/`dealloc` cycles against it).
///
/// `decoy_count` must equal the cache's OWN scan bound minus 1 (7 for the
/// base 8-slot cache, 39 for the materialised 40-slot extended cache) for
/// the fitting entry to land at the true last index.
fn populate_worst_case(core: &mut AllocCore, decoy_count: usize) -> usize {
    // Decoys: sizes 1..=decoy_count segments (all deposited AND freed in
    // ascending order so they occupy slots 0..decoy_count in that order --
    // `large_cache_find_free_slot` on an empty cache returns the first
    // `None`, i.e. slots fill in allocation order 0, 1, 2, ...).
    for mult in 1..=decoy_count {
        let sz = large_size(mult);
        let p = alloc_one(core, sz);
        dealloc_one(core, p, sz);
    }
    // The ONE fitting entry: strictly larger than every decoy (so best-fit
    // never prefers a decoy -- decoys are excluded by the `usable_size >=
    // usable` guard regardless, but sizing it larger keeps the intent
    // unambiguous), deposited last -> lands at the fixed last slot index
    // (`decoy_count`).
    let target_size = large_size(decoy_count + 1);
    let p = alloc_one(core, target_size);
    dealloc_one(core, p, target_size);
    target_size
}

/// Run `rounds` timed alloc/dealloc cycles of `target_size` against an
/// already-worst-case-populated `core`, returning total elapsed ns. Each
/// cycle: `alloc` (must scan the full bound to confirm best-fit, since the
/// ONE fitting entry is at the LAST index), immediately `dealloc` (re-
/// deposits at the same now-vacated last index -- occupancy shape is stable
/// across iterations, no re-setup needed).
pub fn run_scan_isolation(core: &mut AllocCore, target_size: usize, rounds: usize) -> u128 {
    let t = Instant::now();
    for _ in 0..rounds {
        let p = alloc_one(core, target_size);
        black_box(p);
        dealloc_one(core, p, target_size);
    }
    t.elapsed().as_nanos()
}
