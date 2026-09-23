//! [`SegmentHeader`] — the per-segment metadata block that lives at offset 0
//! of every segment, and [`PageMap`] / [`BinTable`] — the per-segment page
//! descriptors and per-size-class free bins, all carved from segment memory.
//!
//! These structures are the **self-hosted metadata** of the Phase 8 substrate
//! (§3 / §5 P8 of `ALLOC_PLAN.md`): they live INSIDE the segments they
//! describe, not in a `Vec`/`HashSet` on the global allocator. This is the
//! Membrane Inversion — the safe slot-table discipline governs OS memory
//! instead of consuming `std` collections.
//!
//! ## This file is PURE SAFE DATA + ARITHMETIC
//!
//! Every raw memory touch goes through the [`node`](crate::alloc_core::node) seam. This
//! file declares only `#[repr(C)]` struct layouts, `const` offsets, and
//! methods that compute indices / route reads & writes through `Node`. There
//! is NO `unsafe` here — so the crate's structural promise ("`unsafe` lives
//! ONLY in `os` + `node`") is upheld by the compiler.
//!
//! ## Layout of a small segment
//!
//! ```text
//!   SEGMENT-aligned base
//!   ┌─────────────────────────────────────────────────────────────┐
//!   │ SegmentHeader (fixed-size, page-0)                          │
//!   │  • magic, kind, segment_id                                  │
//!   │  • bump cursor (next uncarved page offset, in bytes)        │
//!   │  • BinTable:  per-class free-list head OFFSETS (u32 each)   │
//!   │  • PageMap:    per-page descriptor (which class, or free)   │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ payload pages (carved bump-allocated into class runs)       │
//!   └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! `segment_of(ptr)` masks the low bits of `ptr` to find the segment base in
//! O(1); the header at offset 0 then tells the Cartographer everything about
//! that segment. Large/huge segments carry only `(size, align)` of their
//! single allocation — no page map (one allocation per segment).

// Items carved into the `descriptors` / `layout_asserts` siblings are
// re-flattened here so every one keeps its exact `segment_header::` path
// (the original single-file module scope).
#[path = "descriptors.rs"]
mod descriptors;

#[path = "layout_asserts.rs"]
mod layout_asserts;

#[path = "segment_header_impl.rs"]
mod implementation;
pub use implementation::*;
