//! R2-22 (independent src review round 2): `AllocStats::ring_overflows`
//! documentation/semantics counterfactuals.
//!
//! ## The finding
//!
//! `ring_overflows` reads `DBG_RING_OVERFLOW`, which ticks when a
//! cross-thread free's FIRST counted push onto a segment's `RemoteFreeRing`
//! finds it full. That is a first-tier MISS, not a loss: after the tick, the
//! free is still saved by the next tiers — the owning heap's second-chance
//! `HeapOverflow` ring (tried immediately) and the bounded spin-retry against
//! both tiers. Only when EVERY tier fails does `DBG_RING_PUSH_RETRY_EXHAUSTED`
//! (exposed as `AllocStats::cross_thread_frees_lost`, R2-09) tick — that is
//! the actual "block permanently discarded" counter. The pre-R2-22 rustdoc
//! asserted the block "is **discarded**" on overflow and that a high rate
//! means "actually being leaked" — a false-leak-diagnosis hazard this task's
//! doc fix (in `src/global/alloc_stats.rs`) removes.
//!
//! ## The three review acceptance cases and where each is pinned
//!
//! 1. **ring miss + immediate overflow-ring rescue** — pinned HERE by
//!    [`ring_miss_rescued_by_overflow_ring_is_not_a_loss`]: exactly two
//!    first-tier misses, both rescued by `HeapOverflow`, zero retries, zero
//!    loss, and BOTH blocks physically reissued to the owner.
//! 2. **retry success** — pinned HERE (native-only) by
//!    [`double_saturation_retry_success_is_not_a_loss`]: segment ring AND
//!    `HeapOverflow` both full, the bounded spin-retry recovers the free,
//!    `DBG_RING_PUSH_RETRIED` ticks exactly once, zero loss.
//! 3. **genuine terminal drop** — already pinned by
//!    `tests/remote_fanin.rs::remote_fanin_owner_starved_residual_is_exactly_accounted`
//!    (N=3000 burst exceeds RING_CAP+HEAP_OVERFLOW_CAP; asserts
//!    `exhausted_delta > 0` and the public `stats().cross_thread_frees_lost`
//!    wiring). Deliberately NOT duplicated here — duplicating that calibrated
//!    burst would only slow the suite.
//!
//! Together with the guard test in `tests/no_stale_doc_references.rs`
//! (`no_ring_overflow_leak_overclaim_in_alloc_stats_docs`), this makes the
//! distinction mechanical: the SAME `ring_overflows` event is proven NOT
//! interpretable as a leak in cases 1 and 2, and case 3 shows which field
//! actually means loss.
//!
//! **Feature gate.** `internals` (dbg hooks), `alloc-global`, `alloc-xthread`
//! (ring/retry path), `fastbin` (magazine refill reasoning),
//! `alloc-segment-directory` — the same harness shape as
//! `tests/r11_2_overflow_drain_directory_sync.rs`, whose deterministic
//! `dbg_push_to_ring` ring-full construction this file mirrors. Under other
//! feature configurations the file compiles as an empty test binary (0
//! tests, pass by absence).

#![cfg(all(
    feature = "internals",
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "alloc-segment-directory"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::alloc_core::remote_free_ring::{DBG_RING_OVERFLOW, RING_CAP};
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{
    bootstrap, HeapRegistry, DBG_RING_PUSH_RETRIED, DBG_RING_PUSH_RETRY_EXHAUSTED,
};

// Serialise all tests in this binary: the registry and the diagnostic
// counters are process-global statics; concurrent test-fn execution under
// `cargo test`'s default multi-threaded runner would make the exact-delta
// assertions flaky (same pattern as tests/remote_fanin.rs and
// tests/r11_2_overflow_drain_directory_sync.rs).
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

/// Same choice as tests/r11_2_overflow_drain_directory_sync.rs (refill_n == 1).
const TARGET_CLASS: usize = 40;

/// ── Case 1: ring miss rescued IMMEDIATELY by the overflow ring ───────────
///
/// Deterministic, miri-scale-small. Fill one segment's `RemoteFreeRing` to
/// `RING_CAP` with double-free notes for a LIVE block (`dbg_push_to_ring`,
/// rejected defensively at drain time — the established
/// `tests/r11_2_overflow_drain_directory_sync.rs` construction), then free
/// TWO blocks of that segment cross-thread. Each free must: (a) fail its
/// first counted ring push → tick `DBG_RING_OVERFLOW` exactly once (the
/// first-tier miss the `ring_overflows` field counts), (b) be rescued
/// IMMEDIATELY by the owner's `HeapOverflow` ring — no spin-retry
/// (`DBG_RING_PUSH_RETRIED` delta 0), no terminal loss
/// (`DBG_RING_PUSH_RETRY_EXHAUSTED` delta 0) — and (c) be PHYSICALLY
/// reissued to the owner by the next two allocs, proving nothing leaked.
///
/// Counterfactual: under the pre-R2-22 doc's claim ("on overflow the freed
/// block is discarded — blocks are actually being leaked") these two ticks
/// would read as two leaked blocks; the reissue assertions below prove zero
/// loss for the exact events the field counted.
#[test]
fn ring_miss_rescued_by_overflow_ring_is_not_a_loss() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Same class-40 discipline as
    // tests/r11_2_overflow_drain_directory_sync.rs: carves its own segments
    // and `refill_n == 1`, giving deterministic magazine/refill reasoning.
    let refill_n = unsafe { (*heap).dbg_refill_n_for_class(TARGET_CLASS) };
    assert_eq!(
        refill_n, 1,
        "TARGET_CLASS must have refill_n=1 for this test's magazine reasoning"
    );
    let bs = AllocCore::dbg_block_size(TARGET_CLASS);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS layout");

    // Six blocks of TARGET_CLASS — all must land in ONE segment (~93 blocks
    // of ~43 KiB fit in a 4 MiB segment), giving the ring-fill target + the
    // two rescue victims + three spares that keep the segment live.
    let mut blocks: Vec<*mut u8> = Vec::with_capacity(6);
    for i in 0..6 {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc[{i}] returned null");
        blocks.push(p);
    }
    let base = unsafe { (*heap).dbg_segment_base_of_ptr(blocks[0]) };
    for (i, &b) in blocks.iter().enumerate() {
        let b_base = unsafe { (*heap).dbg_segment_base_of_ptr(b) };
        assert_eq!(
            b_base, base,
            "block {i} landed in a different segment — the one-segment assumption broke"
        );
    }
    let p_ring = blocks[0];
    let p_ovf_a = blocks[1];
    let p_ovf_b = blocks[2];

    let overflow_before = DBG_RING_OVERFLOW.load(Ordering::Relaxed);
    let retried_before = DBG_RING_PUSH_RETRIED.load(Ordering::Relaxed);
    let exhausted_before = DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed);

    // Fill the segment's ring to capacity with double-free notes for the
    // LIVE `p_ring`, then verify one more push fails: the ring is full.
    for _ in 0..RING_CAP {
        let ok = unsafe { (*heap).dbg_push_to_ring(p_ring, TARGET_CLASS) };
        assert!(ok, "fill push failed before reaching RING_CAP");
    }
    let ok = unsafe { (*heap).dbg_push_to_ring(p_ring, TARGET_CLASS) };
    assert!(!ok, "ring should be full after RING_CAP pushes");
    // That failing probe IS a counted push — it ticked DBG_RING_OVERFLOW
    // once. Re-baseline so the deltas below measure ONLY the producer's two
    // cross-thread frees.
    let overflow_after_fill = DBG_RING_OVERFLOW.load(Ordering::Relaxed);
    debug_assert_eq!(overflow_after_fill - overflow_before, 1);

    // Two cross-thread frees into the FULL ring from a producer thread.
    let a = p_ovf_a as usize;
    let b = p_ovf_b as usize;
    let producer = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "producer HeapRegistry::claim failed");
        // SAFETY: `remote` is a live heap; both addresses are blocks
        // previously allocated by `heap` (the owner) and are freed exactly
        // once, here, from this different thread — routing through
        // `dealloc_foreign_slow` → `push_with_overflow_retry`.
        unsafe { (*remote).dealloc(a as *mut u8, layout) };
        unsafe { (*remote).dealloc(b as *mut u8, layout) };
        unsafe { HeapRegistry::recycle(remote) };
    });
    producer.join().expect("producer thread must not panic");

    let overflow_delta = DBG_RING_OVERFLOW.load(Ordering::Relaxed) - overflow_after_fill;
    let retried_delta = DBG_RING_PUSH_RETRIED.load(Ordering::Relaxed) - retried_before;
    let exhausted_delta = DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed) - exhausted_before;

    // The doc-semantics oracle: two first-tier misses, zero retries, zero
    // loss — the exact distinction the fixed AllocStats doc draws.
    assert_eq!(
        overflow_delta, 2,
        "each cross-thread free into the full ring must tick DBG_RING_OVERFLOW \
         exactly once (the first-tier miss `ring_overflows` counts)"
    );
    assert_eq!(
        retried_delta, 0,
        "an immediate HeapOverflow rescue must NOT reach the spin-retry tier \
         (DBG_RING_PUSH_RETRIED ticked)"
    );
    assert_eq!(
        exhausted_delta, 0,
        "a rescued free must NOT tick the terminal-loss counter \
         (DBG_RING_PUSH_RETRY_EXHAUSTED / cross_thread_frees_lost)"
    );

    // Physical proof they were rescued, not leaked: the owner's next two
    // allocs must reissue EXACTLY these two blocks. Alloc #1 misses the
    // (empty, refill_n==1) magazine → refill → drain_heap_overflow reclaims
    // BOTH overflow entries onto the BinTable and issues one; alloc #2
    // issues the second. No other free blocks exist in this freshly carved
    // segment set, so nothing else can come back.
    let mut reclaimed: Vec<usize> = Vec::with_capacity(2);
    for i in 0..2 {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "reclaim alloc[{i}] returned null");
        reclaimed.push(p as usize);
    }
    let mut sorted = reclaimed.clone();
    sorted.sort_unstable();
    let mut expected = vec![p_ovf_a as usize, p_ovf_b as usize];
    expected.sort_unstable();
    assert_eq!(
        sorted, expected,
        "both overflow-rescued blocks must be reissued to the owner — they \
         were NOT leaked despite two `ring_overflows` ticks"
    );

    // Cleanup. `p_ovf_a`/`p_ovf_b` were freed by the producer; `reclaimed`
    // holds their reissued lives (freed here, once each). `p_ring` and the
    // three spares were never producer-freed → freed here. The 256 stale
    // double-free notes were already drained/rejected during reclaim alloc
    // #2 (p_ring is still live, so every note is defensively rejected).
    for &p in &reclaimed {
        // SAFETY: live reissued allocation owned by this heap; freed once.
        unsafe { (*heap).dealloc(p as *mut u8, layout) };
    }
    for (i, &p) in blocks.iter().enumerate() {
        if i != 1 && i != 2 {
            // SAFETY: live allocation owned by this heap; freed exactly once
            // (i == 1/2 were producer-freed and are covered by `reclaimed`).
            unsafe { (*heap).dealloc(p, layout) };
        }
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// Class 0: smallest block size, so 2049 blocks fit in one segment with
/// huge margin (the one-segment assumption is asserted at runtime below).
const FILL_CLASS: usize = 0;

/// ── Case 2: double saturation resolved by a SUCCESSFUL retry ─────────────
///
/// Native-only (`#[cfg(not(miri))]`): hardcodes the native
/// `HEAP_OVERFLOW_CAP = 2048` (`src/registry/heap_overflow.rs` — under miri
/// it is shrunk to 64) and leans on real scheduler sleeps inside the retry
/// loop; the retry path's miri UB coverage already lives in
/// `tests/remote_fanin.rs::remote_fanin_miri_minimal_retry_ub_check`.
///
/// Shape: fill the segment ring (`RING_CAP` stale notes), then have a
/// producer free exactly `HEAP_OVERFLOW_CAP` blocks cross-thread — every one
/// fails its first ring push (ticks `DBG_RING_OVERFLOW`) and is rescued by
/// `HeapOverflow`, which ends EXACTLY full. The next free (`p_stuck`) fails
/// its ring push AND the immediate overflow attempt → it enters the bounded
/// spin-retry. The owner waits for that tick, then drains via allocs
/// (refill-miss → `drain_heap_overflow`), giving the retry room: the push
/// must land through the RETRY tier (`DBG_RING_PUSH_RETRIED` ticks exactly
/// once) with ZERO terminal loss.
///
/// Counterfactual: this is the third case the (fixed) doc must distinguish —
/// `ring_overflows` ticked FILL+1 times, every free was saved, and the field
/// that stays at zero (`cross_thread_frees_lost`) is the one the doc points
/// to for actual loss. If `HEAP_OVERFLOW_CAP` ever changes, this test fails
/// loudly (`retried_delta == 0`: p_stuck would be rescued immediately
/// instead of entering the retry) rather than passing vacuously.
#[cfg(not(miri))]
#[test]
fn double_saturation_retry_success_is_not_a_loss() {
    const HEAP_OVERFLOW_CAP: usize = 2048;
    const FILL: usize = HEAP_OVERFLOW_CAP; // one overflow entry per fill-free

    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let bs = AllocCore::dbg_block_size(FILL_CLASS);
    assert!(
        bs <= 512,
        "FILL_CLASS block size {bs} too large for the one-segment assumption"
    );
    let layout = Layout::from_size_align(bs, 8).expect("FILL_CLASS layout");

    // blocks[0] is p_stuck — allocated FIRST so its segment certainly also
    // holds fill blocks (that segment's ring gets filled below).
    let mut blocks: Vec<*mut u8> = Vec::with_capacity(1 + FILL);
    for i in 0..(1 + FILL) {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc[{i}] returned null");
        blocks.push(p);
    }
    let base = unsafe { (*heap).dbg_segment_base_of_ptr(blocks[0]) };
    for (i, &b) in blocks.iter().enumerate() {
        let b_base = unsafe { (*heap).dbg_segment_base_of_ptr(b) };
        assert_eq!(
            b_base, base,
            "block {i} landed in a different segment — the one-segment assumption broke"
        );
    }
    let p_stuck = blocks[0];

    let _overflow_before = DBG_RING_OVERFLOW.load(Ordering::Relaxed);
    let retried_before = DBG_RING_PUSH_RETRIED.load(Ordering::Relaxed);
    let exhausted_before = DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed);

    // Fill the segment's ring so every fill-free's FIRST ring push fails.
    for _ in 0..RING_CAP {
        let ok = unsafe { (*heap).dbg_push_to_ring(blocks[1], FILL_CLASS) };
        assert!(ok, "fill push failed before reaching RING_CAP");
    }
    let ok = unsafe { (*heap).dbg_push_to_ring(blocks[1], FILL_CLASS) };
    assert!(!ok, "ring should be full after RING_CAP pushes");
    // The failing probe ticked once; re-baseline past it.
    let overflow_after_fill = DBG_RING_OVERFLOW.load(Ordering::Relaxed);

    let fill_addrs: Vec<usize> = blocks[1..].iter().map(|&p| p as usize).collect();
    let stuck_addr = p_stuck as usize;
    let producer = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "producer HeapRegistry::claim failed");
        // FILL frees into the full ring: each ticks DBG_RING_OVERFLOW once
        // and lands in HeapOverflow — filling it to EXACTLY its cap.
        for &addr in &fill_addrs {
            // SAFETY: live owner-allocated block, freed exactly once, from a
            // different thread (foreign → dealloc_foreign_slow).
            unsafe { (*remote).dealloc(addr as *mut u8, layout) };
        }
        // HeapOverflow is now exactly full. This free fails its ring push
        // (tick FILL+1), fails the immediate overflow attempt (ring full),
        // and enters the bounded spin-retry — where it stays until the owner
        // drains below.
        // SAFETY: as above.
        unsafe { (*remote).dealloc(stuck_addr as *mut u8, layout) };
        unsafe { HeapRegistry::recycle(remote) };
    });

    // Wait until p_stuck's ring push has ALREADY failed (tick FILL+1). The
    // producer's overflow attempt is only a few instructions behind its tick,
    // and HeapOverflow is still full at that moment (this thread has not
    // drained anything), so by the time this poll observes the tick the
    // producer is in — or a handful of instructions from — the spin-retry
    // loop. The 50 ms sleep adds margin for pathological preemption inside
    // that window (same calibrated-timing class as
    // tests/regression_paused_owner_wallclock.rs).
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let delta = DBG_RING_OVERFLOW.load(Ordering::Relaxed) - overflow_after_fill;
        if delta >= (FILL + 1) as u64 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "producer never reached the double-saturation point (delta={delta})"
        );
        std::hint::spin_loop();
        thread::yield_now();
    }
    thread::sleep(Duration::from_millis(50));

    // Drain: the first magazine miss → refill_magazine_slow →
    // drain_heap_overflow, which reclaims all FILL overflow entries and
    // gives the retried push room on its next poll. `refill_n + 8` allocs
    // guarantees a magazine miss regardless of the magazine's state.
    let refill_n = unsafe { (*heap).dbg_refill_n_for_class(FILL_CLASS) };
    let mut issued: Vec<*mut u8> = Vec::with_capacity(refill_n + 8);
    for _ in 0..(refill_n + 8) {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "drain alloc returned null");
        issued.push(p);
    }

    producer
        .join()
        .expect("producer thread must not panic — the retried push never landed");

    let overflow_delta = DBG_RING_OVERFLOW.load(Ordering::Relaxed) - overflow_after_fill;
    let retried_delta = DBG_RING_PUSH_RETRIED.load(Ordering::Relaxed) - retried_before;
    let exhausted_delta = DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed) - exhausted_before;

    assert_eq!(
        overflow_delta,
        (FILL + 1) as u64,
        "each double-saturated free must tick DBG_RING_OVERFLOW exactly once \
         (FILL rescues + p_stuck's first-tier miss)"
    );
    assert_eq!(
        retried_delta, 1,
        "p_stuck's free must complete via the bounded spin-retry \
         (DBG_RING_PUSH_RETRIED ticks exactly once on a successful retry) — \
         if this is 0, HeapOverflow was not full and the retry tier was \
         never entered"
    );
    assert_eq!(
        exhausted_delta, 0,
        "a retry-SUCCESS must never tick the terminal-loss counter \
         (DBG_RING_PUSH_RETRY_EXHAUSTED / cross_thread_frees_lost)"
    );

    // Cleanup. All `blocks` lives were freed by the producer (fill blocks +
    // p_stuck); `issued` holds reissued lives (freed once each, here). The
    // RING_CAP stale double-free notes stay in the segment ring undrained —
    // harmless (defensively rejected at any future drain; the same residue
    // other recycled-heap tests leave behind).
    for &p in &issued {
        // SAFETY: live reissued allocation owned by this heap; freed once.
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}
