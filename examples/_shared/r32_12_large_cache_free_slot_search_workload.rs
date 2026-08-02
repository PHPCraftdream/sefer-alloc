// R32-12 (task #503, F8 sub-change (2)) — deterministic, native (portable,
// works on Windows without Valgrind) isolation microjudge for the
// FREE-SLOT-SEARCH scan specifically: `large_cache_find_free_slot`'s old
// `self.large_cache.iter().position(|s| s.is_none())` linear walk
// (`src/alloc_core/alloc_core_large_cache.rs`), which runs on EVERY
// large-dealloc admission attempt — a different call site than R31-8's
// microjudge (`examples/_shared/r31_8_large_cache_scan_isolation_workload.rs`),
// which isolates the BEST-FIT lookup (`alloc_large`'s scan reading
// `slot.usable_size`). Both scans walk the same underlying array; this file
// isolates the OTHER one, per this task's sub-change (2) scope.
//
// WHY THIS EXISTS, NOT R31-8's HARNESS REUSED AS-IS. R31-8's workload times
// `alloc()` immediately followed by `dealloc()` of the SAME size, at a fixed
// worst-case scan position — but the loop's TIMED region covers BOTH the
// best-fit scan (inside `alloc`) AND the free-slot search (inside
// `dealloc`'s admission), conflated into one number. This microjudge isolates
// ONLY the free-slot search: it pre-populates every slot but one (a WORST
// CASE for a linear "find first None" scan — the one free slot sits at the
// LAST scanned index, so the scan must walk the whole bound before finding
// it), then times a REPEATED admission cycle where each iteration deposits
// into that one free slot (clearing it — same size class every time so the
// deposit is never budget-rejected) and the very next `alloc()` of a
// deliberately-INCOMPATIBLE size takes nothing from the cache (a guaranteed
// miss — sized far outside every occupant's `[usable_size, usable_size*2]`
// best-fit window) so the cache's occupancy shape does not change from the
// deposit alone. Symmetric to that: immediately re-evict the miss's own
// fresh reservation back via a same-size `dealloc` at the SAME slot,
// restoring the fixed worst-case shape for the next iteration. Net effect:
// the SAME free-slot-search worst case is walked on every iteration, with
// no best-fit HIT ever occurring (so the best-fit-scan cost this microjudge
// is NOT trying to isolate stays out of the timed region as much as
// possible).
//
// SIMPLER DESIGN ACTUALLY USED. The above two-step "deposit + probe-and-undo"
// shape is more complex than necessary. Simpler and equally valid: since
// `large_cache_find_free_slot` is called ONLY from the admission path (large
// `dealloc`), and the loop question is "how expensive is finding the ONE
// free slot when N-1 others are occupied", we drive it via a REPEATED
// evict-then-refill pattern:
//   1. Populate slots `0..N-1` with permanent (never touched again) decoy
//      entries of DISTINCT sizes, so no two ever alias under best-fit.
//   2. Timed loop: alloc+dealloc a FIXED size, `S`, whose usable_size is
//      picked to NEVER match any decoy under best-fit's `[size, size*2]`
//      window (decoys are all far smaller) — the dealloc's admission must
//      call `large_cache_find_free_slot`, which walks decoys 0..N-2 (all
//      `Some`) before finding the one genuinely free slot at index N-1
//      (whichever slot the PREVIOUS iteration's dealloc-of-S last placed its
//      entry into, then this iteration's alloc-of-S takes it back out via a
//      cache HIT, and the following dealloc re-deposits it at the SAME slot
//      -- `large_cache_find_free_slot` is deterministic: `position()`
//      returns the LOWEST `None` index, so with decoys pinned at 0..N-2 and
//      nothing else ever touching them, the S-cycle's own slot is ALWAYS the
//      single free index N-1, worst-case-positioned by construction).
//
// PATH-ACTIVATION ORACLE (CLAUDE.md R30-8): `dbg_large_cache_hits()`
// before/after the timed loop must show a delta of exactly `ROUNDS` (every
// alloc(S) in the loop is a genuine cache HIT against the entry the PRIOR
// iteration's dealloc(S) deposited — not a miss/fresh-OS-reservation, which
// would silently measure something else). Checked once, before timing
// starts (a single warm-up round primes the shape identically to every
// timed round), and once after, via the delta.

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

/// One Large size, `SEGMENT`-multiple-rounded, `mult` segments large.
fn large_size(mult: usize) -> usize {
    mult * SegmentLayout::SEGMENT
}

/// Populate `core`'s large cache with `decoy_count` permanent decoy entries
/// at sizes `1..=decoy_count` segments (occupying slots `0..decoy_count`,
/// filled in ascending allocation order — the pre-existing
/// `large_cache_find_free_slot`'s first-free-slot semantics fill an
/// initially-empty cache in index order 0, 1, 2, ...). Returns the ONE
/// distinct "cycle size" `S` used by the timed loop: strictly larger than
/// every decoy (`decoy_count + 1` segments) so best-fit's `usable_size >=
/// usable` guard can never match a decoy for an `alloc(S)` request, keeping
/// the decoys permanently untouched for the whole run.
fn populate_decoys(core: &mut AllocCore, decoy_count: usize) -> usize {
    for mult in 1..=decoy_count {
        let sz = large_size(mult);
        let p = alloc_one(core, sz);
        dealloc_one(core, p, sz);
    }
    large_size(decoy_count + 1)
}

/// Run `rounds` timed alloc(S)/dealloc(S) cycles. Each cycle:
///   - `alloc(S)`: a cache HIT against the entry the PRIOR iteration (or the
///     one-time priming dealloc before the timed loop) deposited at the
///     single free slot (index `decoy_count`) — this is the ONLY entry that
///     fits `S` under best-fit, so the scan (if the best-fit path is
///     reached at all before a match — see this file's module doc) always
///     resolves to this one slot, never a decoy.
///   - `dealloc(S)`: re-deposits at the now-vacated slot. Because every
///     decoy slot (`0..decoy_count`) is PERMANENTLY occupied and never
///     freed/retaken, `large_cache_find_free_slot`'s scan must walk all
///     `decoy_count` occupied decoy slots before finding the one free slot
///     at index `decoy_count` — the worst case for a "find first None"
///     linear scan, on EVERY iteration.
pub fn run_free_slot_search_isolation(
    core: &mut AllocCore,
    cycle_size: usize,
    rounds: usize,
) -> u128 {
    let t = Instant::now();
    for _ in 0..rounds {
        let p = alloc_one(core, cycle_size);
        black_box(p);
        dealloc_one(core, p, cycle_size);
    }
    t.elapsed().as_nanos()
}
