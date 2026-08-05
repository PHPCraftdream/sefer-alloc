//! Regression (R34-15/task #534, release-stabilization finding F-4 [medium]):
//! a chunk-materialisation OOM during a cross-thread free must NOT abort the
//! process — the free path (`Registry::slot_or_none`) must return `None` and
//! the caller must bail gracefully through its existing defensive-return
//! branch.
//!
//! ## The bug this covers
//!
//! Before R34-15, `Registry::slot()` was infallible (`&'static HeapSlot`): on
//! chunk-materialisation OOM it called `std::process::abort()` inside
//! `ensure_chunk_slow`. The three free-path callers
//! (`set_dirty_bit_for_segment`, `resolve_heap_overflow`, `owner_slot_is_live`
//! in `heap_core_xthread.rs`) each had a defensive early-return for garbled
//! owner ids, but `slot()` itself was the single point that could still abort
//! — reachable from `SeferAlloc::dealloc` → `dealloc_foreign_routing` →
//! `push_with_overflow_retry` → the three callers above. `GlobalAlloc::dealloc`
//! is infallible by contract, so abort is a legal way to honour it, but too
//! coarse for a stabilized allocator: a transient VM-starvation event during a
//! cross-thread free kills the entire process.
//!
//! ## The fix under test
//!
//! `Registry::slot_or_none` (R34-15) is the fallible variant of `slot()`: it
//! returns `Option<&'static HeapSlot>`, yielding `None` on
//! chunk-materialisation OOM instead of aborting. The three free-path callers
//! now use `slot_or_none`, folding the `None` case into their existing
//! defensive-return branches.
//!
//! ## How OOM is forced
//!
//! Real OOM is not deterministic in a test process. Instead, a test-only
//! `AtomicBool` (`DBG_INJECT_CHUNK_OOM`, `internals`-gated) makes
//! `ensure_chunk_slow`'s winner closure return `None` WITHOUT calling
//! `leak_zeroed_pages` — simulating the OS refusing the VM reservation. The
//! flag is process-global; a `Drop` guard clears it unconditionally (even on
//! panic) so subsequent tests are unaffected.
//!
//! ## Non-vacuousness (counterfactual)
//!
//! Before R34-15, the code path under test (`slot()` → `ensure_chunk_slow` →
//! OOM → `abort()`) would have KILLED the test process — the test runner
//! would observe a non-zero exit code, not an assertion failure. After
//! R34-15, `slot_or_none` returns `None` and the process survives. This test
//! proves the graceful return by ASSERTING `dbg_slot_or_none` returns `false`
//! (the slot was not resolved) AND the process is still alive (the assertion
//! itself ran). The counterfactual is structural: the old `slot()` returned
//! `&'static HeapSlot` (no `None` possible) and aborted on OOM — a return of
//! `false` from this hook is only possible BECAUSE the fix replaced the abort
//! with a `None` return.
//!
//! ## Race safety
//!
//! The test targets a slot index in the LAST chunk (`MAX_HEAPS - 1`), whose
//! chunk is never materialised by ordinary `claim()` traffic in this test
//! suite (the whole suite's cumulative claims stay in the low hundreds at
//! most — see `regression_bootstrap_oom_sentinel_rollback.rs`'s identical
//! reasoning). The OOM-injection flag is set and cleared within a single
//! `#[test]` fn with no spawned threads, so no concurrent materialisation can
//! race with the flag.

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::sync::Mutex;

use sefer_alloc::registry::bootstrap;

// `DBG_INJECT_CHUNK_OOM` is a PROCESS-WIDE `internals`-gated `AtomicBool`.
// `cargo test` runs the two `#[test]` fns below in parallel by default, but
// their correctness relies on sequential execution (this file's own module
// doc: `oom_injection_flag_is_clean_after_test` is designed to run AFTER
// `chunk_oom_on_free_path_returns_gracefully_not_abort`, verifying its
// `OomInjectionGuard` cleared the flag). Found via CI run 31045983765 on
// landing SHA `42d4206` (task #621): a race window between
// `dbg_set_inject_chunk_oom(true)` and the guard's `Drop` clearing it back
// to `false` let the second test observe the flag stuck `true` — the SAME
// process-wide-diagnostic parallelism-artifact class already fixed twice
// this session (`docs/CORRECTNESS_OPEN_ITEMS.md` items 1/25/26). Serializes
// with the SAME established `static TEST_LOCK: Mutex<()>` + per-test
// `let _guard = TEST_LOCK.lock().unwrap();` pattern used in
// `tests/segment_table_contains_base_tier1_counters.rs` and
// `tests/r31_10_trim_current_thread_api.rs`.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard that clears the OOM-injection flag on drop (even on panic),
/// so a test failure cannot leave the flag set for subsequent tests.
struct OomInjectionGuard;
impl Drop for OomInjectionGuard {
    fn drop(&mut self) {
        bootstrap::dbg_set_inject_chunk_oom(false);
    }
}

/// Chunk-materialisation OOM on the free path must return `None` (graceful),
/// not abort the process (R34-15/task #534).
#[test]
fn chunk_oom_on_free_path_returns_gracefully_not_abort() {
    let _lock_guard = TEST_LOCK.lock().unwrap();
    // Pick a slot index in the LAST chunk — guaranteed unmaterialised by
    // ordinary test traffic (see module doc "Race safety").
    let last_chunk = bootstrap::dbg_num_chunks() - 1;
    // A slot index inside the last chunk (first slot of that chunk).
    // CHUNK_SLOTS is pub(crate); but MAX_HEAPS-1 is the last slot overall,
    // guaranteed to be in the last chunk.
    let high_idx = bootstrap::MAX_HEAPS - 1;

    // Sanity: verify the chunk is NOT already materialised (if it were, the
    // fast path in try_ensure_chunk would succeed without hitting the closure
    // and the OOM injection would be a no-op — making the test vacuous).
    let reg = bootstrap::ensure();
    assert!(
        !reg.dbg_chunk_is_materialised(last_chunk),
        "precondition: last chunk must not be materialised by prior test traffic"
    );

    // ── With OOM injection ON: slot_or_none must return false (None) ────
    let _guard = OomInjectionGuard;
    bootstrap::dbg_set_inject_chunk_oom(true);

    let result = bootstrap::dbg_slot_or_none(high_idx);
    assert!(
        !result,
        "slot_or_none must return None on chunk-materialisation OOM — \
         if this assertion runs at all, the process did NOT abort (the \
         pre-R34-15 behaviour). A `false` return is only possible because \
         the fix replaced the abort with a graceful None return."
    );

    // The chunk must STILL not be materialised (the OOM-bailout rolled the
    // sentinel back to null — anti-livelock, same property
    // regression_bootstrap_oom_sentinel_rollback.rs verifies independently).
    assert!(
        !reg.dbg_chunk_is_materialised(last_chunk),
        "chunk must not be materialised after an OOM bailout"
    );

    // ── With OOM injection OFF: slot_or_none must succeed (Some) ────────
    // Drop the guard (clears the flag) and re-test: the same chunk index
    // should now materialise normally, proving the OOM bailout did not
    // permanently damage the registry.
    drop(_guard);
    let result = bootstrap::dbg_slot_or_none(high_idx);
    assert!(
        result,
        "slot_or_none must succeed after OOM injection is cleared — \
         the chunk must materialise normally on retry (anti-livelock)"
    );

    // And now the chunk IS materialised.
    assert!(
        reg.dbg_chunk_is_materialised(last_chunk),
        "chunk must be materialised after a successful (non-OOM) resolution"
    );
}

/// The OOM-injection flag must be clean (false) on entry and after the test —
/// a leaked `true` would break every subsequent test that touches an
/// unmaterialised chunk. This test runs FIRST (alphabetical ordering) and
/// asserts the flag is clean, then the main test runs, then this test's
/// Drop-equivalent is the OomInjectionGuard inside the main test. This
/// separate test provides a second layer of confidence: run after the main
/// test (by name, `chunk_oom_...` sorts before `oom_injection_flag_...`).
#[test]
fn oom_injection_flag_is_clean_after_test() {
    let _lock_guard = TEST_LOCK.lock().unwrap();
    // The main test's OomInjectionGuard cleared the flag on drop. This
    // assertion verifies that: if the flag were still `true`, calling
    // slot_or_none on an unmaterialised chunk would return false (the
    // injected OOM). We pick a DIFFERENT chunk than the main test used
    // (second-to-last) so this is not confused by the main test's
    // successful materialisation of the last chunk.
    let second_last_chunk = bootstrap::dbg_num_chunks() - 2;
    let reg = bootstrap::ensure();

    // Only run if this chunk hasn't been materialised (it may have been by
    // some other test in the suite). If it IS already materialised, the
    // fast path succeeds regardless of the flag, so the check is vacuous —
    // skip it.
    if !reg.dbg_chunk_is_materialised(second_last_chunk) {
        let idx = (second_last_chunk * (bootstrap::MAX_HEAPS / bootstrap::dbg_num_chunks()))
            .min(bootstrap::MAX_HEAPS - 1);
        let result = bootstrap::dbg_slot_or_none(idx);
        assert!(
            result,
            "slot_or_none must succeed on an unmaterialised chunk when the \
             OOM-injection flag is clean — if this fails, a prior test leaked \
             the flag set (DBG_INJECT_CHUNK_OOM stuck at true)"
        );
    }
}
