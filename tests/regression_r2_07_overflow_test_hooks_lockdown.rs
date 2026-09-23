//! R2-07 (independent src review round 2, task #2009) — the `HeapOverflow`
//! test-hook lockdown regression.
//!
//! Before this fix, two `#[doc(hidden)] pub` test hooks on `HeapOverflow`
//! could leave a reachable cursor pointing at unpublished/null sidecar
//! state, reachable under plain `alloc-global + alloc-xthread + internals`
//! (no `bench-internals` needed):
//!
//! 1. `dbg_reserve_unpublished_for_test` took `&self` and bounded its
//!    reservation to the always-inline tier (`t < INLINE_CAP`) with only a
//!    `debug_assert!` — compiled OUT of release builds. A release build
//!    could advance `tail` past `INLINE_CAP` with the sidecar left
//!    unmaterialised; the next `drain`/`push` reaching that index would hit
//!    `HeapOverflow::slot`'s OWN precondition check (also only a
//!    `debug_assert!` pre-fix) and, in release, dereference a null pointer
//!    through `bootstrap::deref_overflow_sidecar` — an invalid dereference
//!    reachable from a safe diagnostic hook.
//! 2. `dbg_rollback_sidecar_sentinel_for_test` took `&self` on a type whose
//!    production instances are `Sync` and reachable via a shared
//!    `&'static HeapSlot`; its rollback sequence temporarily installs the
//!    materialisation sentinel and then unconditionally writes `null`,
//!    which — if run concurrently against a REAL `ensure_overflow_sidecar`
//!    caller racing the same ring — could inject a spurious
//!    materialisation failure into live production traffic the probe never
//!    touched.
//!
//! The fix: both hooks now take `&mut self`, obtainable only through
//! [`HeapOverflow::new_boxed_for_test`]'s exclusively-owned standalone
//! `Box` — never through the shared `&'static HeapSlot` production reaches
//! a real ring through — mirroring R2-04's identical `&self` -> `&mut self`
//! lockdown for `EpochRegion`'s test-only generation setter (every existing
//! call site needing a `let mut` added is itself confirmation the tightened
//! signature is load-bearing, not just documentation — see
//! `tests/heap_overflow_drain_return.rs` / `tests/heap_overflow_sidecar.rs`).
//! `dbg_reserve_unpublished_for_test`'s inline-tier bound is now a real
//! `assert!` that fires in EVERY build profile, and `HeapOverflow::slot`'s
//! own precondition check (directly upstream of the `unsafe` sidecar
//! dereference) is likewise now a release-surviving `assert!`, not a
//! `debug_assert!` — defense in depth, since production callers never
//! violate it by construction (the wedge-hazard-safe ordering in
//! `push_impl` already guarantees it unconditionally).
//!
//! This file proves the release-surviving boundary check with a genuine
//! counterfactual: `debug_assert!` is controlled by `cfg(debug_assertions)`,
//! NOT by `cfg(test)` or the dev/release cargo profile alone, so a plain
//! `cargo test` run (which defaults to the dev profile, `debug_assertions =
//! true`) cannot by itself distinguish the pre-fix `debug_assert!` from the
//! post-fix `assert!` — both fire identically under `debug_assertions =
//! true`. The discriminating run is `cargo test --release` (which disables
//! `debug_assertions` by default): stashing just the `src/` fix and
//! re-running this file's `#[should_panic]` test under `--release`
//! reproduces the pre-fix silent-corruption path (the test FAILS to panic);
//! restoring the fix makes it panic again under `--release` too. See the
//! commit message for the exact counterfactual log.

#![cfg(all(feature = "alloc-xthread", feature = "internals"))]

use sefer_alloc::registry::heap_overflow::HeapOverflow;

/// `dbg_reserve_unpublished_for_test` must reject (panic on) an attempt to
/// reserve past the always-inline tier, in EVERY build profile — the R2-07
/// discriminator. Fills and drains exactly `INLINE_CAP` entries (leaving
/// `tail == INLINE_CAP`, the first index that needs the sidecar), then
/// attempts one more unpublished reservation, which must now panic instead
/// of silently advancing `tail` into unmaterialised sidecar territory.
#[test]
#[should_panic(expected = "only supports reserving within the always-inline tier")]
fn dbg_reserve_unpublished_for_test_rejects_sidecar_range_in_every_profile() {
    let mut ring = HeapOverflow::new_boxed_for_test();
    let inline_cap = ring.dbg_fill_and_drain_inline_tier_for_test();
    assert!(inline_cap > 0, "INLINE_CAP must be positive for this test to be meaningful");

    // tail == INLINE_CAP now: the first sidecar-range index. Pre-fix, this
    // call would silently succeed in a release build (debug_assert! compiled
    // out), advancing tail to INLINE_CAP + 1 with no backing sidecar entry.
    ring.dbg_reserve_unpublished_for_test();
}

/// Sanity floor: the hook still works (does not over-reject) for every
/// index genuinely within the always-inline tier — confirming the R2-07
/// fix tightened only the OUT-OF-BOUNDS case, not the sanctioned one.
#[test]
fn dbg_reserve_unpublished_for_test_still_works_within_inline_tier() {
    let mut ring = HeapOverflow::new_boxed_for_test();
    // Reserve (without publishing) a single slot at tail == 0 -- squarely
    // inside the always-inline tier.
    ring.dbg_reserve_unpublished_for_test();

    let mut reclaimed = 0u32;
    let stop = ring.try_drain(|_, _| reclaimed += 1);
    assert_eq!(reclaimed, 0, "the reserved-but-unpublished slot must not be reclaimed");
    assert_eq!(stop, Some(0), "drain must stop at the unpublished slot (index 0)");
}
