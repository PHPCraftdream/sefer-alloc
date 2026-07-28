//! R25-4 (task #398) — isolated `HeapCore`-level correctness oracle for the
//! `dealloc_batch` multi-flush path (`src/registry/heap_core_dealloc_batch.rs`),
//! proving the property `tests/r24_8_dealloc_batch_multi_flush.rs` explicitly
//! disclaims it cannot prove.
//!
//! ## What this test proves, precisely
//!
//! **Every one of the N blocks freed in a single multi-flush `dealloc_batch`
//! call is correctly accounted for in `HeapCore`'s OWN authoritative
//! `live_count` bookkeeping** — not "some later allocation succeeded", but a
//! direct before/after state-transition check against the segment's real
//! live-block counter (`AllocCore::dbg_live_count_for`, the same counter
//! `alloc-decommit` itself trusts to decide when a segment is empty enough to
//! release).
//!
//! The expected transition is NOT "live_count drops by N": per the D1
//! invariant documented on `HeapCore::tcache` (`src/registry/heap_core.rs`),
//! *"a magazine-resident block COUNTS AS LIVE... magazine push/pop do NOT
//! touch live_count... magazine flush calls dealloc_small -> dec_live"*. The
//! multi-flush batched path (`dealloc_batch_small`) fills the magazine
//! FIRST-warm up to `TCACHE_CAP` (16) blocks (no `dec_live` — those stay
//! "live" by design, exactly like the scalar path would), and only the
//! REMAINING blocks are routed through `AllocCore::flush_class`, each of
//! which DOES call `dec_live`. So for N=200 at `TCACHE_CAP=16`, the correct,
//! exact expected transition is `live_count -= (N - TCACHE_CAP) = 184`, not
//! `-= 200`. This test asserts that EXACT number — not `<=`, not "eventually
//! reaches some steady state" — which is precisely the state a broken
//! multi-flush loop (miscounting staged entries, double-flushing a chunk, or
//! silently dropping a chunk) would get wrong.
//!
//! ## Why this is provable HERE but not in the R24-8 `GlobalAlloc` test
//!
//! `tests/r24_8_dealloc_batch_multi_flush.rs` runs through `SeferAlloc` wired
//! up as `#[global_allocator]`. Under that setup, the test harness's OWN
//! machinery (`Vec`/`HashSet` growth, assertion formatting, `std::collections`
//! internals) allocates from the SAME global pool between the `dealloc_batch`
//! call and any later inspection — so that file settles for the weaker
//! "no OOM + no aliasing on re-alloc" property (see its own module doc).
//!
//! This test instead follows the established isolated-`HeapCore` idiom (see
//! `tests/heap_core_tcache.rs`, `tests/regression_batch_flush.rs`'s sibling
//! `AllocCore`-level `dbg_live_count_for` idiom, and
//! `tests/r11_4_dealloc_batch_mixed_ownership.rs`): it obtains a `*mut
//! HeapCore` directly via `HeapRegistry::claim()` and calls
//! `HeapCore::alloc`/`HeapCore::dealloc_batch`/`HeapCore::dbg_live_count_for`
//! DIRECTLY — bypassing `SeferAlloc`/`#[global_allocator]` entirely. This
//! file never installs `SeferAlloc` as the global allocator, so `live_count`
//! is read straight from the authoritative segment header, with no
//! intervening allocation able to perturb it between the `dealloc_batch` call
//! and the assertion.
//!
//! ## Mutation counterfactual (documented; run by the implementer, reverted)
//!
//! To confirm this test is non-vacuous at `STAGE_CAP = 64` (i.e. it would
//! actually catch a broken multi-flush path, not just compile and pass by
//! construction): temporarily changed the mid-loop flush guard in
//! `dealloc_batch_small` (`src/registry/heap_core_dealloc_batch.rs`) from
//!
//! ```text
//! if staged == STAGE_CAP {
//!     unsafe { self.core.flush_class(c, &stage[..staged]) };
//!     staged = 0;
//! }
//! ```
//! to
//! ```text
//! if staged == STAGE_CAP {
//!     staged = 0; // BUG: drop the staged blocks instead of flushing them
//! }
//! ```
//!
//! i.e. the mid-loop reset now silently DISCARDS the staged pointers instead
//! of flushing them to `AllocCore::flush_class` before resetting the cursor.
//! This reproduces a "lost chunk" multi-flush bug without touching array
//! bounds (so it does not just trip a bounds panic the way disabling the
//! flush entirely would — see `tests/r24_8_dealloc_batch_multi_flush.rs`'s
//! own documented counterfactual for that variant). At N=200 the mid-loop
//! guard fires at BOTH `STAGE_CAP`(64) boundaries (64 + 64 staged, mutated to
//! drop both chunks instead of flushing), leaving only the final 56-entry
//! post-loop flush (which is unconditional, outside the mutated `if` — see
//! the `if staged > 0` block after the loop) to actually call
//! `flush_class`/`dec_live`. **Actually observed** (re-run against this exact
//! mutation, not estimated): `live_before=200, live_after=144`, a delta of
//! only `56` instead of the expected `184` — exactly the "only the final
//! 56-entry chunk survived" prediction. This tripped this test's exact
//! `assert_eq!` on the live-count delta (RED). Reverting the mutation
//! restored GREEN (`live_before=200, live_after=16`, delta `184`, matching
//! the magazine holding exactly `TCACHE_CAP`=16 resident blocks). This
//! confirms the test fails when the multi-flush path silently loses staged
//! chunks, not just when it panics outright.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "batch-api",
    feature = "alloc-decommit"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise against other tests in this binary: the registry is a
// process-global static shared across every HeapCore in the process (same
// idiom as `tests/heap_core_tcache.rs` / `tests/r11_4_dealloc_batch_mixed_ownership.rs`).
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// `TCACHE_CAP` (`src/registry/tcache.rs`) is `pub(crate)`, invisible from
/// `tests/` — mirrored here as a literal, exactly like
/// `tests/r24_8_dealloc_batch_multi_flush.rs`'s own doc comment already does
/// for `STAGE_CAP`/`TCACHE_CAP`.
const TCACHE_CAP: usize = 16;

/// N=200 at `STAGE_CAP=64`/`TCACHE_CAP=16`: the first 16 accepted blocks fill
/// the magazine (first-warm policy), the remaining 184 stage and flush in two
/// 64-entry intermediate `flush_class` calls plus one final 56-entry flush
/// (64 + 64 + 56 = 184) — the SAME three-flush shape
/// `tests/r24_8_dealloc_batch_multi_flush.rs` exercises, here proven against
/// authoritative `HeapCore`/`AllocCore` live-count state instead of through
/// `GlobalAlloc`.
#[test]
fn dealloc_batch_multi_flush_live_count_transition_is_exact() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(16, 8).unwrap();
    let n = 200usize;

    // Phase 1: allocate N blocks directly via HeapCore::alloc (no
    // GlobalAlloc indirection — no other allocation happens on this heap
    // between here and the dealloc_batch call below).
    let mut blocks: Vec<*mut u8> = Vec::with_capacity(n);
    for i in 0..n {
        // SAFETY: valid non-zero layout; `heap` was just claimed.
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "setup alloc returned null at i={i}");
        blocks.push(p);
    }

    // Precondition: all N blocks land in exactly ONE segment (16 B blocks
    // comfortably fit one 4 MiB segment at this count) — this is what makes
    // the multi-flush path's per-segment `live_count` delta a clean, single
    // number to check, matching the file's module-doc "pure same-segment
    // multi-flush stress" framing (mirrors `r24_8`'s own precondition).
    let seg_bases: std::collections::HashSet<usize> = blocks
        .iter()
        .map(|&p| unsafe { (*heap).dbg_segment_base_of_ptr(p) } as usize)
        .collect();
    assert_eq!(
        seg_bases.len(),
        1,
        "test precondition: all {n} blocks must share one segment, got {} segments",
        seg_bases.len()
    );

    // Magazine-refill batching (`refill_n_for_class`, `TCACHE_CAP`-sized
    // batches for this class) means the setup loop above almost certainly
    // leaves the LAST refill's leftover blocks magazine-resident (live, but
    // NOT in `blocks` — e.g. N=200=12*16+8 leaves 8 extra live-but-unissued
    // blocks sitting in the magazine after the loop, since the 13th refill
    // batch of 16 only had 8 of its blocks actually handed to the caller).
    // Flush the magazine to empty HERE (before recording the baseline) so
    // `live_before`/`live_after` below reflect EXACTLY the N=200 blocks this
    // test holds references to, and so `dealloc_batch`'s magazine-fill phase
    // starts from a known-empty magazine (`cnt == 0`), matching this file's
    // documented "first TCACHE_CAP accepted blocks fill the magazine"
    // expectation precisely instead of being perturbed by refill-batch
    // leftovers unrelated to the multi-flush path under test.
    unsafe { (*heap).dbg_flush_all() };

    // Authoritative live_count BEFORE the batched free (magazine now empty
    // for this class: live_count == exactly the N=200 blocks in `blocks`).
    let live_before = unsafe { (*heap).dbg_live_count_for(blocks[0]) }
        .expect("segment must be small/primordial and registered");
    assert_eq!(
        live_before as usize, n,
        "precondition: after flushing the magazine, live_count must equal \
         exactly N — otherwise something besides this test's own N \
         allocations is live in this segment, and the delta assertion below \
         would not isolate the multi-flush path's own behaviour"
    );

    // Phase 2: free all N in ONE dealloc_batch call, directly on HeapCore —
    // triggers the exact multi-flush path (2 intermediate + 1 final
    // flush_class calls at STAGE_CAP=64: 64 + 64 + 56 = 3 flushes total).
    // SAFETY: every entry of `blocks` was allocated by `heap` above with
    // `layout`; freed exactly once here, in a single well-formed call.
    unsafe { (*heap).dealloc_batch(layout, &blocks) };

    // Authoritative live_count AFTER the batched free.
    let live_after = unsafe { (*heap).dbg_live_count_for(blocks[0]) }
        .expect("segment must still be registered (small segment; not the sole occupant, so it should not have decommitted/recycled away entirely)");

    // The load-bearing assertion this file exists for: the EXACT expected
    // live_count transition. First-warm policy keeps the first TCACHE_CAP
    // (16) accepted blocks magazine-resident (still "live" per the D1
    // invariant — see module doc); the remaining N - TCACHE_CAP = 184 blocks
    // are routed through flush_class's dec_live, one decrement per accepted
    // block. A multi-flush bug that drops, double-counts, or re-flushes a
    // staged chunk changes this delta from exactly 184 to something else —
    // this is a strictly stronger, more precise assertion than "no OOM on
    // re-alloc" (which tolerates a live_count that is wrong by a wide margin
    // as long as SOME free capacity exists somewhere).
    let expected_delta = (n - TCACHE_CAP) as u32;
    assert_eq!(
        live_before.saturating_sub(live_after),
        expected_delta,
        "live_count did not drop by the expected {expected_delta} (= N({n}) - \
         TCACHE_CAP({TCACHE_CAP})) — the multi-flush dealloc_batch path lost \
         track of at least one staged block (live_before={live_before}, \
         live_after={live_after})"
    );

    // Also assert the magazine itself now holds exactly TCACHE_CAP resident
    // blocks for this class — confirms the "missing" live_count decrement is
    // fully and ONLY explained by the documented first-warm magazine
    // residency, not by some other accounting error that happens to net out
    // to the same delta.
    let c =
        unsafe { (*heap).dbg_class_for(layout) }.expect("16 B @ align 8 must be Small-classified");
    let tcache_count = unsafe { (*heap).dbg_tcache_count(c) };
    assert_eq!(
        tcache_count as usize, TCACHE_CAP,
        "magazine for class {c} must hold exactly TCACHE_CAP({TCACHE_CAP}) \
         resident blocks after the batched free (first-warm policy) — got {tcache_count}"
    );

    // Cleanup: drain the magazine (dbg_flush_all forwards to the same
    // flush_class the batched path itself uses) so the heap is left tidy
    // before recycling.
    unsafe { (*heap).dbg_flush_all() };

    // SAFETY: `heap` was claimed above via HeapRegistry::claim; recycled
    // whole here, matching every other isolated-HeapCore test's teardown.
    unsafe { HeapRegistry::recycle(heap) };
}
