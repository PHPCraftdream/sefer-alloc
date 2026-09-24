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
//! - IS: pure safe data + arithmetic over the `node` (`crate::alloc_core::node`) seam. Every
//!   atomic access goes through `Node::atomic_u32_at` / `atomic_u64_at`.
//!   There is NO `unsafe`
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
//!   │  • head: AtomicU64  (8 B) — drain cursor (consumer)      │
//!   │  • [56 B reserved padding]                                │
//!   │  offset 64..128 (own cache line — producer-touched):     │
//!   │  • tail: AtomicU64  (8 B) — push reserve cursor (producers)
//!   │  • overflow: AtomicU32 (4 B) — count of discarded pushes  │
//!   │    (ring-full → bounded leak; sound, never corrupts)      │
//!   │  • cached_head: AtomicU64 (8 B) — F10 shadow-head hint,   │
//!   │    same line as tail/overflow (producer-only touched)     │
//!   │  • [40 B reserved padding]                                │
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
//! ## MPSC reservation protocol
//!
//! Cursors are non-wrapping u64 values. The ring supports at most
//! `u64::MAX` successful reservations over its entire lifetime. At
//! `tail == u64::MAX`, both push variants return `PushOverflow` permanently;
//! draining does not rebase the cursors. This is an enforced exhaustion
//! contract, not an assumption about throughput or scheduler delay. A
//! producer paused between its capacity check and tail CAS can never see
//! the same tail value again after any other reservation has succeeded.
//!
//! A producer loads `tail`, checks room against `cached_head` (Acquire),
//! then, if the cache is insufficient, loads the real `head` (Acquire)
//! and refreshes `cached_head` (Release). The cache can be stale-low,
//! including when an older refresh store races a newer one. Since neither
//! cursor wraps, stale-low can only force a slow check, not admit an
//! over-capacity reservation. An observed cache value ahead of a stale
//! tail snapshot also forces the slow check. The Acquire/Release cache
//! handoff carries the consumer's prior slot-clear before recycled-slot
//! publication. The successful tail CAS (AcqRel) reserves one index;
//! a failed CAS retries the entire check. A producer Release-stores the
//! offset into its reserved slot.
//!
//! The single consumer Acquire-loads tail and each slot. It stops at the
//! first reserved-but-unpublished slot. After reclaiming an entry it clears
//! that slot, increments head, and a Drop guard Release-publishes head,
//! including during unwind. Slot identity is `cursor % RING_CAP`, and
//! `head <= tail` with `tail - head <= RING_CAP` is maintained. The
//! power-of-two capacity pin is retained for the established layout and
//! indexing contract; cursor arithmetic no longer wraps.
//! The four head write sites are the drain guard's Release store,
//! exclusive bootstrap's zero store, and the two quiescent test hooks
//! (`dbg_set_cursors`, `dbg_advance_head_only`). Only the drain guard
//! writes head during production operation.
//!
//! The owner-private `SegmentHeader::ring_drain_head` cache is still u32.
//! To avoid a false empty verdict after its representable range, the
//! ring's guard-facing `tail_relaxed` returns `u32::MAX` whenever the
//! real tail reaches that value, while `drain` returns 0 once head
//! reaches it. Before the boundary, both report exact u32 cursors.
//! Thus the guard's equality shortcut is disabled forever after the
//! boundary without changing the header layout; a real drain still uses
//! the full u64 cursors. This loses only the empty-guard optimization.
//!
//! Reduced-width loom tests exercise a producer paused before CAS through
//! an entire finite cursor lifetime and terminal exhaustion; the old
//! wrapping protocol is retained there as a counterfactual. Kani checks
//! non-wrapping occupancy arithmetic; it does not prove the concurrent
//! interleaving protocol.
//! ## Overflow semantics (the honest remainder)
//!
//! When the ring is full (`tail - head == CAP`) or the lifetime cursor is
//! exhausted (`tail == u64::MAX`), a push returns
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
//!
//! ## R2-10 — tail-CAS ABA closed by non-reuse
//!
//! The former u32 wrapping tail could revisit a stalled producer's
//! capacity-checked compare value after a full incarnation. The u64 tail
//! now has a checked terminal value and never wraps or rebases. Therefore,
//! if another producer has reserved even one slot since the stalled
//! producer's snapshot, its CAS compare value can never match again.
//! The same rule holds at u64 exhaustion: no successor is attempted.
//! See `tests/loom_remote_ring_tail_aba.rs` for the reduced-width
//! full-lifetime pause/resume model and the old protocol's negative control.
use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

mod ops;

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

/// F10 (task #502) path-activation oracle: process-wide count of
/// [`RemoteFreeRing::full_check`] calls that took the FAST (shadow-hit)
/// path — `cached_head` alone proved the ring had room, no real
/// `head.load(Acquire)` was issued. `bench-internals`-gated: this is a
/// measurement-only counter with no production caller, so it defaults to the
/// narrowest gate per CLAUDE.md's benchmark-hook rule (never widens a plain
/// `production` build's surface). Relaxed: diagnostic only.
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
pub static DBG_RING_PUSH_SHADOW_FAST: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// F10 (task #502) path-activation oracle: process-wide count of
/// [`RemoteFreeRing::full_check`] calls that took the SLOW (real
/// `head.load(Acquire)`) path — the shadow suggested the ring might be full
/// (or had never been refreshed) and a genuine cross-core-visible load was
/// issued. `bench-internals`-gated, same rationale as
/// [`DBG_RING_PUSH_SHADOW_FAST`]. `DBG_RING_PUSH_SHADOW_FAST +
/// DBG_RING_PUSH_SHADOW_SLOW` is the total number of `full_check` calls
/// (i.e. of push attempts, counted or uncounted) since process start — a
/// gate's harness uses the SLOW/(FAST+SLOW) ratio as its regime oracle: near
/// 0 proves the "favorable" (rarely-full) regime; near 1 proves the
/// "adversarial" (often-full) regime.
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
pub static DBG_RING_PUSH_SHADOW_SLOW: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

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

// The ring's u64 cursors never wrap. Keep the power-of-two layout pin so
// slot indexing stays consistent with the established layout contract.
const _: () = assert!(
    RING_CAP.is_power_of_two(),
    "RING_CAP must remain power-of-two for the established ring layout"
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

// R6-OPT-P0-3a (correctness-surface item #6, "cross-thread free" / packed
// `(offset, class)` bit budget): the non-hardened packing above reserves 22
// bits for `off` and the remaining `32 - 22 = 10` bits (values 0..=1023) for
// `class_idx`. `medium-classes` (R6-OPT-P0-3a) grows `SMALL_CLASS_COUNT` from
// 49 to 55 — nowhere near the 10-bit ceiling, so this packing has ample
// headroom (the review's own §4 P0-3 correctness-surface list explicitly
// calls for verifying this, even when it "technically still fits" — see the
// task's final report). Two things must hold for every real
// `(off, class_idx)` pair: `class_idx` must fit in the 10 high bits
// (`SMALL_CLASS_COUNT <= 1024`), and the packed word must never equal
// `RING_SLOT_EMPTY` (`u32::MAX`) — which only happens when EVERY bit is 1,
// i.e. `off == ENTRY_OFF_MASK` (0x3FFFFF, a real reachable last-block offset)
// AND `class_idx == 1023` (0x3FF). The second conjunct is what this assert
// closes: as long as the maximum REAL class index (`SMALL_CLASS_COUNT - 1`)
// stays strictly below 1023, no real pair can produce the sentinel. (Compare
// the `hardened` packing's identical-shaped guard further down this file,
// which pins the SAME property for its own, much tighter 6-bit class field.)
const _: () = assert!(
    (SMALL_CLASS_COUNT as u32) < (1u32 << (32 - ENTRY_OFF_BITS)) - 1,
    "the non-hardened ring entry's class field is 32 - ENTRY_OFF_BITS bits wide; \
     SMALL_CLASS_COUNT must stay strictly below its all-ones value so a real \
     (offset, class) pair can never collide with RING_SLOT_EMPTY (u32::MAX)"
);

/// Pack a `(offset, class_idx)` pair into a single `u32` ring entry.
/// `off < 2^22` (a segment offset) and `class_idx < SMALL_CLASS_COUNT (= 49
/// without `medium-classes`, 55 with it)`, so the result is `< 2^32` and
/// never collides with `RING_SLOT_EMPTY` (`u32::MAX`) for any real block —
/// see the compile-time pin immediately above.
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
///
/// Task #2000: `#[cfg_attr]` widened to mirror [`pack_entry`]'s own —
/// `reclaim_offset` no longer inlines its own `unpack_entry` call (it
/// delegates to `reclaim_offset_checked`, whose `hardened`/non-hardened
/// unpack split lives in ITS body, same shape as `entry_class_idx` above),
/// so under a `hardened` build this fn has no live caller left, same as
/// `pack_entry` already anticipated.
#[cfg_attr(
    any(not(feature = "alloc-xthread"), feature = "hardened"),
    allow(dead_code)
)]
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
// real small class index is `SMALL_CLASS_COUNT - 1 = 48` (`0x30`) without
// `medium-classes`, or `54` (`0x36`) WITH it (R6-OPT-P0-3a: 49 -> 55
// classes), so the class field never reaches `0x3F` either way. The maximum
// packed word over real ranges is therefore `0xFFC3_FFFF < u32::MAX` without
// `medium-classes` (computed and pinned by the
// `entry_never_collides_with_ring_slot_empty` regression test) — WITH
// `medium-classes` the maximum real class value shifts from `0x30` to `0x36`
// but stays strictly below `0x3F`, so the same non-collision argument holds,
// just with a NARROWER margin. This safety HOLDS ONLY WHILE
// `SMALL_CLASS_COUNT <= 62` — the const-assert below pins that the class
// field's all-ones value (`2^ENTRY_CLASS_BITS - 1 = 63`) stays strictly above
// `SMALL_CLASS_COUNT - 1`, so a future bump of `SMALL_CLASS_COUNT` past 62
// cannot silently reintroduce a collision. Ф3's ring `push`/`drain` reuse is
// sound under that invariant.
//
// R6-OPT-P0-3a HONEST MARGIN NOTE (correctness-surface item #6, "cross-thread
// free" — the task's own instruction to flag a tight fit explicitly even when
// it technically still fits): `medium-classes` consumes 6 of the 6-bit
// field's 14 headroom values (49 -> 55 used, ceiling 62) — plenty of room for
// THIS experiment (55 << 62), but this field is measurably tighter than the
// non-hardened packing's 10-bit field (ceiling 1022, headroom in the
// hundreds). A THIRD source of classes stacked on top of `medium-classes`
// under `hardened` (e.g. a future page-run layer's own per-run classes, if
// P0-3b ever reuses this SAME packed-word scheme rather than a dedicated one)
// would need to re-check this 62-class ceiling explicitly — it is the first
// of this crate's two ring-entry encodings to feel `medium-classes`' growth
// at all.
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
// are referenced via `crate::alloc_core::` paths (this file imports only `SMALL_CLASS_COUNT`); the
// hardened-only `use` is colocated with the asserts so it is invisible to a
// non-hardened compile.
#[cfg(feature = "hardened")]
const _: () = {
    use crate::alloc_core::os::SEGMENT;
    use crate::alloc_core::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT, SMALL_CLASS_COUNT};
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
        off >> crate::alloc_core::size_classes::MIN_BLOCK_SHIFT <= ENTRY_OFF16_MASK,
        "offset overflows hardened ring-entry off16 field"
    );
    debug_assert!(
        off.is_multiple_of(crate::alloc_core::size_classes::MIN_BLOCK as u32),
        "hardened ring-entry offset must be MIN_BLOCK-aligned (off16 = off >> MIN_BLOCK_SHIFT)"
    );
    debug_assert!(
        class_idx <= ENTRY_CLASS_MASK,
        "class_idx overflows hardened ring-entry class field"
    );
    let off16 = off >> crate::alloc_core::size_classes::MIN_BLOCK_SHIFT;
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
    let off = off16 << crate::alloc_core::size_classes::MIN_BLOCK_SHIFT;
    (gen, class_idx, off)
}

/// R8-1 (task #214): extract ONLY the class index from a packed ring entry,
/// without paying for the full unpack (offset/generation are not needed by the
/// ring-drain call sites that just want to know WHICH classes a drain pass
/// touched, so they can drive an incremental directory sync instead of
/// re-sweeping all `SMALL_CLASS_COUNT` classes — see
/// `AllocCore::sync_directory_for_segment_classes`).
///
/// Dispatches to the existing [`unpack_entry`] / [`unpack_entry_hardened`]
/// (matching the build's packing scheme) rather than re-deriving the bit
/// layout, so this stays correct by construction if either packing changes.
///
/// Compiled under `alloc-xthread` (the only build where ring drains run); the
/// `hardened`/non-hardened split lives in the BODY (not on the function's own
/// cfg), so the function is reachable in both hardened and non-hardened
/// `alloc-xthread` builds. `hardened` implies `fastbin` implies
/// `alloc-xthread`, so [`unpack_entry_hardened`] is always in scope when this
/// function's hardened arm compiles.
#[cfg(feature = "alloc-xthread")]
#[inline(always)]
pub(crate) fn entry_class_idx(packed: u32) -> usize {
    #[cfg(feature = "hardened")]
    {
        let (_gen, class_idx, _off) = unpack_entry_hardened(packed);
        class_idx as usize
    }
    #[cfg(not(feature = "hardened"))]
    {
        let (_off, class_idx) = unpack_entry(packed);
        class_idx as usize
    }
}

// R8-1 (task #214): the incremental directory sync driven by
// `entry_class_idx` packs the classes a drain pass touched into a `u64`
// bitmask (one bit per class). That design is valid only while
// `SMALL_CLASS_COUNT <= 64` (a wider class space would need a wider mask).
// Pin it at compile time so a future bump past 64 fails HERE instead of
// silently truncating the bitmask at runtime. `SMALL_CLASS_COUNT` is already
// imported at the top of this file (`use crate::alloc_core::size_classes::...`), matching
// the sibling const-assert above that references the same constant.
#[cfg(feature = "alloc-xthread")]
const _: () = assert!(
    SMALL_CLASS_COUNT <= 64,
    "entry_class_idx-based incremental directory sync packs touched classes \
     into a u64 bitmask; SMALL_CLASS_COUNT must stay <= 64"
);

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
/// moved from 8 to 68, then to 72 for the u64 tail — shares `tail`'s line;
/// `overflow` is only written on the rare full-ring path, so co-locating it
/// with `tail` costs nothing on the common push path and avoids spending a
/// THIRD cache line on one counter).
const OVERFLOW_OFF: usize = 72;
/// F10 (task #502): offset of the `cached_head` shadow within the ring
/// metadata — 80, immediately after `overflow` (72) on the SAME producer
/// line as `tail`/`overflow`. Was unused reserved padding (bytes 72..128 of
/// the cursor block were entirely unclaimed before this task — confirmed by
/// grepping every other `_OFF` constant in this file: none references any
/// offset in `[72, 128)`). A producer's full-check reads this field on the
/// SAME cache line as its own `tail` load, instead of the CONSUMER's `head`
/// line — see the module doc's "F10 — shadow/cached head" section for the
/// full soundness argument. `CURSOR_BLOCK` (128) is unchanged, so
/// `FOOTPRINT`/`SLOTS_OFF` and every downstream segment-metadata offset are
/// byte-identical to before this task.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const CACHED_HEAD_OFF: usize = 80;
/// Offset of the first slot within the ring metadata. PERF-PASS-4: moved
/// from 16 to 128 (`CURSOR_BLOCK`) — the data slots now start on a line past
/// BOTH cursor lines, so neither producer's `tail` CAS nor the consumer's
/// `head` store dirties a line the other side is scanning for data.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const SLOTS_OFF: usize = CURSOR_BLOCK;

// F10 (task #502): pin that `cached_head` fits strictly within the existing
// `CURSOR_BLOCK` padding without colliding with `SLOTS_OFF` — a future
// CURSOR_BLOCK shrink (unlikely, but this is exactly the kind of silent
// layout hazard the module's other compile-time pins exist to catch) would
// fail HERE instead of corrupting ring data at runtime.
const _: () = assert!(
    CACHED_HEAD_OFF + core::mem::size_of::<u64>() <= CURSOR_BLOCK,
    "F10's cached_head field (CACHED_HEAD_OFF..+8) must fit inside CURSOR_BLOCK, \
     strictly before SLOTS_OFF"
);
const _: () = assert!(
    HEAD_OFF.is_multiple_of(8) && TAIL_OFF.is_multiple_of(8) && CACHED_HEAD_OFF.is_multiple_of(8)
);
const _: () = assert!(HEAD_OFF + 8 <= 64 && TAIL_OFF + 8 <= OVERFLOW_OFF);
const _: () = assert!(OVERFLOW_OFF + 4 <= CACHED_HEAD_OFF);

/// The per-segment non-intrusive cross-thread-free MPSC ring.
///
/// A thin view over in-segment metadata (no allocation — the bootstrap carves
/// the bytes at [`crate::alloc_core::segment_header::Layout::remote_ring_off`]). Producers
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

/// F-7 (R34-17/task #536) — RAII guard that publishes [`RemoteFreeRing::drain`]'s
/// `head` cursor on drop, so a `reclaim` closure that unwinds mid-drain still
/// commits the progress made before the panic. Mirrors the `LockGuard`
/// (`global::fallback`, task L4) panic-safety pattern already in this crate.
/// The guard is the SOLE
/// writer of `head` on the drain path: the pre-F-7 explicit
/// `head.store(h, Release)` after the loop was removed in favour of this `Drop`,
/// so there is exactly one publish whether the drain completes normally or
/// unwinds.
///
/// **Exact contract (Sol-F5, task #567 — release-readiness review finding
/// F5, `docs/reviews/2026-08-05-sol-release-readonly-review.md`).** This
/// guard is unwind-safe against **losing already-fully-processed prior
/// elements**: every offset whose `reclaim` call returned normally, and
/// whose slot was cleared, before the panicking iteration is still
/// published (not re-drained, not silently dropped). It does **NOT**
/// provide exactly-once semantics for the **specific element being
/// processed when the panic occurs**. `drain`'s loop body
/// (`RemoteFreeRing::drain`, below) calls `reclaim(off)` BEFORE clearing
/// the slot and BEFORE advancing/publishing `h` — so if `reclaim(off)`
/// mutates allocator/external state and THEN panics, the slot is left
/// non-empty and `h` is left one short: a `catch_unwind`ing caller that
/// resumes draining will re-pass that SAME `off` to `reclaim` on the next
/// call, i.e. `reclaim` may run twice (once mutating state, then again
/// after resume) for the element that was in flight at panic time. This is
/// a property of the drain loop's `reclaim → clear → advance` ORDER
/// (`RemoteFreeRing::drain`'s loop body), not something this guard's
/// publish-on-drop can fix by itself — the guard only ever publishes `h`
/// values that were fully advanced past a cleared slot.
///
/// Production `reclaim` closures (`AllocCore::reclaim_offset` /
/// `AllocCore::reclaim_offset_checked`, `src/alloc_core/alloc_core_small_reclaim.rs`,
/// called from the three drain sites in `src/alloc_core/alloc_core_small.rs`)
/// are, by inspection of their current bodies, not currently known to panic
/// after mutating state — the reclaim functions themselves are ordinary
/// `pub(crate) fn`s returning `bool` with no `unwrap`/`expect`/`panic!`/
/// unchecked indexing on their mutation-bearing paths, and the calling
/// closures only perform a decrement/OR-accumulate afterward. But this is
/// an observation about the code AS WRITTEN, not a structural guarantee —
/// nothing in the type system prevents a future `reclaim` closure (or a
/// direct/internal `catch_unwind` caller of `drain`) from panicking after a
/// mutation. Achieving true exactly-once-under-unwind would need a
/// two-phase/idempotent reclaim protocol (or an explicit poison/skip
/// policy), which is out of scope for this guard — see the review finding
/// for the fuller discussion. In practice this residual is reachable only
/// through a direct/internal `catch_unwind` around `drain`: an unwind that
/// escapes through the `GlobalAlloc` entry points still aborts the process
/// (see `src/global/sefer_alloc.rs`'s panic-tripwire docs), so the replay
/// window described here cannot be observed through ordinary allocator
/// usage. Tracked as `docs/CORRECTNESS_OPEN_ITEMS.md` item 22 (task #575/H5).
#[cfg(feature = "alloc-xthread")]
struct DrainHeadPublish {
    head: &'static core::sync::atomic::AtomicU64,
    h: u64,
}
