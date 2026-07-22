//! R12-6 (P1) regression: `drain_heap_overflow`'s `EMPTIED_BASES_CAP = 64`
//! on-stack dedup buffer must not silently drop finalization for the 65th+
//! DISTINCT segment that goes fully empty via `HeapOverflow` reclaims in a
//! single drain pass.
//!
//! **Context.** `HeapCore::drain_heap_overflow` collects the bases of every
//! segment that reaches `live_count == 0` during one drain into a fixed
//! `[*mut u8; EMPTIED_BASES_CAP]` (64) on-stack array, then finalizes
//! (pools/releases) each collected base AFTER the drain returns (R11-2 Bug
//! 2 — see `tests/r11_2_overflow_drain_pool_release.rs`). Native's
//! `HeapOverflow` ring holds up to `HEAP_OVERFLOW_CAP = 2048` entries, so a
//! single drain pass can genuinely empty more than 64 DISTINCT segments (one
//! entry per segment's last live block). Before R12-6, the 65th+ distinct
//! base was simply not collected into the buffer AND never finalized this
//! pass — the segment reached `live_count == 0`, stayed correctly usable
//! (its BinTable is populated, so `find_segment_with_free` can still find
//! and reuse it — nothing is leaked or corrupted), but sat outside the
//! pool-cap accounting and at inflated RSS/commit indefinitely (until some
//! unrelated future event happened to empty it again through a normal,
//! non-overflow path).
//!
//! **Fix.** `drain_heap_overflow` now tracks whether the dedup buffer
//! actually overflowed (`emptied_overflowed`); if so, a single rare
//! post-drain fallback sweep (`AllocCore::finalize_orphaned_empty_segments`)
//! scans every registered segment via the SAME index-driven `table.base_at`
//! idiom `find_segment_with_free_impl`'s linear-scan fallback already uses,
//! and finalizes any `Small`, non-`small_cur`, non-decommitted, non-pooled
//! segment sitting at `live_count == 0` — catching every base the fixed-size
//! buffer had no room for.
//!
//! **Counterfactual test.** Construct `SEGMENT_COUNT` (66 — safely more than
//! `EMPTIED_BASES_CAP` = 64) DISTINCT small segments of `TARGET_CLASS`, each
//! driven down to exactly ONE live block, that block's segment ring filled
//! to capacity, then that LAST live block of EVERY one of the 66 segments
//! freed CROSS-THREAD (so every one of the 66 final frees overflows into
//! `HeapOverflow`, since each segment's own ring is already full). A single
//! trigger allocation on an unrelated class then drains the entire
//! `HeapOverflow` ring in one pass, reclaiming all 66 overflow entries and
//! bringing all 66 segments to `live_count == 0` in the SAME
//! `drain_heap_overflow` call — exceeding `EMPTIED_BASES_CAP` by 2 distinct
//! bases.
//!
//! Under the buggy pre-R12-6 code, only the first 64 of the 66 emptied
//! segments are finalized (pooled) this pass; the last 2 are left as
//! ordinary registered segments (`live_count == 0` but NOT pooled), so the
//! aggregate `dbg_pooled_count` delta this test can observe is capped at 64
//! by construction. Under the fixed code, all 66 are finalized (pooled) —
//! the delta reaches `SEGMENT_COUNT` (66), which is IMPOSSIBLE under the
//! buggy code.
//!
//! **Feature gate.** `alloc-global`, `alloc-xthread`, `fastbin`,
//! `alloc-decommit` (gates `dec_live_and_maybe_decommit` /
//! `release_or_pool_empty_segment` / `finalize_orphaned_empty_segments`
//! themselves), `alloc-stats` (production bundle). Under other
//! configurations the file compiles as an empty test binary (0 tests, pass
//! by absence).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "alloc-decommit"
))]

extern crate sefer_alloc;

use std::alloc::Layout;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};
use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

// Serialise against other tests in this binary: the registry is a
// process-global static shared across every HeapCore/HeapCore in the process.
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

/// The class the OWNER allocates and frees to construct the target segments.
/// Class 48 (block_size ~= 258752 B, ~16 blocks/4 MiB segment) is the
/// largest small class under `production` — chosen specifically to minimise
/// the total number of blocks needed to span `SEGMENT_COUNT` distinct
/// segments (this test needs ~16 blocks/segment rather than the ~96-120 a
/// mid-range class like 40/41 would require). Its block_size is far above
/// `REFILL_BYTE_BUDGET/2` (32768), so `refill_n_for_class == 1` — the
/// magazine holds at most 1 block at a time, matching the simplifying
/// precondition the sibling R11-2 tests rely on.
const TARGET_CLASS: usize = 48;

/// A SEPARATE class, touched by nothing else in this test, used ONLY to
/// trigger `refill_magazine_slow` -> `drain_heap_overflow` without that same
/// refill also reaching into one of the just-emptied TARGET_CLASS segments
/// (mirrors `tests/r11_2_overflow_drain_pool_release.rs`'s `TRIGGER_CLASS`
/// rationale exactly).
const TRIGGER_CLASS: usize = 47;

/// Number of distinct target segments to construct — strictly greater than
/// `EMPTIED_BASES_CAP` (64, private to `heap_core_xthread.rs`) so at least 2
/// segments exceed the dedup buffer in one drain pass.
const SEGMENT_COUNT: usize = 66;

/// Minimum blocks a target segment must hold for this scenario (>=2: at
/// least one own-thread-freed "drain-down" block plus the final
/// overflow-routed block).
const MIN_BLOCKS_PER_SEGMENT: usize = 2;

/// Capacity of the per-segment RemoteFreeRing. Must match
/// `RemoteFreeRing::RING_CAP` (256) — filled per target segment so each
/// segment's final cross-thread free overflows into `HeapOverflow`.
const RING_CAP: usize = 256;

fn alloc_one(heap: *mut HeapCore, class_idx: usize) -> *mut u8 {
    let bs = AllocCore::dbg_block_size(class_idx);
    let layout = Layout::from_size_align(bs, 8).expect("class block size is a valid layout");
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null(), "alloc returned null for class {class_idx}");
    p
}

/// Regression test for R12-6: `drain_heap_overflow`'s post-drain finalization
/// must cover every distinct emptied segment in a pass, not just the first
/// `EMPTIED_BASES_CAP` (64).
#[test]
fn overflow_drain_finalizes_beyond_emptied_bases_cap() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // Pool sized generously above SEGMENT_COUNT so every finalized segment
    // this test empties is admitted to the pool (observable via
    // `dbg_pooled_count`), not released — keeps the assertion a simple
    // monotonic counter check rather than needing to distinguish
    // pooled-vs-released per base.
    let config = LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(SEGMENT_COUNT + 16)
            .pool_byte_cap((SEGMENT_COUNT as u64 + 16) as usize * 4 * 1024 * 1024),
    );
    let heap = HeapRegistry::claim_with_config(config);
    assert!(
        !heap.is_null(),
        "HeapRegistry::claim_with_config returned null"
    );

    let refill_n = unsafe { (*heap).dbg_refill_n_for_class(TARGET_CLASS) };
    assert_eq!(
        refill_n, 1,
        "TARGET_CLASS must have refill_n=1 for this test's magazine reasoning"
    );

    let bs = AllocCore::dbg_block_size(TARGET_CLASS);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS layout");

    let pooled_before = unsafe { (*heap).dbg_pooled_count() };

    // Phase 1: allocate TARGET_CLASS blocks, grouping them by segment base,
    // until at least SEGMENT_COUNT distinct bases (excluding small_cur, the
    // current carve target, which is never finalized) each hold at least
    // MIN_BLOCKS_PER_SEGMENT blocks. This greedy grouping needs no
    // assumption about exactly when the carve target rolls over to a new
    // segment — it simply keeps allocating and bucketing by observed base
    // until the target segment count/depth is reached, then leaves any
    // partially-filled trailing segment or the live small_cur bucket alone
    // (freed at teardown).
    let mut by_base: HashMap<usize, Vec<*mut u8>> = HashMap::new();
    // Collect SPARE qualifying segments beyond SEGMENT_COUNT, to make room
    // for two exclusions:
    //   1. The segment that is the LIVE carve target (`small_cur`) at the
    //      moment allocation stops must be excluded (it can never be
    //      finalized — the fixed-size dedup array AND the fallback sweep
    //      both correctly skip `base == small_cur`, mirroring
    //      `dec_live_and_maybe_decommit`'s own exclusion), but WHICH base
    //      that is is only known once the loop below stops allocating (the
    //      initial `small_cur` snapshot goes stale the moment a fresh
    //      segment is carved).
    //   2. The PRIMORDIAL segment (kind tag `0`, see `dbg_kind_at_tag`) is
    //      ALSO never pool/release-eligible (same exclusion, for a different
    //      reason — see `dec_live_and_maybe_decommit`'s doc comment on why
    //      the primordial segment's payload is never decommitted). It can
    //      legitimately host TARGET_CLASS blocks (it has spare non-metadata
    //      capacity) and so can end up as one of the qualifying buckets;
    //      `dbg_live_count_for` returns `Some` for it too (Primordial is in
    //      its `Small | Primordial` match arm), so a naive `live_count == 0`
    //      check cannot distinguish it from a genuinely finalizable segment
    //      — it must be filtered out explicitly by kind.
    // Over-collecting by two and filtering against BOTH the CURRENT carve
    // target and the primordial kind right before Phase 2 (not a stale
    // pre-loop snapshot or an allocation-order assumption) avoids ever
    // depending on incidental construction details.
    const SPARE: usize = 2;
    loop {
        let p = alloc_one(heap, TARGET_CLASS);
        let base = unsafe { (*heap).dbg_segment_base_of_ptr(p) } as usize;
        by_base.entry(base).or_default().push(p);

        let qualifying = by_base
            .iter()
            .filter(|(_, v)| v.len() >= MIN_BLOCKS_PER_SEGMENT)
            .count();
        if qualifying >= SEGMENT_COUNT + SPARE {
            break;
        }
        // Safety valve: this class carves a fresh segment roughly every ~16
        // blocks, so SEGMENT_COUNT segments need on the order of
        // SEGMENT_COUNT * 20 allocations; bail out loudly instead of
        // spinning forever if size-class layout assumptions ever drift.
        let total: usize = by_base.values().map(Vec::len).sum();
        assert!(
            total < (SEGMENT_COUNT + SPARE) * 64,
            "allocated {total} TARGET_CLASS blocks without reaching \
             {SEGMENT_COUNT} qualifying segments — size-class layout \
             assumption drifted"
        );
    }

    // Re-read the carve target NOW (post-loop) — the ONLY segment that can
    // still be `small_cur` by the time `drain_heap_overflow` runs later, and
    // therefore the one base that must be excluded from the target set.
    let live_small_cur = unsafe { (*heap).dbg_last_stamped_segment() } as usize;

    // Select exactly SEGMENT_COUNT qualifying bases, excluding the live
    // carve target AND the primordial segment (kind tag `0` — see the SPARE
    // comment above; `dbg_kind_at_tag` requires a live pointer INTO the
    // segment, so probe with any already-collected block of that base rather
    // than the bare base address). Deterministic order via sorted keys, so
    // the test is reproducible across runs.
    let mut target_bases: Vec<usize> = by_base
        .iter()
        .filter(|(&b, v)| {
            b != live_small_cur
                && v.len() >= MIN_BLOCKS_PER_SEGMENT
                && unsafe { (*heap).dbg_kind_at_tag(v[0]) } != 0
        })
        .map(|(&b, _)| b)
        .collect();
    target_bases.sort_unstable();
    assert!(
        target_bases.len() >= SEGMENT_COUNT,
        "expected at least {SEGMENT_COUNT} qualifying non-carve-target, \
         non-primordial segments, got {}",
        target_bases.len()
    );
    target_bases.truncate(SEGMENT_COUNT);
    assert_eq!(target_bases.len(), SEGMENT_COUNT);

    // Phase 2: for each target segment, drive it down to exactly ONE live
    // block, fill its ring, and reserve that last block for the Phase 3
    // cross-thread free. Blocks in non-selected buckets (small_cur's, or any
    // extra qualifying segments beyond SEGMENT_COUNT) are left alone and
    // freed at teardown.
    let mut last_live: Vec<usize> = Vec::with_capacity(SEGMENT_COUNT);
    for &base_addr in &target_bases {
        let base = base_addr as *mut u8;
        let blocks = by_base.get(&base_addr).unwrap().clone();

        let live_before = unsafe { (*heap).dbg_live_count_for(base) }
            .expect("dbg_live_count_for must resolve a live small/primordial segment");
        assert_eq!(
            live_before as usize,
            blocks.len(),
            "every live block of base must be one we allocated for it \
             (no other allocation touched this segment)"
        );

        // Reserve the LAST block as the final cross-thread free that must
        // route through HeapOverflow; free every other block of this
        // segment own-thread.
        let (rest, p_overflow) = blocks.split_at(blocks.len() - 1);
        let p_overflow = p_overflow[0];
        for &p in rest {
            unsafe { (*heap).dealloc(p, layout) };
        }
        unsafe { (*heap).dbg_flush_all() };

        let live_mid = unsafe { (*heap).dbg_live_count_for(base) }
            .expect("target segment must still be registered (not yet empty)");
        assert_eq!(
            live_mid, 1,
            "target segment must have exactly 1 live block left (p_overflow)"
        );

        // Fill this segment's RemoteFreeRing with RING_CAP double-free
        // entries targeting an already-freed block of this segment.
        let ring_filler = rest[0];
        for _ in 0..RING_CAP {
            let ok = unsafe { (*heap).dbg_push_to_ring(ring_filler, TARGET_CLASS) };
            assert!(ok, "dbg_push_to_ring failed before reaching RING_CAP");
        }
        let overflow_check = unsafe { (*heap).dbg_push_to_ring(ring_filler, TARGET_CLASS) };
        assert!(!overflow_check, "ring should be full after RING_CAP pushes");

        last_live.push(p_overflow as usize);
    }

    // Phase 3: cross-thread free every reserved "last live block" (one per
    // target segment) from a producer thread. Each segment's ring is
    // already full, so every one of these SEGMENT_COUNT frees routes into
    // HeapOverflow -- filling it with SEGMENT_COUNT entries targeting
    // SEGMENT_COUNT DISTINCT bases, each of which will reach live_count == 0
    // once its entry is reclaimed.
    let addrs = last_live.clone();
    let producer = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "producer HeapRegistry::claim failed");
        for x_addr in addrs {
            // SAFETY (R6-MS-1/2 + raw-deref): each `x_addr` is a block
            // previously allocated by `heap` (the owner) and still live at
            // this point (reserved, never freed until now). This dealloc
            // from a DIFFERENT thread routes through `dealloc_foreign_slow`
            // -> `push_with_overflow_retry`, which finds the segment's own
            // ring full and pushes into HeapOverflow.
            unsafe { (*remote).dealloc(x_addr as *mut u8, layout) };
        }
        unsafe { HeapRegistry::recycle(remote) };
    });
    producer.join().expect("producer thread must not panic");

    // Phase 4: allocate one block of TRIGGER_CLASS (untouched by anything
    // else in this test). This is a genuine magazine MISS ->
    // `refill_magazine_slow` -> `drain_heap_overflow`, which drains the
    // ENTIRE HeapOverflow ring (all SEGMENT_COUNT entries) in ONE pass --
    // reclaiming every target segment's overflow entry and bringing all
    // SEGMENT_COUNT segments down to live_count == 0 in this single drain
    // call, exceeding EMPTIED_BASES_CAP (64) by (SEGMENT_COUNT - 64).
    let trigger_bs = AllocCore::dbg_block_size(TRIGGER_CLASS);
    let trigger_layout = Layout::from_size_align(trigger_bs, 8).expect("TRIGGER_CLASS layout");
    let _trigger = unsafe { (*heap).alloc(trigger_layout) };
    assert!(!_trigger.is_null(), "trigger alloc returned null");

    // Phase 5: verify every one of the SEGMENT_COUNT target segments reached
    // live_count == 0 (the overflow reclaim succeeded for all of them, not
    // just the first 64 -- the drain always finishes reclaiming every ring
    // entry regardless of EMPTIED_BASES_CAP; only FINALIZATION was at risk).
    for &base_addr in &target_bases {
        let base = base_addr as *mut u8;
        let live = unsafe { (*heap).dbg_live_count_for(base) };
        assert_eq!(
            live,
            Some(0),
            "every target segment must reach live_count == 0 after the drain \
             reclaimed its last live block via HeapOverflow"
        );
    }

    // Phase 6: the core R12-6 assertion. Under the buggy pre-fix code, AT
    // MOST EMPTIED_BASES_CAP (64) of the SEGMENT_COUNT (66) emptied segments
    // are finalized this pass (the dedup buffer silently drops the rest) --
    // `dbg_pooled_count` can increase by at most 64. Under the fixed code,
    // the post-drain fallback sweep finalizes every remaining orphaned base
    // too, so the delta reaches SEGMENT_COUNT.
    let pooled_after = unsafe { (*heap).dbg_pooled_count() };
    let delta = pooled_after.saturating_sub(pooled_before);
    assert!(
        delta > 64,
        "pooled_count delta must exceed EMPTIED_BASES_CAP (64) once all \
         {SEGMENT_COUNT} distinct overflow-emptied segments are finalized: \
         pooled_before={pooled_before}, pooled_after={pooled_after}, delta={delta}. \
         Before R12-6, drain_heap_overflow's on-stack dedup buffer silently \
         dropped finalization for the 65th+ distinct emptied segment in a \
         single drain pass, capping the achievable delta at 64 regardless of \
         how many segments actually emptied."
    );
    assert!(
        delta >= SEGMENT_COUNT,
        "pooled_count delta ({delta}) should account for all {SEGMENT_COUNT} \
         target segments finalized by this test (plus any incidental others)"
    );

    // Cleanup: free the TRIGGER_CLASS block and every non-target block
    // (small_cur's live blocks and any extra qualifying-but-unselected
    // segment's blocks). Target segments' blocks were already freed
    // (own-thread drain-down + the cross-thread overflow reclaim) -- must
    // NOT be dealloc'd again.
    unsafe { (*heap).dealloc(_trigger, trigger_layout) };
    let target_set: std::collections::HashSet<usize> = target_bases.iter().copied().collect();
    for (&base_addr, blocks) in &by_base {
        if target_set.contains(&base_addr) {
            continue;
        }
        for &p in blocks {
            unsafe { (*heap).dealloc(p, layout) };
        }
    }
    unsafe { HeapRegistry::recycle(heap) };
}
