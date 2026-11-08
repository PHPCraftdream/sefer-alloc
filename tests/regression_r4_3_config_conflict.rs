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
//! `AllocStats::config_conflicts`. The slot's existing config still wins
//! (first-materialisation-wins), but the event is no longer fully silent.
//! (A `debug_assert!` that used to accompany the counter was removed by
//! R2-08 / task #2010: this is the cold bind path behind every `GlobalAlloc`
//! method, which must never unwind — the strict no-panic check lives in
//! `tests/regression_r2_08_globalalloc_no_unwind.rs`. The `catch_unwind`s
//! below are kept as tolerant harnesses.)
//!
//! **RED (before fix):** `claim_with_config` on an already-initialised slot
//! silently ignores the caller's config — no counter, no signal.
//!
//! **GREEN (after fix):** the counter increments.
//!
//! Also verifies no false positive: re-claiming with a *matching* config does
//! not trigger the conflict signal.

#![cfg(all(feature = "alloc-decommit", feature = "internals"))]

use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::{LargeCacheConfig, SeferAlloc};

// The registry is a process-global static and CONFIG_CONFLICTS is a
// process-wide counter; both tests below rely on LIFO free_slots reuse
// landing them on the SAME slot they just recycled. Running them
// concurrently (cargo test's default) lets one test's claim/recycle
// interleave with the other's, so a slot reused by test A's differently-
// configured claim can spuriously trip test B's "no conflict" assertion.
// Serialize (same established pattern as tests/registry_basic.rs).
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
    let _serial = SerialGuard::acquire();
    // 1. Claim + materialise a slot with CONFIG_A. Capture its slot id so the
    //    R6-CQ-3 leak check below can prove the SAME slot is reclaimable
    //    after the conflict-triggered panic.
    let lease1 = HeapRegistry::dbg_claim_lease_with_config(CONFIG_A)
        .expect("claim_with_config returned None");
    let slot_idx = lease1.slot_index();

    // 2. Recycle it back to the free pool (drop of the lease: LIVE -> FREE).
    drop(lease1);

    let before = SeferAlloc::new().stats().config_conflicts;

    // 3. Re-claim — LIFO free_slots means we reuse the same slot. The slot
    //    is already initialised with CONFIG_A; CONFIG_B differs → conflict.
    //    (Pre-R2-08 a debug_assert fired here after the counter increment;
    //    the tolerant catch_unwind is kept from that era.)
    let result = std::panic::catch_unwind(|| {
        // dbg_claim_lease_with_config copies the config (by value), which is
        // UnwindSafe.
        HeapRegistry::dbg_claim_lease_with_config(CONFIG_B)
    });
    // Post-R2-08: Ok in every profile (strictly asserted in regression_r2_08).
    if let Ok(lease2) = result {
        // Drop of the lease recycles the slot (LIVE -> FREE); clean up.
        drop(lease2);
    }

    let after = SeferAlloc::new().stats().config_conflicts;
    assert!(
        after > before,
        "config conflict was not counted: before={before}, after={after} \
         (expected the CONFIG_A → CONFIG_B mismatch to increment the counter)"
    );

    // 4. R6-CQ-3 (panic-safety / no-leak): the config-conflict signal must
    //    NOT leak the slot. The mismatched re-claim returns normally (every
    //    profile since R2-08; formerly a debug-build panic), and the slot must be
    //    reclaimable on the NEXT claim — and LIFO free_slots reuse means it
    //    is the SAME slot index. Pre-fix this failed in debug builds: the
    //    panic left the slot stuck LIVE and out of free_slots, so the
    //    re-claim minted a different index (or, eventually, exhausted the
    //    registry). See the dedicated `slot_not_leaked_after_config_conflict_panic`
    //    test for the full RED→GREEN counterfactual.
    let lease3 = HeapRegistry::dbg_claim_lease_with_config(CONFIG_A);
    assert!(
        lease3.is_some(),
        "slot leaked: re-claim after the config-conflict signal returned None"
    );
    let lease3 = lease3.unwrap();
    assert_eq!(
        lease3.slot_index(),
        slot_idx,
        "slot leaked: the original slot {slot_idx} was not restored to \
         free_slots after the config-conflict signal — re-claim reused a \
         different slot (the original is stuck LIVE)"
    );
    // Drop of the lease recycles the slot (LIVE -> FREE).
    drop(lease3);
}

/// Re-claim a recycled slot with the *same* config → no conflict, no
/// panic. This is the false-positive guard: a normal single-config
/// usage pattern must not trip the signal.
#[test]
fn matching_config_does_not_trigger_conflict_signal() {
    let _serial = SerialGuard::acquire();
    let lease1 = HeapRegistry::dbg_claim_lease_with_config(CONFIG_A)
        .expect("claim_with_config returned None");
    // Drop of the lease recycles the slot (LIVE -> FREE).
    drop(lease1);

    let before = SeferAlloc::new().stats().config_conflicts;

    // Re-claim with the SAME config. This must NOT panic and must NOT
    // increment the counter. catch_unwind turns any panic into a clear
    // failure message.
    let result = std::panic::catch_unwind(|| HeapRegistry::dbg_claim_lease_with_config(CONFIG_A));
    let lease2 = match result {
        Ok(l) => l,
        Err(panic) => panic!("false positive: claim panicked on a matching config: {panic:?}"),
    };
    assert!(lease2.is_some(), "claim returned None");

    let after = SeferAlloc::new().stats().config_conflicts;
    assert_eq!(
        before, after,
        "false positive: config conflict counted when configs match"
    );

    // Drop of the lease recycles the slot (LIVE -> FREE).
    drop(lease2);
}

/// R6-CQ-3 — the dedicated slot-leak counterfactual on the config-conflict
/// panic path (round5 code_quality_review #3, HIGH).
///
/// **RED (pre-fix):** `claim_with_config(CONFIG_A)` → `recycle` →
/// `claim_with_config(CONFIG_B)` reuses the same slot (LIFO), finds it
/// already initialised with CONFIG_A, and trips the config-conflict
/// `debug_assert!`, which PANICS in debug builds. The `FREE → LIVE` CAS had
/// already succeeded by then; the panic propagated before `return`, so the
/// caller never received the `*mut HeapCore` and could never `recycle`. The
/// slot was stuck `LIVE` forever — out of `free_slots`, never reclaimable.
/// The next `claim_with_config` would mint a DIFFERENT slot (via
/// `bump_count`), and the original slot index could never be claimed again.
///
/// **GREEN (post-fix, R6-CQ-3):** a rollback guard restores the slot to FREE
/// and `free_slots` during the unwind, so the original slot index is
/// immediately reclaimable (LIFO pop). This test captures the original slot
/// id and proves it is reclaimable after the conflict panic — failing
/// pre-fix (a different id, because the original leaked) and passing post-fix
/// (same id, restored by the guard).
///
/// **R2-08 (task #2010):** the conflict no longer panics in any profile (the
/// `debug_assert!` and, with it, the rollback guard are gone), so step 3 now
/// always takes the `Ok` branch; the no-leak property this test pins still
/// holds and is still checked.
#[test]
fn slot_not_leaked_after_config_conflict_panic() {
    let _serial = SerialGuard::acquire();
    // 1. Claim + materialise with CONFIG_A; capture the slot id.
    let lease_a = HeapRegistry::dbg_claim_lease_with_config(CONFIG_A)
        .expect("claim_with_config returned None");
    let slot_idx = lease_a.slot_index();

    // 2. Recycle — slot returns to free_slots (drop of the lease: LIVE -> FREE).
    drop(lease_a);

    // 3. Re-claim with CONFIG_B — same slot (LIFO), already initialised with
    //    CONFIG_A → conflict → counted, returns normally (R2-08); the Ok
    //    branch recycles the returned pointer so the slot is back in
    //    free_slots for the re-claim check below.
    let result = std::panic::catch_unwind(|| {
        // dbg_claim_lease_with_config copies the config (by value), which is
        // UnwindSafe.
        HeapRegistry::dbg_claim_lease_with_config(CONFIG_B)
    });
    if let Ok(lease_b) = result {
        // Drop of the lease recycles the slot (LIVE -> FREE) so it is back in
        // free_slots for the re-claim check below.
        drop(lease_b);
    }

    // 4. R6-CQ-3: the slot MUST be reclaimable as the SAME index. Pre-fix
    //    this returned a DIFFERENT slot (bump_count minting a fresh index)
    //    because the original was stuck LIVE; post-fix the guard restored
    //    the original slot, so LIFO re-claim returns the SAME index.
    let lease_c = HeapRegistry::dbg_claim_lease_with_config(CONFIG_A);
    assert!(
        lease_c.is_some(),
        "registry returned None on re-claim after a config-conflict panic — \
         the slot appears leaked (stuck LIVE, never returned to free_slots)"
    );
    let lease_c = lease_c.unwrap();
    assert_eq!(
        lease_c.slot_index(),
        slot_idx,
        "slot leaked: after the config-conflict panic the original slot \
         {slot_idx} was not restored to free_slots — re-claim got a different \
         slot (the original is stuck LIVE)"
    );

    // 5. Sustained recyclability: the restored slot participates in a normal
    //    claim/recycle round-trip, proving it genuinely re-entered the free
    //    pool (not a one-shot borrow that drops out again).
    // Drop of the lease recycles the slot (LIVE -> FREE).
    drop(lease_c);
    let lease_d = HeapRegistry::dbg_claim_lease_with_config(CONFIG_A);
    assert!(lease_d.is_some());
    let lease_d = lease_d.unwrap();
    assert_eq!(
        lease_d.slot_index(),
        slot_idx,
        "restored slot {slot_idx} not reused on a second round-trip — it did \
         not genuinely re-enter the free pool"
    );
    // Drop of the lease recycles the slot (LIVE -> FREE).
    drop(lease_d);
}
