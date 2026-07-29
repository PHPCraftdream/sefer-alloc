//! R26-5 (task #414) — per-block partition oracle for `dealloc_batch`'s
//! multi-flush path. **STRENGTHENS** (does not replace) the aggregate oracle
//! in `tests/r25_4_dealloc_batch_multi_flush_oracle.rs` (R25-4, task #398).
//!
//! ## What the aggregate oracle proves, and the gap this file closes
//!
//! `r25_4` proves the EXACT aggregate `live_count` transition for one
//! `dealloc_batch` of N=200 blocks: `live_count` drops by exactly
//! `N - TCACHE_CAP` (= 184), because the first `TCACHE_CAP` (16) accepted
//! blocks stay magazine-resident (first-warm policy — no `dec_live`) and the
//! remaining 184 are routed through `flush_class`/`dec_live`. A broken
//! multi-flush loop that drops, double-counts, or re-flushes a staged chunk
//! changes that aggregate delta, so `r25_4` catches it.
//!
//! **But `r25_4` cannot catch a CANCELLING-PAIR bug.** A bug that mis-handles
//! exactly TWO blocks in offsetting ways — e.g. one block silently LEAKED
//! (never flushed; still allocated, still counted live — `live_count` +1 vs.
//! the expected) and one DIFFERENT block DOUBLE-PROCESSED (flushed while ALSO
//! kept magazine-resident, so `dec_live` is called once for it — `live_count`
//! −1 vs. expected) — nets to the SAME aggregate delta of 184. The two
//! errors cancel in the `live_count` sum, so the aggregate assertion passes
//! while two individual blocks are in corrupted, unsound states:
//!
//! - the leaked block is STILL ALLOCATED from the allocator's perspective
//!   (its caller thinks it was freed; the allocator may hand its storage to
//!   a DIFFERENT future allocation → aliasing / use-after-free on reuse), and
//! - the double-processed magazine block is simultaneously on the free list
//!   AND in the tcache, so it can be handed out by BOTH the next `alloc`
//!   (magazine pop) and the next same-segment free-list pop → two callers
//!   get the same storage.
//!
//! ## What this test proves, precisely
//!
//! **Every one of the N=200 `dealloc_batch`'d blocks is individually
//! classified by TWO independent signals, and the resulting classification
//! partitions all 200 blocks exactly into the two expected sets — no block
//! in BOTH, no block in NEITHER.** The two signals are:
//!
//! 1. `dbg_tcache_contains(c, p)` — `true` iff `p` is sitting in one of this
//!    heap's magazine slots for class `c` (the magazine-resident predicate).
//!    Mirrors exactly the first-warm policy's result: the first `TCACHE_CAP`
//!    accepted blocks occupy `slots[0..16]`.
//! 2. `dbg_is_free_for(p)` — `true` iff `p`'s alloc-bitmap bit reads FREE
//!    (its storage is on a free list, available for reuse). A magazine-
//!    resident block reads ALLOCATED here (the magazine push never calls
//!    `dec_live`/`mark_free`), so this signal is genuinely INDEPENDENT of
//!    signal 1 for a correctly-partitioned block.
//!
//! The four combinations map to four disjoint sets:
//!
//! | `(in_magazine, is_free)` | meaning                       | expected size |
//! |--------------------------|-------------------------------|---------------|
//! | `(true, false)`          | magazine-resident (correct)   | `TCACHE_CAP`  |
//! | `(false, true)`          | genuinely free (correct)      | `N-TCACHE_CAP`|
//! | `(false, false)`         | NEITHER — leaked (BUG)        | 0             |
//! | `(true, true)`           | BOTH — double-processed (BUG) | 0             |
//!
//! A cancelling-pair bug populates the `BOTH` and `NEITHER` sets (one block
//! each), which the aggregate `live_count` delta cannot detect because the
//! two errors net to zero. This test asserts both error sets are empty, so it
//! fails on exactly that class of bug.
//!
//! It ALSO asserts the magazine-resident set is exactly the FIRST `TCACHE_CAP`
//! block indices `{0,1,...,15}` — confirming the first-warm policy fills
//! `slots[0..16]` with `blocks[0..16]` in source order (a stronger check than
//! just "16 blocks are magazine-resident somewhere").
//!
//! ## Accessors this test relies on (both added by this task)
//!
//! - `HeapCore::dbg_tcache_contains(c, ptr)` (`src/registry/heap_core_diag.rs`)
//!   — safe read-only scan of `tcache.classes[c].slots[0..count]`.
//! - `HeapCore::dbg_is_free_for(ptr)` (`src/registry/heap_core_diag.rs`) —
//!   thin forwarder to the pre-existing `AllocCore::dbg_is_free_for`, exposed
//!   at the `HeapCore` level for this test. Both are `#[doc(hidden)]` safe
//!   reads (no raw-pointer metadata writes) — see their doc comments for why
//!   they are plain safe `fn`, not the `unsafe fn`/`bench-internals`-gated
//!   category reserved for hooks that WRITE through unvalidated pointers.
//!
//! ## Mutation counterfactual (run by the orchestrator, reverted before finish)
//!
//! Two mutations are applied to `src/registry/heap_core_dealloc_batch.rs`,
//! each run against THIS test (and `r25_4`) to confirm non-vacuity, then
//! reverted:
//!
//! 1. **Drop-staged-chunk** (the mutation `r25_4` already documents): the
//!    mid-loop `flush_class` is replaced with `staged = 0;` (drop the staged
//!    pointers). Expected: BOTH the aggregate oracle AND this per-block test
//!    go RED (the dropped blocks are neither magazine-resident nor free —
//!    they land in the `NEITHER` set).
//! 2. **Cancelling pair** (the mutation ONLY this test is designed to catch):
//!    one overflow block is dropped from staging (leaked — still allocated),
//!    and in its place one magazine-resident block (`slots[0]`) is flushed
//!    via `flush_class` (double-processed — marked free AND kept in the
//!    magazine). Expected: the AGGREGATE oracle stays GREEN (the +1 leak and
//!    −1 double-process net to the same delta=184), but THIS per-block test
//!    goes RED on BOTH the `BOTH` and `NEITHER` set assertions. Observed
//!    numbers are recorded in the task summary, not hardcoded here.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "batch-api",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise against other tests in this binary: the registry is a
// process-global static shared across every HeapCore in the process (same
// idiom as `tests/heap_core_tcache.rs` / `tests/r25_4_dealloc_batch_multi_flush_oracle.rs`).
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

/// `TCACHE_CAP` (`src/registry/tcache.rs`) is `pub(crate)` — mirrored here as
/// a literal, exactly like `tests/r25_4_dealloc_batch_multi_flush_oracle.rs`.
const TCACHE_CAP: usize = 16;

/// N=200 at `STAGE_CAP=64`/`TCACHE_CAP=16`: identical scenario to
/// `r25_4_dealloc_batch_multi_flush_oracle` (same N, same layout, same
/// magazine-empty precondition, same same-segment precondition), so the
/// aggregate assertions below are directly comparable to that file's, and the
/// per-block partition is a strict strengthening layered on top.
#[test]
fn dealloc_batch_per_block_partition_is_exact() {
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

    // Precondition: all N blocks land in exactly ONE segment — same rationale
    // as `r25_4` (makes the live_count delta a clean single number and
    // guarantees every block shares one segment's allocation bitmap, which
    // `dbg_is_free_for` reads).
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

    // Flush the magazine to empty HERE (before recording the baseline) so the
    // setup loop's last refill-batch leftovers don't perturb `live_before`
    // and so `dealloc_batch`'s magazine-fill phase starts from `cnt == 0`,
    // matching the first-warm policy exactly. Identical to `r25_4`.
    unsafe { (*heap).dbg_flush_all() };

    // Authoritative live_count BEFORE the batched free (magazine now empty
    // for this class: live_count == exactly the N=200 blocks in `blocks`).
    let live_before = unsafe { (*heap).dbg_live_count_for(blocks[0]) }
        .expect("segment must be small/primordial and registered");
    assert_eq!(
        live_before as usize, n,
        "precondition: after flushing the magazine, live_count must equal \
         exactly N"
    );

    // Phase 2: free all N in ONE dealloc_batch call — same multi-flush path
    // `r25_4` exercises.
    // SAFETY: every entry of `blocks` was allocated by `heap` above with
    // `layout`; freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &blocks) };

    // ── AGGREGATE assertions (kept identical to `r25_4` — this test
    // STRENGTHENS that oracle, it does not replace it) ──────────────────────
    let live_after =
        unsafe { (*heap).dbg_live_count_for(blocks[0]) }.expect("segment must still be registered");
    let expected_delta = (n - TCACHE_CAP) as u32;
    assert_eq!(
        live_before.saturating_sub(live_after),
        expected_delta,
        "aggregate: live_count did not drop by the expected {expected_delta}"
    );

    let c =
        unsafe { (*heap).dbg_class_for(layout) }.expect("16 B @ align 8 must be Small-classified");
    let tcache_count = unsafe { (*heap).dbg_tcache_count(c) };
    assert_eq!(
        tcache_count as usize, TCACHE_CAP,
        "aggregate: magazine for class {c} must hold exactly TCACHE_CAP blocks"
    );

    // ── PER-BLOCK PARTITION (the load-bearing new check this file exists
    //    for). Classify each of the 200 blocks by two INDEPENDENT signals:
    //    magazine-residency (`dbg_tcache_contains`) and free-bit
    //    (`dbg_is_free_for`). The four combinations must partition into
    //    exactly: 16 magazine-resident + 184 free + 0 both + 0 neither. ────
    let mut magazine_resident: Vec<usize> = Vec::with_capacity(TCACHE_CAP);
    let mut genuinely_free: Vec<usize> = Vec::with_capacity(n - TCACHE_CAP);
    let mut neither: Vec<usize> = Vec::new(); // expected empty
    let mut both: Vec<usize> = Vec::new(); // expected empty

    for (i, &p) in blocks.iter().enumerate() {
        let in_magazine = unsafe { (*heap).dbg_tcache_contains(c, p) };
        let is_free = unsafe { (*heap).dbg_is_free_for(p) };
        match (in_magazine, is_free) {
            (true, false) => magazine_resident.push(i),
            (false, true) => genuinely_free.push(i),
            (false, false) => neither.push(i),
            (true, true) => both.push(i),
        }
    }

    assert!(
        both.is_empty(),
        "BOTH magazine-resident AND free (a block double-processed: flushed \
         via flush_class AND kept in the tcache — would be handed out twice \
         on reuse): {:?}",
        both
    );
    assert!(
        neither.is_empty(),
        "NEITHER magazine-resident nor free (a block LEAKED: still allocated, \
         its caller thinks it was freed — storage may be aliased on reuse): \
         {:?}",
        neither
    );
    assert_eq!(
        magazine_resident.len(),
        TCACHE_CAP,
        "exactly TCACHE_CAP({TCACHE_CAP}) blocks must be magazine-resident, \
         got {}: {:?}",
        magazine_resident.len(),
        magazine_resident
    );
    assert_eq!(
        genuinely_free.len(),
        n - TCACHE_CAP,
        "exactly N-TCACHE_CAP({}) blocks must be genuinely free, got {}: {:?}",
        n - TCACHE_CAP,
        genuinely_free.len(),
        genuinely_free
    );

    // The magazine-resident set must be EXACTLY the first TCACHE_CAP block
    // indices — the first-warm policy fills slots[0..16] with blocks[0..16]
    // in source order. This is a stronger check than "16 blocks are
    // magazine-resident somewhere in `blocks`".
    let expected_magazine: Vec<usize> = (0..TCACHE_CAP).collect();
    assert_eq!(
        magazine_resident, expected_magazine,
        "magazine-resident set must be exactly blocks[0..TCACHE_CAP] \
         (first-warm policy fills slots in source order)"
    );

    // Cleanup: drain the magazine then recycle, identical to `r25_4`.
    unsafe { (*heap).dbg_flush_all() };
    // SAFETY: `heap` was claimed above via HeapRegistry::claim; recycled
    // whole here, matching every other isolated-HeapCore test's teardown.
    unsafe { HeapRegistry::recycle(heap) };
}
