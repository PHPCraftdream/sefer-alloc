#![cfg(all(
    feature = "production",
    feature = "internals",
    not(feature = "numa-aware")
))]
// R13-01 regression: `directory_words_examined` must count EVERY u64 word
// inspected by the per-class directory bitmap scan — including the
// all-zero words the pre-fix code skipped after its `bits == 0` `continue`,
// so a negative lookup over an empty materialized bitmap reported a zero
// delta. Increments are `alloc-stats`-gated: this file compiles with and
// without the feature and asserts delta == inspected words (on) / delta
// == 0 (off), with identical search outcomes. Setup mirrors
// tests/r12_03_routed_directory_negative.rs on a plain non-routed owner
// core — the counter is exercised by the shared directory scan, not routed
// trust-negative semantics.

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

/// One `#[test]` fn on purpose: the three scenarios share the
/// process-global diagnostic counter, so they run sequentially inside a
/// single test with a delta snapshot taken around each probed call (no
/// same-binary cross-test races, independent of `--test-threads`).
#[test]
fn words_examined_counts_every_inspected_directory_word() {
    // ── Scenario A: absent directory ────────────────────────────────────
    // A fresh core has no materialised sidecar: the class-0 lookup must
    // attribute no word examinations. This distinguishes "no directory"
    // from "directory with an all-zero bitmap" (scenario B). The search
    // OUTCOME is deliberately not asserted here — bootstrap primordial
    // state is not this slice's contract; only the counter attribution is.
    let mut core = AllocCore::new().expect("core");
    assert!(
        !core.dbg_directory_is_materialised(),
        "fresh core must not start with a materialised directory"
    );
    let before = AllocCore::dbg_directory_words_examined();
    let _ = core.dbg_find_segment_with_free(0);
    assert_eq!(
        AllocCore::dbg_directory_words_examined() - before,
        0,
        "absent directory must examine no words"
    );

    // ── Scenario B: materialised directory, all-zero class-0 bitmap ─────
    // Allocate distinct largest-small-class roots until the sidecar is
    // materialised, without populating class zero. The negative class-0
    // lookup walks the whole word array; every zero word must be counted
    // (delta == words per class, single bucket with `numa-aware` off). On
    // the pre-fix code this delta was 0 — this assertion is the regression
    // oracle and FAILS without the fix.
    let large_class = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let mut large_class_live = Vec::new();
    for _ in 0..4096 {
        let ptr = core.alloc(large_class);
        assert!(!ptr.is_null(), "setup allocation failed");
        large_class_live.push(ptr);
        if core.dbg_directory_is_materialised() {
            break;
        }
    }
    assert!(
        core.dbg_directory_is_materialised(),
        "setup crosses directory threshold"
    );
    let slots = core.dbg_table_count() as usize;
    for slot in 0..slots {
        assert_eq!(
            core.dbg_directory_get_bit(0, slot),
            Some(false),
            "class-0 bit must be unset for every registered slot"
        );
    }
    core.dbg_directory_reset_miss_streak();
    #[cfg(feature = "alloc-stats")]
    let words_per_class =
        u64::try_from(AllocCore::dbg_words_per_class()).expect("bounded word count fits u64");
    let before = AllocCore::dbg_directory_words_examined();
    assert!(
        core.dbg_find_segment_with_free(0).is_none(),
        "all-zero class-0 bitmap must yield a negative lookup"
    );
    let empty_delta = AllocCore::dbg_directory_words_examined() - before;
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        empty_delta, words_per_class,
        "empty materialized bitmap must count every inspected word"
    );
    #[cfg(not(feature = "alloc-stats"))]
    assert_eq!(
        empty_delta, 0,
        "with alloc-stats off the counter must read 0"
    );

    // ── Scenario C: nonempty bitmap, early return at the set word ───────
    // Two class-0 blocks land on one fresh root; freeing one publishes its
    // root's bit while the other stays live (root not released). The
    // positive lookup must stop at that word: delta == target word index
    // + 1, with every earlier word verified zero — the scan returns at the
    // winning word instead of walking on.
    let class_zero = Layout::from_size_align(AllocCore::dbg_block_size(0), 8).unwrap();
    let a = core.alloc(class_zero);
    let b = core.alloc(class_zero);
    assert!(
        !a.is_null() && !b.is_null(),
        "class-0 setup allocation failed"
    );
    assert_eq!(
        SegmentLayout::segment_base_of(a as usize),
        SegmentLayout::segment_base_of(b as usize),
        "back-to-back allocations should share a root"
    );
    // SAFETY: `a` is a live class-zero allocation, freed exactly once.
    unsafe { core.dealloc(a, class_zero) };
    let target_slot = core.dbg_segment_id_of(a) as usize;
    assert_eq!(
        core.dbg_directory_get_bit(0, target_slot),
        Some(true),
        "freed class-0 block must publish its root's bit"
    );
    let target_word = target_slot / 64;
    for slot in 0..target_word * 64 {
        assert_eq!(
            core.dbg_directory_get_bit(0, slot),
            Some(false),
            "slot {slot} precedes the set word and must be unset"
        );
    }
    core.dbg_directory_reset_miss_streak();
    let before = AllocCore::dbg_directory_words_examined();
    let found = core.dbg_find_segment_with_free(0);
    assert!(found.is_some(), "published class-0 bit must validate");
    let hit_delta = AllocCore::dbg_directory_words_examined() - before;
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        hit_delta,
        u64::try_from(target_word + 1).expect("bounded word count fits u64"),
        "positive lookup must count the winning word and stop there"
    );
    #[cfg(not(feature = "alloc-stats"))]
    assert_eq!(hit_delta, 0, "with alloc-stats off the counter must read 0");

    // ── Teardown: release the remaining live allocations. ───────────────
    for ptr in large_class_live {
        // SAFETY: each pointer is live, distinct, and freed once here.
        unsafe { core.dealloc(ptr, large_class) };
    }
    // SAFETY: `b` is a live class-zero allocation, freed exactly once.
    unsafe { core.dealloc(b, class_zero) };
}
