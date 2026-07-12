//! `RemoteFreeRing` — a per-segment, bounded, **non-intrusive** MPSC queue
//! of freed-block **offsets** (`u32`), carved from segment metadata.
//!
//! ## Why this exists — the cross-thread-free drain-reclaim UAF fix
//!
//! The Phase 12.5 inline `ThreadFreeStack` (an intrusive Treiber stack whose
//! "node" was the freed block's own first word) raced fatally across the slot
//! release→claim boundary (root-caused in `docs/RACE_DRAIN_RECLAIM.md` §8): a
//! cross-thread freer and the slot's new owner contended the SAME block word —
//! the freer wrote a `next` pointer into it while the owner had already popped
//! the block from the `BinTable` and handed it to the app (which wrote user
//! data). The drain then read user data as a free-list `next` pointer → UAF.
//!
//! **This queue removes the contended word entirely.** A cross-thread freer
//! never touches the block's bytes: it only pushes the block's
//! *segment-relative offset* (a plain `u32`) into this in-segment ring. The
//! owner drains the ring and reclaims each offset into the segment's `BinTable`
//! as the single writer. The block's first word is owned solely by whoever
//! currently holds it (free-list `next` while queued in the `BinTable`, or user
//! data while live) — there is no third "in-flight to a remote queue" role that
//! the intrusive TFS introduced. This restores the original `ShardedRegion` 7b
//! discipline (queues carry references/indices, never poison the object).
//!
//! ## What this module IS and is NOT
//!
//! - IS: pure safe data + arithmetic over the `node` (`super::node`) seam. Every
//!   atomic access goes through `Node::atomic_u32_at` (a confined-`unsafe`
//!   primitive identical in spirit to `atomic_u64_at`). There is NO `unsafe`
//!   here — the crate's structural promise ("`unsafe` lives ONLY in `os` +
//!   `node`") is upheld by the compiler.
//! - IS: an MPSC bounded queue. **Many producers** (cross-thread freers) push
//!   via `fetch_add`-free CAS-reserve; **one consumer** (the owning thread)
//!   drains. The single-consumer invariant is the slot's single-writer rule
//!   (the slot's owner is the sole `BinTable` writer, hence the sole drainer).
//! - IS NOT: a way to read or write the *payload* of a freed block. Only the
//!   offset (an integer) crosses the queue.
//!
//! ## Layout in a segment
//!
//! ```text
//!   ... bin_table_off + BinTable::FOOTPRINT (4-byte aligned)
//!   ┌──────────────────────────────────────────────────────────┐
//!   │ RemoteFreeRing                                           │
//!   │  offset 0..64  (own cache line — consumer-only writes):  │
//!   │  • head: AtomicU32  (4 B) — drain cursor (consumer)      │
//!   │  • [60 B reserved padding]                                │
//!   │  offset 64..128 (own cache line — producer-touched):     │
//!   │  • tail: AtomicU32  (4 B) — push reserve cursor (producers)
//!   │  • overflow: AtomicU32 (4 B) — count of discarded pushes  │
//!   │    (ring-full → bounded leak; sound, never corrupts)      │
//!   │  • [56 B reserved padding]                                │
//!   │  offset 128.. (data, starts on its own cache line):       │
//!   │  • slots: [AtomicU32; RING_CAP]  (RING_CAP × 4 B)         │
//!   │    each slot holds a block offset or RING_SLOT_EMPTY      │
//!   └──────────────────────────────────────────────────────────┘
//! ```
//!
//! **PERF-PASS-4 (G8/ML4, task #52):** the cursor block widened from 16 to
//! 128 bytes so `head` (consumer-only), `tail`/`overflow` (producer-touched),
//! and the data slots each start on their OWN 64-byte cache line — the
//! pre-task packing put all three on one line (the ring's in-segment base is
//! 64-byte aligned), guaranteeing maximal ping-pong: a consumer's `head`
//! publish invalidated the producers' `tail` CAS line AND the first 12 data
//! slots. `FOOTPRINT = CURSOR_BLOCK (128) + RING_CAP * 4`. With `RING_CAP =
//! 256` that is 1152 bytes per segment (was 1040) — still under one page,
//! negligible vs. the 4 MiB segment.
//!
//! ## MPSC protocol (Vyukov-style bounded, CAS-reserved)
//!
//! Two monotonic cursors: `tail` (producers reserve push slots) and `head`
//! (the consumer advances past drained slots). `slots[i % CAP]` holds the
//! offset for the reservation `i`, or `RING_SLOT_EMPTY` if not-yet-written /
//! already-drained.
//!
//! **Push (multi-producer):**
//! 1. `t = tail.load(Relaxed)`. If `t.wrapping_sub(head.load(Acquire)) >= CAP`
//!    → ring full → return `Err(Overflow)` (the caller discards the block:
//!    bounded leak, sound). `Acquire` on the head load sees the consumer's
//!    `Release` head advance, so a slot freed by the drain is observable.
//! 2. CAS `tail: t → t+1` with `AcqRel` on success (the reservation is the
//!    linearization point — exactly one producer wins each `t`). `Relaxed` on
//!    failure (retry; no side-effect).
//! 3. Store `slots[t % CAP] = offset` with `Release` (publishes the offset to
//!    the consumer's `Acquire` slot read). Return `Ok(())`.
//!
//! **Drain (single consumer):**
//! 1. `t = tail.load(Acquire)` (sees every producer's `Release` reservation).
//! 2. While `h != t` (wrap-correct — both cursors are monotonic wrapping
//!    counters, so the undrained count is `t.wrapping_sub(h)`, NOT `t - h`):
//!    load `slots[h % CAP]` with `Acquire`. If `RING_SLOT_EMPTY`
//!    → the reservation was won but the publish store hasn't happened yet
//!    (producer is between steps 2 and 3); **stop draining** (we cannot skip
//!    it — order is preserved by the cursors; a later drain picks it up).
//!    Otherwise reclaim the offset, store `slots[h % CAP] = RING_SLOT_EMPTY`
//!    (`Relaxed` — only this consumer writes a non-empty value... no: producers
//!    also write here on their reserved slot; but a producer only writes to
//!    `slots[p % CAP]` for a `p` it reserved, and reservations are unique, so
//!    by the time we drain slot `h`, no producer will write it again until
//!    `tail` wraps past `h + CAP` — which the full-check prevents. `Relaxed` is
//!    safe because the next producer to touch this slot will `Release`-store
//!    its offset, and our drain reads with `Acquire`.), `h = h.wrapping_add(1)`.
//! 3. `head.store(h, Release)` (publishes the drain progress to producers'
//!    full-check `Acquire` head load).
//!
//! **Ordering summary (each justified above):**
//! - producer reservation CAS: `AcqRel` (success) / `Relaxed` (failure).
//! - producer publish store: `Release`.
//! - consumer tail load: `Acquire`.
//! - consumer slot load: `Acquire`.
//! - consumer slot clear: `Relaxed`.
//! - consumer head store: `Release`.
//! - producer full-check head load: `Acquire`.
//!
//! ## Overflow semantics (the honest remainder)
//!
//! When the ring is full (`tail - head == CAP`), a push returns
//! `Err(PushOverflow)` and the caller **discards** the block (it stays mapped,
//! unused — a bounded leak). This is SOUND (no UAF, no corruption) but costs
//! RSS: at most `(CAP - drained_count)` blocks per segment can be in flight,
//! and a sustained burst faster than the owner drains leaks one block per
//! overflow. In practice the owner drains on every alloc, so the ring rarely
//! fills under normal churn; the leak bound is the in-flight cross-thread-free
//! footprint per segment between drains. This is strictly better than the
//! Phase 12.5 discard (which leaked the ENTIRE cross-thread-free chain per slot
//! recycle) and, crucially, it is a *correctness-preserving* fallback, not a
//! correctness violation — the race is gone.

// Only reached by the ring's atomic push/drain methods, which are themselves
// only reachable on builds that exercise cross-thread free
// (`alloc-xthread`); unused under `--features alloc-core` alone.
#[cfg_attr(not(feature = "alloc-xthread"), allow(unused_imports))]
use core::sync::atomic::Ordering;

use super::node::Node;

/// TEST/DIAGNOSTIC-ONLY (task D2): process-wide count of ring-push overflows
/// (a cross-thread free that found its target segment's ring full and
/// discarded the block — a sound but observable bounded leak; see "Overflow
/// semantics" above). Bumped in [`RemoteFreeRing::push`] alongside the
/// existing per-segment `overflow` cursor-block counter. The per-segment
/// counter ([`RemoteFreeRing::overflow_count`]) is exact for one segment but
/// requires the caller to already hold a `RemoteFreeRing` handle (i.e. know
/// which segment to ask); this process-wide counter gives O(1) visibility
/// into "did overflow happen anywhere, ever" without walking the segment
/// table — the minimum bar for production observability (feeds Phase E
/// stats). Relaxed: diagnostic only, no synchronisation implied.
#[doc(hidden)]
pub static DBG_RING_OVERFLOW: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Sentinel slot value meaning "this slot carries no offset" (either
/// not-yet-published by a producer, or already drained by the consumer). A real
/// block offset is always `< SEGMENT` (`1 << 22`), so `u32::MAX` is unambiguous.
#[doc(hidden)]
pub const RING_SLOT_EMPTY: u32 = u32::MAX;

/// The number of offset slots in the ring. 256 → 1 KiB of slots per segment.
///
/// **Rationale:** a 4 MiB segment holds up to `SEGMENT / MIN_BLOCK` blocks
/// (≈ 256 K at `MIN_BLOCK = 16`). The ring need only absorb the *burst* of
/// cross-thread frees that arrive between the owner's drains (the owner drains
/// on every alloc and on the `find_segment_with_free` scan). 256 covers a
/// typical burst with headroom; overflow degrades to a bounded leak (sound).
/// Larger caps trade segment metadata footprint for rarer overflow; 256 is the
/// mimalloc-class default for per-page deferred-free queues.
#[doc(hidden)]
pub const RING_CAP: usize = 256;

// The ring's u32 `head`/`tail` cursors are monotonic WRAPPING counters:
// occupancy is `tail.wrapping_sub(head)` and the slot index is `i % RING_CAP`.
// For the slot sequence to stay CONTINUOUS across the `u32::MAX → 0` wrap, the
// index must not jump at the boundary: `(2^32 - 1) % CAP` must be followed by
// `0 % CAP`, i.e. `2^32 % CAP == 0`. That holds iff `CAP` is a power of two.
// A non-power-of-two CAP would jump the slot index at the wrap (…, (2^32-1) mod
// CAP, 0 mod CAP …) and corrupt the FIFO on the ONE genuinely reachable wrap
// hazard (2^32 cross-thread frees on a single hot, long-lived segment). This is
// an otherwise UNSTATED dependency; pin it at compile time.
const _: () = assert!(
    RING_CAP.is_power_of_two(),
    "RING_CAP must be a power of two so 2^32 % RING_CAP == 0 — the ring's u32 \
     head/tail cursors wrap continuously across u32::MAX only then; a \
     non-power-of-two CAP would jump the slot index at the wrap and corrupt the \
     FIFO"
);

/// The byte footprint of a `RemoteFreeRing` in segment metadata. Fixed so the
/// bootstrap can carve it deterministically alongside the bin table.
#[doc(hidden)]
pub const FOOTPRINT: usize = CURSOR_BLOCK + RING_CAP * core::mem::size_of::<u32>();

/// Bits of a ring entry reserved for the block's segment-relative offset.
/// `SEGMENT = 1 << 22`, so every offset is `< 2^22` and fits in the low 22 bits;
/// the high bits carry the size **class** the cross-thread freer stamped (it has
/// the `Layout`, unlike the owner, whose `page_map` is unreliable for the
/// mixed-class pages a shared bump cursor produces — see RACE_DRAIN_RECLAIM §13).
pub(crate) const ENTRY_OFF_BITS: u32 = 22;
/// Mask for the offset field of a packed ring entry.
pub(crate) const ENTRY_OFF_MASK: u32 = (1 << ENTRY_OFF_BITS) - 1;

/// Pack a `(offset, class_idx)` pair into a single `u32` ring entry.
/// `off < 2^22` (a segment offset) and `class_idx < SMALL_CLASS_COUNT (= 49)`,
/// so the result is `< 2^32` and never collides with `RING_SLOT_EMPTY`
/// (`u32::MAX`) for any real block.
#[cfg_attr(
    any(not(feature = "alloc-xthread"), feature = "hardened"),
    allow(dead_code)
)]
#[inline(always)]
pub(crate) fn pack_entry(off: u32, class_idx: u32) -> u32 {
    debug_assert!(off <= ENTRY_OFF_MASK, "offset overflows ring-entry field");
    off | (class_idx << ENTRY_OFF_BITS)
}

/// Unpack a ring entry into `(offset, class_idx)`.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[inline(always)]
pub(crate) fn unpack_entry(packed: u32) -> (u32, u32) {
    (packed & ENTRY_OFF_MASK, packed >> ENTRY_OFF_BITS)
}

// ---------------------------------------------------------------------------
// X7 Ф2 (task #190) — hardened ring-entry repack: `[gen:8|class:6|off16:18]`.
//
// The non-hardened `pack_entry`/`unpack_entry` ABOVE are byte-for-byte
// untouched (the production entry format, compiled whenever `alloc-xthread`
// is on — this is NOT a hardened-only surface). The block below adds a
// SEPARATE packing scheme compiled ONLY under `#[cfg(feature = "hardened")]`,
// threading the block's generation counter (X7 Ф1's gen-table byte) into the
// ring note so a drain can drop a note whose generation no longer matches the
// block's current life (X7 plan §2.4, §3-Ф2). Nothing here is wired into
// `push`/`drain` or any other ring method yet — that is Ф3. This phase is
// purely the pack/unpack pair + round-trip tests, mirroring Ф1's discipline.
//
// Bit layout (low bits → high bits), matching the plan's notation
// `[gen:8|class:6|off16:18]` read high-to-low (the same convention the
// non-hardened doc comment uses: `[class_idx: bits 22..32][off: bits 0..22]`
// lists the HIGH field first):
//
//   bits [ 0..18) : off16 = off >> MIN_BLOCK_SHIFT   (off in MIN_BLOCK units)
//   bits [18..24) : class_idx                         (size class, < 64)
//   bits [24..32) : gen                               (generation byte, 0..=255)
//
// `off16` is 18 bits because `SEGMENT / MIN_BLOCK = 2^22 / 2^4 = 2^18` — every
// `MIN_BLOCK`-aligned segment-relative offset divides to a value `< 2^18`.
// `class` is 6 bits because `SMALL_CLASS_COUNT = 49 < 64 = 2^6`. `gen` is 8
// bits — the `u8` generation counter established in Ф1 (wraps at 256, the
// accepted 1/256 residual; X7 §2.5). The three fields sum to exactly 32 — no
// wasted or overlapping bits. The external contract is symmetric with the
// non-hardened pair: callers pass and receive the FULL segment-relative byte
// offset (the `off16` internal representation never leaks — pack shifts down
// by `MIN_BLOCK_SHIFT`, unpack shifts back up).
//
// `RING_SLOT_EMPTY` (`u32::MAX`) non-collision: the packed word equals
// `u32::MAX` only when ALL three fields are simultaneously all-ones — i.e.
// `gen=0xFF`, `class=0x3F` (=63), `off16=0x3_FFFF`. `off16=0x3_FFFF` IS
// reachable (it is `SEGMENT - MIN_BLOCK` >> 4, a real last block start), and
// `gen=0xFF` is reachable (the u8 wrap). BUT `class=63` is NOT: the maximum
// real small class index is `SMALL_CLASS_COUNT - 1 = 48` (`0x30`), so the
// class field never reaches `0x3F`. The maximum packed word over real ranges
// is therefore `0xFFC3_FFFF < u32::MAX` (computed and pinned by the
// `entry_never_collides_with_ring_slot_empty` regression test). This safety
// HOLDS ONLY WHILE `SMALL_CLASS_COUNT <= 49` — the const-assert below pins
// that the class field's all-ones value (`2^ENTRY_CLASS_BITS - 1 = 63`) stays
// strictly above `SMALL_CLASS_COUNT - 1`, so a future bump of
// `SMALL_CLASS_COUNT` past 63 cannot silently reintroduce a collision. Ф3's
// ring `push`/`drain` reuse is sound under that invariant.
// ---------------------------------------------------------------------------

/// X7 Ф2: bits of a hardened ring entry reserved for `off16` (the offset in
/// `MIN_BLOCK` units). `SEGMENT / MIN_BLOCK = 2^18`, so 18 bits suffice.
#[cfg(feature = "hardened")]
#[doc(hidden)]
pub const ENTRY_OFF16_BITS: u32 = 18;
/// X7 Ф2: bits reserved for the size class. `SMALL_CLASS_COUNT = 49 < 2^6`.
#[cfg(feature = "hardened")]
#[doc(hidden)]
pub const ENTRY_CLASS_BITS: u32 = 6;
/// X7 Ф2: bits reserved for the generation counter (the Ф1 `u8`, wraps at 256).
#[cfg(feature = "hardened")]
#[doc(hidden)]
pub const ENTRY_GEN_BITS: u32 = 8;

/// X7 Ф2: shift of the `class` field (starts where `off16` ends).
#[cfg(feature = "hardened")]
const ENTRY_CLASS_SHIFT: u32 = ENTRY_OFF16_BITS;
/// X7 Ф2: shift of the `gen` field (starts where `class` ends).
#[cfg(feature = "hardened")]
const ENTRY_GEN_SHIFT: u32 = ENTRY_OFF16_BITS + ENTRY_CLASS_BITS;

/// X7 Ф2: mask for the `off16` field of a hardened ring entry.
#[cfg(feature = "hardened")]
pub(crate) const ENTRY_OFF16_MASK: u32 = (1u32 << ENTRY_OFF16_BITS) - 1;
/// X7 Ф2: mask for the `class` field of a hardened ring entry.
#[cfg(feature = "hardened")]
pub(crate) const ENTRY_CLASS_MASK: u32 = (1u32 << ENTRY_CLASS_BITS) - 1;
/// X7 Ф2: mask for the `gen` field of a hardened ring entry.
#[cfg(feature = "hardened")]
pub(crate) const ENTRY_GEN_MASK: u32 = (1u32 << ENTRY_GEN_BITS) - 1;

// X7 Ф2: compile-time pin of the bit layout (W7-style const-asserts, mirroring
// the existing `RING_CAP.is_power_of_two()` assert above). Each field's value
// range is provably covered, and the three fields sum to exactly 32 — the
// plan's layout, not a looser one. `SEGMENT / MIN_BLOCK` and `SMALL_CLASS_COUNT`
// are referenced via `super::` (this file otherwise imports only `Node`); the
// hardened-only `use` is colocated with the asserts so it is invisible to a
// non-hardened compile.
#[cfg(feature = "hardened")]
const _: () = {
    use super::os::SEGMENT;
    use super::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT, SMALL_CLASS_COUNT};
    assert!(
        ENTRY_GEN_BITS + ENTRY_CLASS_BITS + ENTRY_OFF16_BITS == 32,
        "hardened ring entry fields must sum to exactly 32 bits (X7 §2.4 layout)"
    );
    assert!(
        ENTRY_GEN_BITS == 8,
        "gen field must be exactly 8 bits (the Ф1 u8 generation counter)"
    );
    assert!(
        (SMALL_CLASS_COUNT as u64) <= (1u64 << ENTRY_CLASS_BITS),
        "class field must cover SMALL_CLASS_COUNT"
    );
    assert!(
        MIN_BLOCK.is_power_of_two() && SEGMENT.is_power_of_two(),
        "MIN_BLOCK and SEGMENT must be powers of two for the exact off16 division"
    );
    assert!(
        MIN_BLOCK_SHIFT == MIN_BLOCK.trailing_zeros(),
        "MIN_BLOCK_SHIFT must equal log2(MIN_BLOCK) (kept in sync by size_classes)"
    );
    assert!(
        (SEGMENT as u64) >> MIN_BLOCK_SHIFT <= (1u64 << ENTRY_OFF16_BITS),
        "off16 field must cover SEGMENT/MIN_BLOCK (the largest MIN_BLOCK-aligned offset)"
    );
    // RING_SLOT_EMPTY non-collision pin (see the block doc above): the packed
    // word is `u32::MAX` only when gen=0xFF AND class=0x3F AND off16=0x3_FFFF.
    // gen and off16 maxima ARE reachable, so safety rests on class=0x3F being
    // UNreachable — i.e. the max real class (`SMALL_CLASS_COUNT - 1`) staying
    // strictly below the class field's all-ones value (`2^BITS - 1`). Pin it so
    // a future bump of SMALL_CLASS_COUNT into the all-ones value fails to
    // compile here instead of silently reintroducing a sentinel collision.
    assert!(
        (SMALL_CLASS_COUNT as u64) < (1u64 << ENTRY_CLASS_BITS) - 1,
        "SMALL_CLASS_COUNT must stay strictly below the class field's all-ones value \
         so a hardened ring entry can never equal RING_SLOT_EMPTY (u32::MAX)"
    );
};

/// X7 Ф2: pack `(gen, class_idx, off)` into a single `u32` hardened ring entry
/// with the layout `[gen:8|class:6|off16:18]` (gen in the HIGH bits, class in
/// the middle, `off16 = off >> MIN_BLOCK_SHIFT` in the LOW bits — see the block
/// doc above). `off` is the FULL segment-relative byte offset (same units as
/// the non-hardened [`pack_entry`]); the `off16` internal representation never
/// leaks to callers. Returns a value that never collides with
/// [`RING_SLOT_EMPTY`] for any real `(gen, class, off)` triple (verified by the
/// `entry_never_collides_with_ring_slot_empty` regression test).
///
/// Compiled ONLY under `#[cfg(feature = "hardened")]`; not wired into
/// `push`/`drain` yet (that is Ф3).
#[cfg(feature = "hardened")]
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[inline(always)]
pub fn pack_entry_hardened(gen: u8, class_idx: u32, off: u32) -> u32 {
    debug_assert!(
        off >> super::size_classes::MIN_BLOCK_SHIFT <= ENTRY_OFF16_MASK,
        "offset overflows hardened ring-entry off16 field"
    );
    debug_assert!(
        off.is_multiple_of(super::size_classes::MIN_BLOCK as u32),
        "hardened ring-entry offset must be MIN_BLOCK-aligned (off16 = off >> MIN_BLOCK_SHIFT)"
    );
    debug_assert!(
        class_idx <= ENTRY_CLASS_MASK,
        "class_idx overflows hardened ring-entry class field"
    );
    let off16 = off >> super::size_classes::MIN_BLOCK_SHIFT;
    let packed = (off16 & ENTRY_OFF16_MASK)
        | ((class_idx & ENTRY_CLASS_MASK) << ENTRY_CLASS_SHIFT)
        | ((gen as u32 & ENTRY_GEN_MASK) << ENTRY_GEN_SHIFT);
    debug_assert_ne!(
        packed, RING_SLOT_EMPTY,
        "hardened pack_entry must never produce the ring-slot sentinel"
    );
    packed
}

/// X7 Ф2: unpack a hardened ring entry into `(gen, class_idx, off)`, where
/// `off` is the FULL segment-relative byte offset (the `off16` internal field
/// is shifted back up by `MIN_BLOCK_SHIFT` so the external contract is symmetric
/// with the non-hardened [`unpack_entry`]).
///
/// Compiled ONLY under `#[cfg(feature = "hardened")]`; not wired into
/// `push`/`drain` yet (that is Ф3).
#[cfg(feature = "hardened")]
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[inline(always)]
pub fn unpack_entry_hardened(packed: u32) -> (u8, u32, u32) {
    let off16 = packed & ENTRY_OFF16_MASK;
    let class_idx = (packed >> ENTRY_CLASS_SHIFT) & ENTRY_CLASS_MASK;
    let gen = ((packed >> ENTRY_GEN_SHIFT) & ENTRY_GEN_MASK) as u8;
    let off = off16 << super::size_classes::MIN_BLOCK_SHIFT;
    (gen, class_idx, off)
}

/// The cursor block: `head`, `tail`, `overflow`, padded up to 128 bytes — two
/// full cache lines, so `head` (consumer-only) and `tail`/`overflow`
/// (producer-touched) each start their OWN 64-byte-aligned line.
///
/// **PERF-PASS-4 (G8/ML4, task #52) — was 16 bytes.** At `CURSOR_BLOCK = 16`,
/// `head`@0 + `tail`@4 + `overflow`@8 + a 4-byte pad + `slots[0..12]` all
/// shared ONE 64-byte cache line (the ring's in-segment base is 64-byte
/// aligned, so this was exact, not approximate). Producers CAS `tail` and
/// Acquire-load `head` on every push; the consumer Release-stores `head`,
/// Acquire-loads `tail`, and reads/clears slots — all landing on that SAME
/// line. Widening to 128 bytes puts `head` (offset 0, consumer-only writes)
/// on its own line and `tail`/`overflow` (offset 64, producer-touched) on a
/// SECOND line, disjoint from both `head` and the first data slots
/// (`SLOTS_OFF` moves from 16 to 128). Costs 112 extra bytes per segment's
/// ring metadata (4 MiB segment; negligible). `FOOTPRINT` and every
/// downstream segment-metadata offset (`Layout::small_meta_end`, etc.)
/// derive FROM this constant, so the layout re-composes automatically — see
/// the compile-time layout asserts at the bottom of `segment_header.rs`,
/// which re-verify unchanged.
const CURSOR_BLOCK: usize = 128;

/// Offset of the `head` cursor within the ring metadata. Own cache line
/// (bytes 0..64) — consumer-only writes (`drain`'s `head.store`), producer
/// reads (`push`'s full-check `head.load(Acquire)`).
///
/// Only read by the ring's push/drain methods, which are only reachable on
/// builds that exercise cross-thread free (`alloc-xthread`); unused under
/// `--features alloc-core` alone.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const HEAD_OFF: usize = 0;
/// Offset of the `tail` cursor within the ring metadata. PERF-PASS-4: moved
/// from 4 to 64 — its own cache line, separate from `head`'s line and from
/// the first data slots. Producer-CASed on every push; consumer
/// Acquire-loads it once per drain.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const TAIL_OFF: usize = 64;
/// Offset of the `overflow` counter within the ring metadata. PERF-PASS-4:
/// moved from 8 to 68 — shares `tail`'s line (both are producer-touched;
/// `overflow` is only written on the rare full-ring path, so co-locating it
/// with `tail` costs nothing on the common push path and avoids spending a
/// THIRD cache line on one counter).
const OVERFLOW_OFF: usize = 68;
/// Offset of the first slot within the ring metadata. PERF-PASS-4: moved
/// from 16 to 128 (`CURSOR_BLOCK`) — the data slots now start on a line past
/// BOTH cursor lines, so neither producer's `tail` CAS nor the consumer's
/// `head` store dirties a line the other side is scanning for data.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const SLOTS_OFF: usize = CURSOR_BLOCK;

/// The per-segment non-intrusive cross-thread-free MPSC ring.
///
/// A thin view over in-segment metadata (no allocation — the bootstrap carves
/// the bytes at [`super::segment_header::Layout::remote_ring_off`]). Producers
/// push block offsets; the single consumer ([`drain`](Self::drain)) reclaims
/// them. See the module docs for the protocol and orderings.
///
/// The struct + `FOOTPRINT` are compiled unconditionally (the segment `Layout`
/// always reserves the ring's bytes); the `push`/`drain`/`at`/`init_in_place`
/// methods exist only under `alloc-xthread` (the cross-thread feature).
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[doc(hidden)]
pub struct RemoteFreeRing {
    base: *mut u8,
}

/// A push failed because the ring is full. The caller MUST discard the block
/// (bounded leak) — see "Overflow semantics" in the module docs.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[doc(hidden)]
pub struct PushOverflow;

impl RemoteFreeRing {
    /// Construct the view over ring metadata at `base + off`. The caller (the
    /// bootstrap / `SegmentMeta::remote_ring`) guarantees the byte range
    /// `[base + off, base + off + FOOTPRINT)` is carved, 4-byte-aligned, and
    /// inside a live segment.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn at(base: *mut u8, off: usize) -> Self {
        Self {
            base: Node::offset(base, off),
        }
    }

    /// **Test surface** (`#[doc(hidden)] pub`): construct a ring view over an
    /// arbitrary aligned byte buffer at offset 0. Used ONLY by the isolated
    /// ring unit test (`tests/remote_ring_unit.rs`), which builds a ring over a
    /// plain `Box<[u8]>` (NOT a segment, NOT an allocator) to prove the ring's
    /// MPSC correctness in isolation from the allocator / ABA concerns. The
    /// caller guarantees `base` points to at least `FOOTPRINT` writable,
    /// 4-byte-aligned bytes that live for the ring's use (e.g. an
    /// `alloc::vec![0u8; FOOTPRINT]` boxed slice).
    ///
    /// Production code MUST use [`at`](Self::at) with a segment-relative offset
    /// from [`Layout::remote_ring_off`](super::segment_header::Layout::remote_ring_off).
    ///
    /// R2-3: the null + 4-byte-alignment preconditions are checked by a
    /// RELEASE-surviving `assert!` (not `debug_assert!`), so a null/misaligned
    /// base panics in every build. The `FOOTPRINT`-writability / liveness half
    /// of the contract cannot be checked at runtime (this module is
    /// `#![forbid(unsafe_code)]`, so the `unsafe fn` discipline used by the
    /// `heap_registry` seam does not apply here) and remains the caller's
    /// responsibility — passing a `FOOTPRINT`-valid, aligned live buffer is the
    /// only documented use.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn over_test_buffer(base: *mut u8) -> Self {
        assert!(
            !base.is_null() && (base as usize).is_multiple_of(4),
            "over_test_buffer: base must be non-null and 4-byte-aligned (R2-3 release guard)"
        );
        Self::at(base, 0)
    }

    /// **Test surface**: initialise a fresh ring at `base` (offset 0). Same as
    /// [`init_in_place`](Self::init_in_place) but for a standalone buffer (no
    /// segment-relative offset). See [`over_test_buffer`](Self::over_test_buffer).
    ///
    /// R2-3: carries the same release-surviving null + 4-byte-alignment `assert!`
    /// as [`over_test_buffer`](Self::over_test_buffer); the `FOOTPRINT`-writability
    /// half of the contract stays the caller's responsibility (this module is
    /// `#![forbid(unsafe_code)]`).
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn init_test_buffer(base: *mut u8) {
        assert!(
            !base.is_null() && (base as usize).is_multiple_of(4),
            "init_test_buffer: base must be non-null and 4-byte-aligned (R2-3 release guard)"
        );
        Self::init_in_place(base, 0)
    }

    /// **Test surface**: the overflow counter's current value (diagnostic). Used
    /// by the isolated ring test to assert `reclaimed + overflowed == pushed`.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn overflow_count(&self) -> u32 {
        self.overflow().load(Ordering::Acquire)
    }

    /// **Test surface** (task: long-run u32 wrap): preset the `head` and `tail`
    /// cursors directly so a test can drive the ring across the `u32::MAX → 0`
    /// boundary without first pushing 2^32 entries. Writes the atomics with
    /// `Release` (mirrors the production drain's `head` publish / push's `tail`
    /// reservation visibility) so a subsequently spawned producer/consumer sees
    /// the preset. MUST be called on a quiescent ring (no concurrent push/drain)
    /// and MUST leave `tail.wrapping_sub(head) <= RING_CAP` (the ring's full
    /// invariant) — the caller is responsible for a consistent preset.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn dbg_set_cursors(&self, head: u32, tail: u32) {
        self.head().store(head, Ordering::Release);
        self.tail().store(tail, Ordering::Release);
    }

    /// **Test surface** (task: long-run u32 wrap): read the current `(head,
    /// tail)` cursor pair. Lets a test assert occupancy (`tail.wrapping_sub(
    /// head)`) across the wrap. `Acquire` loads (uniform with the drain/push).
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn dbg_cursors(&self) -> (u32, u32) {
        (
            self.head().load(Ordering::Acquire),
            self.tail().load(Ordering::Acquire),
        )
    }

    /// Initialise a fresh ring at `base + off`: zero the cursors and mark every
    /// slot `RING_SLOT_EMPTY`. Called by the bootstrap when a small/primordial
    /// segment is reserved. The segment is exclusively owned at init time
    /// (single-writer), so plain writes suffice — no atomics needed here.
    ///
    /// `base + off` MUST point to `FOOTPRINT` writable bytes.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn init_in_place(base: *mut u8, off: usize) {
        let ring = Self::at(base, off);
        // Cursors: zero (empty ring). Plain writes — bootstrap is single-writer.
        Node::write_u32(Node::offset(ring.base, HEAD_OFF) as *mut u32, 0);
        Node::write_u32(Node::offset(ring.base, TAIL_OFF) as *mut u32, 0);
        Node::write_u32(Node::offset(ring.base, OVERFLOW_OFF) as *mut u32, 0);
        // Every slot empty.
        for i in 0..RING_CAP {
            let slot =
                Node::offset(ring.base, SLOTS_OFF + i * core::mem::size_of::<u32>()) as *mut u32;
            Node::write_u32(slot, RING_SLOT_EMPTY);
        }
    }

    /// The `&AtomicU32` head cursor (consumer drain position).
    #[cfg(feature = "alloc-xthread")]
    fn head(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, HEAD_OFF)
    }
    /// The `&AtomicU32` tail cursor (producer reserve position).
    #[cfg(feature = "alloc-xthread")]
    fn tail(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, TAIL_OFF)
    }
    /// The `&AtomicU32` overflow counter (diagnostic; number of discarded
    /// pushes due to a full ring).
    #[allow(dead_code)]
    fn overflow(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, OVERFLOW_OFF)
    }
    /// The `&AtomicU32` slot at reservation index `i` (`i % RING_CAP`).
    #[cfg(feature = "alloc-xthread")]
    fn slot(&self, i: usize) -> &'static core::sync::atomic::AtomicU32 {
        let idx = i % RING_CAP;
        Node::atomic_u32_at(self.base, SLOTS_OFF + idx * core::mem::size_of::<u32>())
    }

    /// Push a freed block's segment-relative `offset` into the ring. Called by
    /// a NON-OWNER thread (a cross-thread freer). Returns `Err(PushOverflow)`
    /// if the ring is full — the caller MUST then discard the block (bounded
    /// leak, sound).
    ///
    /// `offset` MUST be `< SEGMENT` (a real block offset, not the sentinel).
    #[cfg(feature = "alloc-xthread")]
    pub fn push(&self, offset: u32) -> Result<(), PushOverflow> {
        debug_assert_ne!(offset, RING_SLOT_EMPTY, "offset must not be the sentinel");
        loop {
            let t = self.tail().load(Ordering::Relaxed);
            // Full check: reserved-but-undrained count == CAP → full. Acquire
            // on the head load to see the consumer's Release head advance (a
            // slot freed by a drain becomes observable, opening space).
            let h = self.head().load(Ordering::Acquire);
            if t.wrapping_sub(h) >= RING_CAP as u32 {
                // Ring full: bounded leak. Count it (diagnostic, both the
                // per-segment cursor-block counter AND the process-wide D2
                // counter) and bail.
                let _ = self.overflow().fetch_add(1, Ordering::Relaxed);
                DBG_RING_OVERFLOW.fetch_add(1, Ordering::Relaxed);
                return Err(PushOverflow);
            }
            // Reserve slot `t`: CAS tail t → t+1. AcqRel on success — the
            // reservation is the linearization point; Acquire pairs with a
            // prior producer's Release publish (harmless here, but uniform with
            // the drain's view). Relaxed on failure: retry, no side-effect.
            match self.tail().compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Publish: write the offset into the reserved slot. Release
                    // so the consumer's Acquire slot load sees this write.
                    self.slot(t as usize).store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue, // Another producer reserved `t`; retry.
            }
        }
    }

    /// Drain all published offsets from the ring, passing each to `reclaim`.
    /// Called ONLY by the owning thread (single consumer). `reclaim` receives
    /// the block's segment-relative offset; the caller turns it back into a
    /// pointer and routes it to the segment's `BinTable`.
    ///
    /// Stops at the first not-yet-published reserved slot (a producer won the
    /// reservation CAS but hasn't stored the offset yet) — order is preserved by
    /// the cursors, so a later drain picks it up.
    ///
    /// Returns the final `head` value written (i.e. the drain cursor after
    /// this call). PERF-PASS-4 (G9/C2, task #52): callers that maintain an
    /// owner-private cached copy of `head` (to skip future empty drains — see
    /// [`RemoteFreeRing::is_likely_empty`]) use this to refresh their cache
    /// without a second atomic load; callers that don't care simply ignore it
    /// (existing call sites are source- and behaviour-compatible).
    #[cfg(feature = "alloc-xthread")]
    pub fn drain<F: FnMut(u32)>(&self, mut reclaim: F) -> u32 {
        // Acquire: see every producer's Release reservation (tail CAS) and
        // their Release publish (slot store).
        let t = self.tail().load(Ordering::Acquire);
        // Relaxed is sound here despite `head` being written (below) with a
        // Release store and read here without an Acquire: the ring has a SINGLE
        // consumer, but consumer IDENTITY moves with slot ownership. A ring
        // belongs to a segment; when that segment is recycled and re-claimed by
        // a new owner thread, the registry recycle→claim handshake is itself a
        // Release/Acquire pair that establishes happens-before between the
        // previous owner's LAST `head` Release store and the new owner's first
        // drain. So the new owner-consumer is guaranteed to observe the prior
        // owner's final `head` value; no per-load Acquire on `head` is needed
        // because there is never a concurrent writer to `head` — only a prior
        // one, already fenced by the ownership transfer (review B, Finding 4).
        let mut h = self.head().load(Ordering::Relaxed);
        // Wrap-correct drain: both cursors are monotonic wrapping counters
        // (incremented by `wrapping_add(1)`), so the undrained count is
        // `t.wrapping_sub(h)` — NOT `t - h`, which overflows on cursor wrap.
        // `while h < t` would silently stop draining once `tail` wraps past
        // `u32::MAX` while `head` has not, leaking every subsequent offset
        // (and, worse, a later drain could re-process a slot whose offset was
        // already reclaimed before the wrap if `head` were ever advanced past
        // `tail` — impossible while `head <= tail` by the full-check, but the
        // `<` comparison is still wrong and must be `!=`). The full-check in
        // `push` guarantees `t.wrapping_sub(h) < RING_CAP` at all times, so
        // `h == t` is exactly the empty condition and `h != t` the non-empty
        // one — order is preserved by the cursors, never by the comparison.
        while h != t {
            let slot = self.slot(h as usize);
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                // Reserved but not yet published. Cannot skip (cursor order);
                // a later drain will pick it up once the producer publishes.
                break;
            }
            // Reclaim the offset. Done BEFORE clearing the slot so a concurrent
            // producer cannot reuse this slot before we've consumed it (the
            // full-check prevents reuse while undrained, and clearing marks it
            // drained for the next wrap).
            reclaim(off);
            // Clear the slot for the next wrap. Relaxed: the next producer to
            // touch this slot will Release-store its offset; our drain reads
            // Acquire. No cross-thread dependency on this clear's ordering.
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        // Publish the new head so producers' full-check sees the freed space.
        // Release: pairs with their Acquire head load in `push`.
        self.head().store(h, Ordering::Release);
        h
    }

    /// Whether the ring is likely empty (momentary observation). Heuristic —
    /// another thread may push immediately after this returns `true`. Used to
    /// skip the drain path when the ring is likely empty.
    #[cfg(feature = "alloc-xthread")]
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        let t = self.tail().load(Ordering::Acquire);
        let h = self.head().load(Ordering::Acquire);
        t == h
    }

    /// PERF-PASS-4 (G9/C2, task #52) — pre-drain empty-guard primitive: a
    /// cheap Relaxed load of `tail` ONLY (no `head` load at all — the caller
    /// already holds its own owner-private cached copy of `head`, refreshed
    /// from [`drain`](Self::drain)'s return value).
    ///
    /// **Why `Relaxed` is sound here (extends the existing single-consumer
    /// argument at [`drain`](Self::drain)'s doc comment):** the sole purpose
    /// of this load is to decide "has ANY producer reserved a slot since we
    /// last drained". A push's `tail` CAS is `AcqRel`; a Relaxed load here may
    /// observe an OLDER value of `tail` than the most recent CAS (no
    /// synchronizes-with edge), but it can NEVER observe a value that skips a
    /// real advance: `tail` is monotonic (only ever `wrapping_add(1)`-ed by a
    /// winning CAS), so ANY Relaxed load of it returns either the cached
    /// value or a LATER one — never a value that hides a genuine push. Three
    /// outcomes:
    ///   - `tail_relaxed() == cached_head` → the ring is PROVABLY unchanged
    ///     since the cache was taken (no push can have landed without moving
    ///     `tail` off `cached_head`, and `cached_head` was itself set FROM a
    ///     real `head` value that only advances up to a real `tail`) — safe
    ///     to skip the drain entirely.
    ///   - `tail_relaxed() != cached_head` but a push landed AFTER this load
    ///     returns → exactly the same as today's drain missing a push that
    ///     lands after `drain`'s own `tail.load(Acquire)` returns: the
    ///     "later drain picks it up" contract (module docs) already covers
    ///     this window, unconditionally, regardless of whether THIS call
    ///     skipped or ran a real drain.
    ///   - A push landed and is visible: `tail_relaxed() != cached_head`, the
    ///     caller falls through to a real `drain()`, which re-establishes
    ///     ordering via its own `Acquire` tail load — this Relaxed load is
    ///     ONLY a pre-filter, never the operation that reads the pushed data.
    ///
    /// The slot re-claim boundary (a segment's ring surviving a `HeapSlot`
    /// recycle→claim, per the shard-reuse discipline — see
    /// `HeapRegistry::abandon_segments`'s module doc) needs NO extra fence
    /// here: the cache lives in the segment's OWN header
    /// (`SegmentHeader::ring_drain_head`), which is reset to `0` only when a
    /// segment is freshly reserved (`SegmentHeader::small`), exactly mirroring
    /// the ring's own `head`/`tail` reset in `RemoteFreeRing::init_in_place`
    /// at the SAME call site (`reserve_small_segment`). A recycled `HeapSlot`
    /// re-claimed by a new owner thread reuses the SAME `HeapCore` (and hence
    /// the SAME live segments/rings) whole — there is no "new owner, old
    /// ring" combination in this codebase's shard-reuse model, so there is no
    /// window where a stale cached head from a different logical owner could
    /// leak across a re-claim.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn tail_relaxed(&self) -> u32 {
        self.tail().load(Ordering::Relaxed)
    }
}
