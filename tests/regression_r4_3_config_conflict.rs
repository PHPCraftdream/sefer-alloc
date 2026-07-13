//! Task #95 / R4-3 (N2) — config-conflict detection on recycled heap slots.
//!
//! RED→GREEN proof for the detect-and-signal fix in
//! `HeapRegistry::claim_with_config` (task #95 / N2). Source:
//! `docs/agent_reviews_round4/performance_review.md` finding N2.
//!
//! ## What the test proves
//!
//! When a recycled, already-materialised slot is re-claimed with a *different*
//! `LargeCacheConfig`, the mismatch is now counted in
//! `AllocStats::config_conflicts` (and surfaced with a `debug_assert!` in
//! debug builds). The slot's existing config still wins (first-
//! materialisation-wins), but the event is no longer fully silent.
//!
//! **RED (before fix):** `claim_with_config` on an already-initialised slot
//! silently ignores the caller's config — no counter, no signal.
//!
//! **GREEN (after fix):** the counter increments; the debug_assert fires in
//! debug builds (caught with `catch_unwind`).
//!
//! Also verifies no false positive: re-claiming with a *matching* config does
//! not trigger the conflict signal.

#![cfg(feature = "alloc-decommit")]

use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::{LargeCacheConfig, SeferAlloc};

/// Two configs that differ in a resolved field (budget_bytes: 64 MiB vs
/// 128 MiB). `live_config_matches` compares resolved values, so these are a
/// genuine mismatch — not a stylistic builder variation.
const CONFIG_A: LargeCacheConfig = LargeCacheConfig::new().budget_bytes(64 * 1024 * 1024);
const CONFIG_B: LargeCacheConfig = LargeCacheConfig::new().budget_bytes(128 * 1024 * 1024);

/// Claim a slot, recycle it, then re-claim with a *different* config. The
/// re-claim reuses the same slot (LIFO `free_slots`), finds it already
/// initialised, and detects the mismatch.
#[test]
fn config_conflict_detected_on_recycled_slot() {
    // 1. Claim + materialise a slot with CONFIG_A.
    let heap1 = HeapRegistry::claim_with_config(CONFIG_A);
    assert!(!heap1.is_null());

    // 2. Recycle it back to the free pool.
    // SAFETY: heap1 was returned by claim_with_config and not yet recycled.
    unsafe { HeapRegistry::recycle(heap1) };

    let before = SeferAlloc::new().stats().config_conflicts;

    // 3. Re-claim — LIFO free_slots means we reuse the same slot. The slot
    //    is already initialised with CONFIG_A; CONFIG_B differs → conflict.
    //    In debug builds the debug_assert fires AFTER the counter is
    //    incremented; catch_unwind handles that so we can still read the
    //    counter.
    let result = std::panic::catch_unwind(|| {
        // claim_with_config copies the config (by value), which is UnwindSafe.
        HeapRegistry::claim_with_config(CONFIG_B)
    });
    // Debug: result is Err (debug_assert panicked). Release: result is Ok.
    if let Ok(heap2) = result {
        // SAFETY: heap2 was returned by claim_with_config; clean up.
        unsafe { HeapRegistry::recycle(heap2) };
    }

    let after = SeferAlloc::new().stats().config_conflicts;
    assert!(
        after > before,
        "config conflict was not counted: before={before}, after={after} \
         (expected the CONFIG_A → CONFIG_B mismatch to increment the counter)"
    );
}

/// Re-claim a recycled slot with the *same* config → no conflict, no
/// debug_assert. This is the false-positive guard: a normal single-config
/// usage pattern must not trip the signal.
#[test]
fn matching_config_does_not_trigger_conflict_signal() {
    let heap1 = HeapRegistry::claim_with_config(CONFIG_A);
    assert!(!heap1.is_null());
    // SAFETY: heap1 was returned by claim_with_config.
    unsafe { HeapRegistry::recycle(heap1) };

    let before = SeferAlloc::new().stats().config_conflicts;

    // Re-claim with the SAME config. This should NOT trigger the debug_assert
    // (in debug builds) and should NOT increment the counter. We use
    // catch_unwind to detect a debug_assert false positive: if it fires,
    // result is Err and we fail with a clear message.
    let result = std::panic::catch_unwind(|| HeapRegistry::claim_with_config(CONFIG_A));
    let heap2 = match result {
        Ok(h) => h,
        Err(panic) => panic!("false positive: debug_assert fired on a matching config: {panic:?}"),
    };
    assert!(!heap2.is_null());

    let after = SeferAlloc::new().stats().config_conflicts;
    assert_eq!(
        before, after,
        "false positive: config conflict counted when configs match"
    );

    // SAFETY: heap2 was returned by claim_with_config; clean up.
    unsafe { HeapRegistry::recycle(heap2) };
}
