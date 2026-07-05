//! Task W7a — counterfactual wrap tests for the two counter-width widenings
//! that move long-run wrap hazards from "reachable over weeks/months" to
//! "unreachable in any process lifetime":
//!
//!  1. `TaggedPtr` repack (`INDEX_BITS` 32 → 16, tag 32 → 48). The `free_slots`
//!     Treiber stack packs a slot index + an ABA tag into a `u64`. The tag
//!     wraps at `2^TAG_BITS`; widening it from 32 to 48 bits pushes the
//!     probabilistic ABA-on-wrap from ~2^32 pops to ~2^48 (∼89 years at 100k
//!     pops/s). This test drives the tag to its NEW maximum (`2^48 - 1`), bumps
//!     it once so it WRAPS to 0, and asserts the packed index round-trips
//!     intact across the wrap and that the empty sentinel is never confused
//!     with a live index.
//!
//!  2. `HeapSlot::generation` (`AtomicU32` → `AtomicU64`). The generation is
//!     bumped once per slot (re)claim; a `u32` wraps at `2^32` recycles (thread
//!     deaths), reachable on a thread-per-request server. `u64` wraps at
//!     `2^64`. This test presets a live slot's generation NEAR `2^32`, forces a
//!     recycle→reclaim, and asserts the generation crosses the old `u32`
//!     boundary WITHOUT truncation — proving no `u32` compare survives in the
//!     dataflow (a truncated compare would reintroduce the very wrap being
//!     removed).
//!
//! NON-VACUOUS: each assertion fails if the widening is reverted. For (1), on
//! the OLD 32-bit tag, `2^48 - 1` would not fit the tag half and packing it
//! would corrupt the index; the boundary values chosen (`> u32::MAX`) cannot
//! even be represented pre-widening. For (2), a `u32` generation cannot exceed
//! `u32::MAX`, so the post-cross value (`> 2^32`) is unrepresentable pre-widen
//! and the `assert!(gen > u32::MAX as u64)` fails to even compile-match / the
//! store truncates.

#![cfg(feature = "alloc-global")]

use core::sync::atomic::Ordering;

use sefer_alloc::registry::tagged_ptr::{
    dbg_empty, dbg_is_empty, dbg_pack, dbg_unpack, DBG_INDEX_BITS, DBG_INDEX_MASK, DBG_TAG_BITS,
};

// ===========================================================================
// (1) TaggedPtr tag-wrap counterfactual.
// ===========================================================================

/// The repack is exactly `INDEX_BITS = 16`, `TAG_BITS = 48` — pinned so a
/// future accidental revert to 32/32 fails this test.
#[test]
fn tagged_ptr_is_repacked_16_48() {
    assert_eq!(DBG_INDEX_BITS, 16, "W7a: index half must be 16 bits");
    assert_eq!(DBG_TAG_BITS, 48, "W7a: tag half must be 48 bits");
    assert_eq!(
        DBG_INDEX_MASK, 0xFFFF,
        "W7a: index mask must be 0xFFFF (16 all-ones bits)"
    );
    // Sanity: the halves partition the 64-bit word exactly.
    assert_eq!(DBG_INDEX_BITS + DBG_TAG_BITS, 64);
}

/// The tag reaches its NEW maximum (`2^48 - 1`) and WRAPS to 0 on the next
/// bump, and the packed index round-trips intact across the wrap. Under the
/// OLD 32-bit tag this maximum is unrepresentable in the tag half, so this
/// test could not even be written against the pre-W7a packing — it is the
/// direct counterfactual for the widening.
#[test]
fn tag_wraps_at_2_pow_48_and_index_survives() {
    // The largest tag the 48-bit half can hold.
    let max_tag: u64 = (1u64 << DBG_TAG_BITS) - 1; // 2^48 - 1
    assert!(
        max_tag > u32::MAX as u64,
        "the NEW max tag must exceed the OLD 32-bit range — this is the whole \
         point of the widening (and makes this test unrepresentable pre-W7a)"
    );

    // A representative valid slot index (well within MAX_HEAPS = 4096).
    let idx: u64 = 0x0ABC; // 2748 < 4096

    // Pack at the tag maximum, then unpack: both halves must round-trip.
    let at_max = dbg_pack(idx, max_tag);
    let (v0, t0) = dbg_unpack(at_max);
    assert_eq!(v0, idx, "index must survive packing at the tag maximum");
    assert_eq!(t0, max_tag, "tag must round-trip at its 48-bit maximum");

    // Bump the tag once. `push_free_slot` computes `tag.wrapping_add(1)` on
    // the tag half recovered by `unpack` (always < 2^48, since `word >> 16`
    // yields at most 48 bits) and re-`pack`s it. At the 48-bit maximum the
    // increment produces 2^48, whose bit-48 is shifted straight OUT of the
    // word by `pack`'s `tag << INDEX_BITS` — so the tag as STORED-and-RE-READ
    // wraps to 0. We model that exactly: pack the incremented tag, then unpack.
    let bumped_tag = max_tag.wrapping_add(1); // 2^48 (bit 48 set)
    let after_wrap = dbg_pack(idx, bumped_tag);
    let (v1, t1) = dbg_unpack(after_wrap);
    assert_eq!(
        t1, 0,
        "tag at 2^48 - 1 must wrap to 0 once bumped and re-packed (bit 48 is \
         shifted out of the 64-bit word by the 16-bit index shift)"
    );
    assert_eq!(
        v1, idx,
        "index must be IDENTICAL across the tag wrap (no bleed from tag into \
         index — a truncation/overlap bug would corrupt it here)"
    );
    assert_eq!(
        v0, v1,
        "the index recovered at the tag maximum and after the wrap must match"
    );

    // The wrapped word must NOT be mistaken for the empty sentinel: a live
    // index (0x0ABC) with tag 0 is a real head, not "stack empty".
    assert!(
        !dbg_is_empty(after_wrap),
        "a live index with a wrapped (0) tag must NOT read as the empty sentinel"
    );
}

/// The empty sentinel (`value = INDEX_MASK = 0xFFFF`, above MAX_HEAPS = 4096)
/// is never mistaken for a live index, and no valid index collides with it —
/// the critical property when the index half shrank to 16 bits. Under the OLD
/// packing the sentinel was `u32::MAX` (`0xFFFF_FFFF`), which does NOT fit 16
/// bits; W7a's move to `0xFFFF` is what this pins.
#[test]
fn empty_sentinel_never_collides_with_a_live_index() {
    let empty = dbg_empty();
    assert!(dbg_is_empty(empty), "the empty sentinel must read as empty");
    let (sentinel_idx, sentinel_tag) = dbg_unpack(empty);
    assert_eq!(
        sentinel_idx, DBG_INDEX_MASK,
        "the empty sentinel's index half is INDEX_MASK (0xFFFF)"
    );
    assert_eq!(sentinel_tag, 0, "the empty sentinel's tag is 0");

    // The sentinel index (0xFFFF = 65535) is strictly above MAX_HEAPS (4096),
    // so it can never be a real slot index — no producer/consumer collision.
    const MAX_HEAPS: u64 = 4096;
    // Compile-time invariant (a runtime `assert!` on two consts trips
    // clippy::assertions_on_constants): the empty sentinel index must be
    // >= MAX_HEAPS so it can never be a real slot index.
    const _: () = assert!(
        DBG_INDEX_MASK >= MAX_HEAPS,
        "the empty sentinel index must be >= MAX_HEAPS so it is a non-index"
    );

    // Every valid index (0..MAX_HEAPS), packed with ANY tag (including the
    // wrapped 0 and the max), is NOT empty and round-trips. Spot-check the
    // boundary indices plus the largest valid one.
    for &idx in &[0u64, 1, MAX_HEAPS - 1] {
        for &tag in &[0u64, 1, (1u64 << DBG_TAG_BITS) - 1] {
            let word = dbg_pack(idx, tag);
            assert!(
                !dbg_is_empty(word),
                "valid index {idx} with tag {tag} must not read as empty"
            );
            let (v, t) = dbg_unpack(word);
            assert_eq!(v, idx, "index {idx} must round-trip (tag {tag})");
            assert_eq!(t, tag, "tag {tag} must round-trip (index {idx})");
        }
    }
}

// ===========================================================================
// (2) HeapSlot::generation u64-width counterfactual.
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

// This is the ONLY test in this binary that touches the registry (the
// TaggedPtr tests above are pure arithmetic, no `claim`). Each integration
// test file is its own process with a fresh process-global registry, so there
// is no cross-file race; within this file this test is the sole registry user,
// and the recycle→reclaim LIFO reuse below is deterministic single-threaded.

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
    let slot = &reg.slots[id];
    assert_eq!(
        slot.state.load(Ordering::Acquire),
        STATE_LIVE,
        "claimed slot must be LIVE"
    );

    // Preset the generation to just below the old u32 wrap boundary. This slot
    // is ours (we hold the only live handle to it), and generation is written
    // only by its owner on (re)claim — safe to store here directly (the field
    // is `pub` under `#[doc(hidden)]`). We pick `u32::MAX - 1` so the next
    // reclaim's `fetch_add(1)` lands EXACTLY on `u32::MAX`, and the one after
    // crosses to `u32::MAX + 1` (> 2^32 - 1), which a u32 cannot represent.
    let preset: u64 = u32::MAX as u64 - 1; // 2^32 - 2
    slot.generation.store(preset, Ordering::Release);
    assert_eq!(slot.generation.load(Ordering::Acquire), preset);

    // Recycle → reclaim: each claim bumps generation by exactly 1.
    // SAFETY: `a` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(a) };
    let b = HeapRegistry::claim();
    assert!(!b.is_null());
    let id_b = unsafe { (*b).id() } as usize;
    assert_eq!(id_b, id, "LIFO reclaim must reuse the same slot");
    let gen1 = reg.slots[id].generation.load(Ordering::Acquire);
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
    let gen2 = reg.slots[id].generation.load(Ordering::Acquire);
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
