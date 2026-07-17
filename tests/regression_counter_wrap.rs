//! Task W7a — counterfactual wrap test for the `HeapSlot::generation` counter
//! widening (`AtomicU32` → `AtomicU64`).
//!
//! The generation is bumped once per slot (re)claim; a `u32` wraps at `2^32`
//! recycles (thread deaths), reachable on a thread-per-request server. `u64`
//! wraps at `2^64`. This test presets a live slot's generation NEAR `2^32`,
//! forces a recycle→reclaim, and asserts the generation crosses the old `u32`
//! boundary WITHOUT truncation — proving no `u32` compare survives in the
//! dataflow (a truncated compare would reintroduce the very wrap being removed).
//!
//! NON-VACUOUS: a `u32` generation cannot exceed `u32::MAX`, so the post-cross
//! value (`> 2^32`) is unrepresentable pre-widen and the
//! `assert!(gen > u32::MAX as u64)` fails / the store truncates.
//!
//! (The companion `TaggedPtr` tag-wrap counterfactual — `INDEX_BITS` 32 → 16,
//! tag 32 → 48, the 48-bit `2^48` wrap boundary — moved to the extracted
//! `tagged-index-stack` crate in CRATE-P7, where it is
//! `crates/tagged-index-stack/tests/regression_counter_wrap.rs`, run against the
//! crate's real `TaggedIndex` packing. The registry's `free_slots` now consumes
//! that crate, so the packing is proven there, not here.)

#![cfg(feature = "alloc-global")]

// ===========================================================================
// HeapSlot::generation u64-width counterfactual.
//
// We preset a live slot's `generation` to just BELOW 2^32, force a
// recycle→reclaim (which bumps it by exactly 1 per claim), and assert it
// crosses the old u32 boundary as a u64 — no truncation anywhere in the
// mint→compare chain. The generation's ONLY consumer is claim's `new_gen == 1`
// first-materialise gate (a local u64, never cached in TLS or a struct field),
// so widening the field is the complete fix; this test proves the field itself
// is observably u64 (holds a value > u32::MAX).
// ===========================================================================

use sefer_alloc::registry::{bootstrap, heap_slot::STATE_LIVE, HeapRegistry};

// This is the ONLY test in this binary that touches the registry. Each
// integration test file is its own process with a fresh process-global
// registry, so there is no cross-file race; within this file this test is the
// sole registry user, and the recycle→reclaim LIFO reuse below is deterministic
// single-threaded.

/// Presetting a live slot's `generation` near `2^32` and reclaiming it crosses
/// the old `u32` ceiling: the generation becomes `> u32::MAX`, observable as a
/// `u64`. A `u32` field could not hold this value — the store would truncate
/// and the final assert would fail. This is the "preset near the limit" test.
#[test]
fn generation_crosses_u32_boundary_as_u64() {
    let a = HeapRegistry::claim();
    assert!(!a.is_null(), "claim must succeed");
    let id = unsafe { (*a).id() } as usize;

    let reg = bootstrap::ensure();
    assert_eq!(
        reg.dbg_slot_state(id),
        STATE_LIVE,
        "claimed slot must be LIVE"
    );

    // Preset the generation to just below the old u32 wrap boundary. This slot
    // is ours (we hold the only live handle to it), and generation is written
    // only by its owner on (re)claim. We pick `u32::MAX - 1` so the next
    // reclaim's `fetch_add(1)` lands EXACTLY on `u32::MAX`, and the one after
    // crosses to `u32::MAX + 1` (> 2^32 - 1), which a u32 cannot represent.
    // `generation` is `pub(crate)` (task #93 / R4-MS-4); the preset goes through
    // the `unsafe fn Registry::dbg_slot_preset_generation` accessor.
    let preset: u64 = u32::MAX as u64 - 1;
    // SAFETY: this is the sole live handle to the slot (single-threaded test,
    // no concurrent claim/recycle), and we preset only our own slot's
    // generation — satisfying `dbg_slot_preset_generation`'s precondition.
    unsafe { reg.dbg_slot_preset_generation(id, preset) };
    assert_eq!(reg.dbg_slot_generation(id), preset);

    // Recycle → reclaim: each claim bumps generation by exactly 1.
    // SAFETY: `a` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(a) };
    let b = HeapRegistry::claim();
    assert!(!b.is_null());
    let id_b = unsafe { (*b).id() } as usize;
    assert_eq!(id_b, id, "LIFO reclaim must reuse the same slot");
    let gen1 = reg.dbg_slot_generation(id);
    assert_eq!(
        gen1,
        u32::MAX as u64,
        "reclaim from (2^32 - 2) must bump generation to exactly u32::MAX \
         (2^32 - 1) with NO truncation"
    );

    // One more recycle → reclaim: generation crosses the u32 ceiling.
    // SAFETY: `b` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(b) };
    let c = HeapRegistry::claim();
    assert!(!c.is_null());
    assert_eq!(unsafe { (*c).id() } as usize, id, "still the same slot");
    let gen2 = reg.dbg_slot_generation(id);
    assert_eq!(
        gen2,
        u32::MAX as u64 + 1,
        "reclaim from u32::MAX must bump generation to 2^32 — a value NO u32 \
         can hold. Observing it proves the field is genuinely u64 (no wrap to \
         0, no truncation). Reverting the field to AtomicU32 makes this FAIL."
    );
    assert!(
        gen2 > u32::MAX as u64,
        "generation must exceed u32::MAX — the wrap boundary the widening removes"
    );

    // Leave the slot recycled so we do not leak a live claim into later tests.
    // SAFETY: `c` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(c) };
}
