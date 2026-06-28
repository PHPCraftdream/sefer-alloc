//! [`RemoteFreeRing`] — a per-segment, bounded, **non-intrusive** MPSC queue
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
//! - IS: pure safe data + arithmetic over the [`node`](super::node) seam. Every
//!   atomic access goes through [`Node::atomic_u32_at`] (a confined-`unsafe`
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
//!   │  • head: AtomicU32  (4 B) — drain cursor (consumer)      │
//!   │  • tail: AtomicU32  (4 B) — push reserve cursor (producers)
//!   │  • overflow: AtomicU32 (4 B) — count of discarded pushes  │
//!   │    (ring-full → bounded leak; sound, never corrupts)      │
//!   │  • pad: 4 B (align slots to 8)                           │
//!   │  • slots: [AtomicU32; RING_CAP]  (RING_CAP × 4 B)         │
//!   │    each slot holds a block offset or RING_SLOT_EMPTY      │
//!   └──────────────────────────────────────────────────────────┘
//! ```
//!
//! `FOOTPRINT = 16 (cursor block) + RING_CAP * 4`. With `RING_CAP = 256` that
//! is 1040 bytes per segment — under one page, negligible vs. the 4 MiB
//! segment, and amortised across all blocks the segment serves.
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

use core::sync::atomic::Ordering;

use super::node::Node;

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
/// `off < 2^22` (a segment offset) and `class_idx < SMALL_CLASS_COUNT (= 40)`,
/// so the result is `< 2^32` and never collides with `RING_SLOT_EMPTY`
/// (`u32::MAX`) for any real block.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
pub(crate) fn pack_entry(off: u32, class_idx: u32) -> u32 {
    debug_assert!(off <= ENTRY_OFF_MASK, "offset overflows ring-entry field");
    off | (class_idx << ENTRY_OFF_BITS)
}

/// Unpack a ring entry into `(offset, class_idx)`.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
pub(crate) fn unpack_entry(packed: u32) -> (u32, u32) {
    (packed & ENTRY_OFF_MASK, packed >> ENTRY_OFF_BITS)
}

/// The cursor block: `head`, `tail`, `overflow`, and a 4-byte pad so the slot
/// array starts 8-aligned (harmless on `AtomicU32` but tidy). 16 bytes total.
const CURSOR_BLOCK: usize = 4 * core::mem::size_of::<u32>();

/// Offset of the `head` cursor within the ring metadata.
const HEAD_OFF: usize = 0;
/// Offset of the `tail` cursor within the ring metadata.
const TAIL_OFF: usize = 4;
/// Offset of the `overflow` counter within the ring metadata.
const OVERFLOW_OFF: usize = 8;
/// Offset of the first slot within the ring metadata.
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
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn over_test_buffer(base: *mut u8) -> Self {
        Self::at(base, 0)
    }

    /// **Test surface**: initialise a fresh ring at `base` (offset 0). Same as
    /// [`init_in_place`](Self::init_in_place) but for a standalone buffer (no
    /// segment-relative offset). See [`over_test_buffer`](Self::over_test_buffer).
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn init_test_buffer(base: *mut u8) {
        Self::init_in_place(base, 0)
    }

    /// **Test surface**: the overflow counter's current value (diagnostic). Used
    /// by the isolated ring test to assert `reclaimed + overflowed == pushed`.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn overflow_count(&self) -> u32 {
        self.overflow().load(Ordering::Acquire)
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
                // Ring full: bounded leak. Count it (diagnostic) and bail.
                let _ = self.overflow().fetch_add(1, Ordering::Relaxed);
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
    #[cfg(feature = "alloc-xthread")]
    pub fn drain<F: FnMut(u32)>(&self, mut reclaim: F) {
        // Acquire: see every producer's Release reservation (tail CAS) and
        // their Release publish (slot store).
        let t = self.tail().load(Ordering::Acquire);
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
}
