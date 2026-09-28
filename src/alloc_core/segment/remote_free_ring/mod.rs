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
//!   atomic access on the production push/drain protocol goes through
//!   `Node::atomic_u32_at` / `atomic_u64_at` — there is NO `unsafe` in that
//!   protocol itself. This module (`remote_free_ring_impl.rs`, re-exported
//!   here) DOES carry two item-scoped, `#[doc(hidden)]` tier-2 `unsafe fn`s
//!   in its `ops.rs` child (`over_test_buffer` / `init_test_buffer`, each
//!   with its own `#[allow(unsafe_code)]` and `# Safety` contract) for
//!   constructing a `RemoteFreeRing` view over a caller-owned buffer outside
//!   a real segment — the established doc-hidden test-only-export pattern
//!   (reachable only via `sefer_alloc::alloc_core::remote_free_ring` under
//!   `internals`, per `src/lib.rs`'s module doc); no production code path
//!   calls either (oxx R2-07).
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
//!   │  • overflow: AtomicU32 (4 B) — count of full-ring pushes  │
//!   │    (the caller routes legal frees to another tier)       │
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
//! ## Overflow semantics
//!
//! When the ring is full (`tail - head == CAP`) or the lifetime cursor is
//! exhausted (`tail == u64::MAX`), a push returns
//! `Err(PushOverflow)`. The production caller first tries the heap-level
//! overflow ring, then bounded retries, then R2-09's allocation-free
//! intrusive spill in the freed block. A full segment ring is therefore a
//! routing event, not a lost free. Direct users of this primitive must
//! provide their own overflow policy; the ring itself stores no failed push.
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
mod remote_free_ring_impl;
pub use remote_free_ring_impl::*;
