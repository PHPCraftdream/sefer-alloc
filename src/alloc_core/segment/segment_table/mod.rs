//! [`SegmentTable`] — the global registry of all live segments, SELF-HOSTED in
//! the primordial segment's payload (not a `Vec` / `Box` on the global
//! allocator).
//!
//! This is the keystone of the Phase 8 Membrane Inversion (§1 of
//! `ALLOC_PLAN.md`): the safe slot-table discipline stops *consuming* memory
//! (via `Vec`/`HashSet`) and starts *governing* it. The registry lives inside
//! the very segments it tracks, so the alloc path can mutate it without ever
//! calling the global allocator (M5 — reentrancy-freedom).
//!
//! ## Design
//!
//! - A fixed-capacity array of segment-base pointers, carved from the
//!   primordial segment at a known offset. Capacity is bounded by
//!   [`MAX_SEGMENTS`] — the **high-water mark** of ever-simultaneously-live
//!   segments, not a hard per-workload cap. Under `alloc-decommit`, empty
//!   segments are recycled: their slot is set to NULL and reused by a future
//!   `register`, so the effective live-segment count is unbounded (task #60
//!   — slot-recycle variant B).
//! - `segment_count` is the **high-water mark** of slots ever written.  It
//!   never decreases.  A NULL entry in `slots[0..count)` is a **recyclable
//!   slot** — the OS reservation for that segment has already been released
//!   (by [`recycle`](SegmentTable::recycle)); the slot is available for the
//!   next [`register`](SegmentTable::register) call.
//! - `drop` walks only non-NULL slots and frees each OS reservation. NULL
//!   slots are already freed and skipped.
//! - O(1) `segment_of(ptr) = ptr & ~(SEGMENT-1)` lives in [`crate::alloc_core::os`] and
//!   yields the segment base; routing then reads the header at offset 0. The
//!   table is only needed for census/drop, not the hot path.
//!
//! ## Safety
//!
//! The table is plain data (a `*mut u8` array + a count) laid down in segment
//! memory; mutation goes through the [`node`](crate::alloc_core::node) seam's pointer
//! writes for the slots. The bootstrap (`crate::alloc_core::bootstrap`) is the ONLY place
//! that hand-writes the table in place before the safe Cartographer takes over.
//!
//! ## Slot encoding (task #60 — NULL = recyclable)
//!
//! - A **live slot** holds the segment base pointer (non-NULL, SEGMENT-aligned).
//! - A **recyclable slot** holds `null_mut()`. The corresponding OS reservation
//!   has already been released; the virtual address is no longer valid.
//! - [`register`] scans left-to-right for a recyclable slot first, reusing it
//!   before appending. This keeps `count` at its high-water mark and never
//!   wastes live slots.
//! - [`recycle`] finds the slot for a given base, releases the OS reservation,
//!   then writes NULL. These two operations happen in `decommit_empty_segment`
//!   as a unit, so there is never a window where the OS reservation is released
//!   but the slot is still non-NULL (which would cause `drop` to double-free).

#[path = "harness.rs"]
mod harness;

#[path = "hash.rs"]
mod hash;

#[path = "segment_table_impl.rs"]
mod implementation;
pub use implementation::*;
