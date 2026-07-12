//! [`RunStack`] — the per-segment **run-encoded freelist** (PERF-3, full
//! Ф1–Ф4 implementation), carved from segment metadata under
//! `#[cfg(feature = "alloc-runfreelist")]`.
//!
//! **VERDICT: Ф5 reached NO-GO (honest-reject)** — see
//! `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md` §Verdict (and the
//! `alloc-runfreelist` feature block in the root `Cargo.toml` for the full
//! disposition). The measured cold/recycle targets regressed +23 %–+31 % (Ir)
//! instead of the predicted ≥5 % improvement, so this feature is NOT part of
//! `production`, is off by default, and is not under active development. The
//! Ф1–Ф4 source is deliberately retained: correct, reviewed and
//! regression-tested, compiles to nothing under the default feature set, kept
//! as a ready starting point for a possible future re-run.
//!
//! The run-encoded freelist (docs/design/RUN_ENCODED_FREELIST_PLAN.md) stores
//! compact `(start_off, count)` descriptors for contiguous freed-block runs,
//! so the recycle path (magazine flush → segment freelist → later refill
//! drain) can reconstruct block addresses by **stride arithmetic** instead of
//! the dependent-load pointer chase through per-block intrusive `next` words
//! that the classic linked freelist pays (plan §1 — the attacked mechanism).
//!
//! The behavioural wiring (the Ф2–Ф4 call sites that consult/mutate a
//! `RunStack`) lives in `alloc_core_small.rs` and `alloc_core_small_pool.rs`
//! under the same `alloc-runfreelist` feature: Ф2 flush (detect-contiguous-run
//! + `push`), Ф3 drain (reconstruct-from-descriptor + `pop`), Ф4 decommit
//! lifecycle (`clear_all`). This file itself holds only the storage layout,
//! init, and accessors.
//!
//! ## What this module IS and is NOT
//!
//! - IS: pure safe data + arithmetic over the [`node`](super::node) seam, like
//!   [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) /
//!   [`BinTable`](super::segment_header::BinTable). Every raw memory touch
//!   goes through the existing `Node::read_struct` / `Node::write_struct`
//!   primitives (the same whole-record typed read/write the segment header
//!   itself uses — `RunDesc` is `Copy`, so a typed read/write is a plain bit
//!   copy with no drop glue). There is NO `unsafe` here — the crate's
//!   structural promise ("`unsafe` lives ONLY in `os` + `node`") is upheld by
//!   the compiler.
//! - IS NOT: a source of truth for free/allocated state. The
//!   [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) remains the SOLE ground
//!   truth (plan §2.3 — M2 invariant, load-bearing). A `RunDesc` is a
//!   fast-path HINT for reconstructing addresses; every reconstructed block is
//!   still re-checked against the bitmap at drain time (Ф3, defense-in-depth).
//!
//! ## No atomics (single-writer)
//!
//! A segment's `RunStack` is written ONLY by the segment's owner: own-thread
//! flush (Ф2) and the owner-side `decommit_empty_segment` clear (Ф4) both run
//! on the owner. Cross-thread frees never touch the `RunStack` — they always
//! go through the classic linked freelist (plan §2.4 — structural: a single
//! remote-free never forms a contiguous run). So plain (non-atomic) typed
//! reads/writes are race-free, matching the `BinTable`/`bump`/`live_count`
//! single-writer rule.
//!
//! ## Layout in a segment
//!
//! ```text
//!   ... small_meta_end_pre_runstack (8-byte aligned)
//!   ┌──────────────────────────────────────────────────────────────┐
//!   │ RunStack                                                      │
//!   │  • RunDesc[SMALL_CLASS_COUNT][RUNSTACK_CAPACITY]              │
//!   │    each RunDesc = { start_off: u32, count: u16, _spare: u16 } │
//!   │    = 8 B; count == 0 is the empty/sentinel state              │
//!   └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! `FOOTPRINT = SMALL_CLASS_COUNT * RUNSTACK_CAPACITY * size_of::<RunDesc>()`.
//! For the default 49 classes × 8 descriptors × 8 B that is `3136` bytes (< 1
//! page once page-aligned in `small_meta_end`) — well under 1% of the 4 MiB
//! segment, an order of magnitude cheaper than the X7 gen-table (~256 KiB).
//! Compiled ONLY under `#[cfg(feature = "alloc-runfreelist")]` (the module
//! declaration in `mod.rs` is feature-gated, so this file is not even parsed
//! outside that feature); outside it the `RunStack` does not exist and the
//! segment byte layout is unchanged (the production-judge-neutrality gate —
//! plan §2.8).

use core::mem::size_of;

use super::node::Node;
use super::segment_header::Layout;
use super::size_classes::SMALL_CLASS_COUNT;

/// One compact run descriptor: the segment-relative offset of the FIRST block
/// in a contiguous freed-block run, and the number of blocks in that run.
///
/// `#[repr(C)]` for a deterministic stable layout (the same discipline every
/// other fixed-layout metadata record in this crate follows —
/// [`SegmentHeader`](super::segment_header::SegmentHeader) is `#[repr(C)]`).
/// 8 bytes: `start_off` + `count` + `_spare` pad to a naturally-aligned 8-byte
/// record, matching the 8-byte alignment the metadata region requires (plan
/// §2.1).
///
/// - `start_off: u32` — segment-relative byte offset of the first block. Real
///   offsets are `< SEGMENT` (= 4 MiB); `u32::MAX` is reserved as a sentinel
///   (like [`FREE_LIST_NULL`](super::segment_header::FREE_LIST_NULL)), so the
///   usable range is `u32` without the top value — ample headroom.
/// - `count: u16` — number of blocks in the run. `count >= 1` for a live
///   descriptor; **`count == 0` is the empty/sentinel state** (the slot holds
///   no run). `u16` covers up to 65535 blocks per run — far beyond any
///   realistic flush batch (`FLUSH_N = TCACHE_CAP/2 = 8` today).
/// - `_spare: u16` — padding to 8 bytes (record alignment). Reserved for future
///   use (e.g. a per-run generation stamp under `hardened` — plan §2.9); NOT
///   read or written in v1.
///
/// `Copy` so the descriptor can be read/written as one typed value through
/// `Node::read_struct` / `Node::write_struct` (a plain bit copy, no drop glue
/// — the same primitive `SegmentHeader` itself uses), avoiding any field-by-
/// field masking.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunDesc {
    /// Segment-relative byte offset of the first block in the run.
    pub start_off: u32,
    /// Number of contiguous blocks in the run. `0` = empty slot (sentinel).
    pub count: u16,
    /// Padding to 8 bytes; reserved for future use (plan §2.9). Unused in v1.
    pub _spare: u16,
}

impl RunDesc {
    /// The all-zero descriptor: `start_off = 0`, `count = 0`, `_spare = 0`.
    /// This is the sentinel written by [`RunStack::init_in_place`] to mark
    /// every slot empty. `const` so the init loop and the tests can name it.
    pub(crate) const ZERO: RunDesc = RunDesc {
        start_off: 0,
        count: 0,
        _spare: 0,
    };
}

/// The per-segment run-encoded freelist storage: a fixed-size 2-D array of
/// [`RunDesc`] indexed by `[class][slot]`, carved into segment metadata right
/// after the current `small_meta_end` (8-byte aligned — plan §2.2).
///
/// This struct owns NO memory — it is a thin namespace of free functions that
/// address the in-segment `RunStack` region via a `(base: *mut u8,
/// class: usize)` pair, exactly like the free-function accessors
/// `gen_at`/`bump_gen` X7-Ф1 added for the generation table and like
/// `BinTable`'s method surface (which takes `&self` only because it caches the
/// `heads` pointer; `RunStack` derives every address from `base + off` at the
/// call site, which composes cleanly with the segment-header `Layout`). The
/// unit struct exists for the "one file, one export" convention (CLAUDE.md);
/// the `impl` block holds the accessors.
pub struct RunStack;

/// Number of run descriptors kept per size-class per segment (plan §2.2).
///
/// `8` is the fixed decision: realistic flush batch = `FLUSH_N = 8` blocks
/// (`TCACHE_CAP=16 / 2`); a typical cold-storm yields one long contiguous run
/// per flush (covering the whole batch) → 1 descriptor. Adversarial
/// random-order freeing can fragment a batch into a chain of runs-of-1; with
/// `RUNSTACK_CAPACITY = 8` and 49 classes we cover up to 8 simultaneous runs
/// per class — conservative with huge headroom (realistically 1–2
/// simultaneously-live runs per class per segment). 16 is unjustified (double
/// the metadata for an unrealistic scenario); 4 is risky (easily overflows on
/// interleaved multi-batch flush). Overflow is NOT a panic: the documented
/// degradation is fallback to the classic linked freelist (plan §2.6) —
/// [`RunStack::push`] returns `false`.
pub const RUNSTACK_CAPACITY: usize = 8;

/// The byte footprint of the `RunStack` in a segment: one row of
/// `RUNSTACK_CAPACITY` descriptors for every small class. Computed from the
/// constants (not a hardcoded literal) so it cannot drift if
/// `SMALL_CLASS_COUNT` / `RUNSTACK_CAPACITY` / `RunDesc` change. For the
/// default geometry this is `49 * 8 * 8 = 3136` bytes (plan §2.2).
pub const FOOTPRINT: usize = SMALL_CLASS_COUNT * RUNSTACK_CAPACITY * size_of::<RunDesc>();

// Compile-time sanity: the footprint is exact (no rounding), the descriptor is
// the 8 bytes the plan fixes (plan §2.1), and the footprint is small (< 1 page
// — `small_meta_end` will page-align it, adding ≤ PAGE-1 bytes of padding).
const _: () = assert!(size_of::<RunDesc>() == 8, "RunDesc must be exactly 8 bytes");
const _: () = assert!(
    FOOTPRINT == SMALL_CLASS_COUNT * RUNSTACK_CAPACITY * 8,
    "FOOTPRINT must be SMALL_CLASS_COUNT * RUNSTACK_CAPACITY * 8"
);

impl RunStack {
    /// The segment-relative byte offset of the `RunStack` region. Equals
    /// [`Layout::run_stack_off`] (the offset past the prior metadata, 8-byte
    /// aligned). Re-exposed here as an associated constant of the export so
    /// callers/tests can name it without reaching into `segment_header`'s
    /// cfg-gated `Layout` accessor (which is `pub(crate)` — test-unreachable).
    #[doc(hidden)]
    pub const OFF: usize = Layout::run_stack_off();
    /// The segment-relative byte offset where payload carving begins (page-
    /// aligned past the last metadata region, including this `RunStack`).
    /// Equals [`Layout::small_meta_end`]. Re-exposed here for the same test-
    /// reachability reason as [`OFF`]: `Layout` is `pub(crate)`, but the
    /// layout-stacking contract (plan §2.2 — `OFF + FOOTPRINT <= SMALL_META_END`)
    /// is part of this phase's deliverable and must be pin-able from an
    /// isolated test.
    #[doc(hidden)]
    pub const SMALL_META_END: usize = Layout::small_meta_end();

    /// Initialise a fresh `RunStack` at `base`: zero every descriptor (all
    /// fields 0; `count == 0` is the empty/sentinel state, plan §2.1/§3-Ф1).
    /// Mirrors [`AllocBitmap::init_in_place`](super::alloc_bitmap::AllocBitmap::init_in_place)'s
    /// zeroing discipline, writing the all-zero [`RunDesc::ZERO`] record
    /// through the `node` seam.
    ///
    /// `base` MUST be a live small/primordial segment base whose `RunStack`
    /// region (`FOOTPRINT` bytes at `Self::OFF`) is carved and about to be
    /// consulted.
    #[doc(hidden)]
    pub fn init_in_place(base: *mut u8) {
        let off = Self::OFF;
        let mut class = 0usize;
        while class < SMALL_CLASS_COUNT {
            let mut slot = 0usize;
            while slot < RUNSTACK_CAPACITY {
                let desc_off = off + (class * RUNSTACK_CAPACITY + slot) * size_of::<RunDesc>();
                let dst = Node::offset(base, desc_off) as *mut RunDesc;
                Node::write_struct(dst, RunDesc::ZERO);
                slot += 1;
            }
            class += 1;
        }
    }

    /// Push a run descriptor for `class`: find an empty slot (`count == 0`)
    /// among the `RUNSTACK_CAPACITY` slots for that class, write
    /// `(start_off, count)`, return `true`. If all slots for the class are
    /// occupied, return `false` — the documented overflow signal (plan §2.6:
    /// the caller falls back to the classic linked freelist, NO panic).
    ///
    /// `_spare` is written as 0 (unused in v1 — plan §2.9).
    ///
    /// `class` MUST be `< SMALL_CLASS_COUNT` (debug-asserted; a corrupt class
    /// index would index out of the `RunStack` region).
    #[doc(hidden)]
    pub fn push(base: *mut u8, class: usize, start_off: u32, count: u16) -> bool {
        // R2-3: release-surviving class bound (replaces debug_assert!).
        assert!(class < SMALL_CLASS_COUNT, "class index out of range");
        let off = Self::OFF;
        let mut slot = 0usize;
        while slot < RUNSTACK_CAPACITY {
            let desc_off = off + (class * RUNSTACK_CAPACITY + slot) * size_of::<RunDesc>();
            let slot_ptr = Node::offset(base, desc_off) as *const RunDesc;
            let existing = Node::read_struct::<RunDesc>(slot_ptr);
            if existing.count == 0 {
                // Empty slot — claim it.
                Node::write_struct(
                    Node::offset(base, desc_off) as *mut RunDesc,
                    RunDesc {
                        start_off,
                        count,
                        _spare: 0,
                    },
                );
                return true;
            }
            slot += 1;
        }
        false
    }

    /// Pop one non-empty descriptor for `class` and clear its slot.
    ///
    /// **Lowest-occupied-slot-first**, not a true LIFO/FIFO discipline —
    /// `push` fills the lowest EMPTY slot; `pop` returns the lowest
    /// NON-EMPTY slot. For a simple push-then-pop sequence with no
    /// interleaving this behaves FIFO (the first descriptor pushed, at slot
    /// 0, is the first popped); once slots are freed and refilled out of
    /// order the ordering is whatever "lowest index" happens to be at that
    /// moment — deliberately unspecified beyond that, since Ф2/Ф3 do not
    /// depend on any particular drain order (every reconstructed block still
    /// goes through the bitmap `is_free`/`mark_alloc` check regardless of
    /// which descriptor is drained first — see the design plan §2.3).
    /// Chosen for simplicity: scan from slot 0, clear the first non-empty
    /// hit, no cursor bookkeeping. Returns `None` if no non-empty descriptor
    /// exists for `class`.
    ///
    /// Clearing writes the all-zero [`RunDesc::ZERO`] record (both words), so
    /// the slot is reusable by a future [`push`].
    ///
    /// [`push`]: Self::push
    #[doc(hidden)]
    pub fn pop(base: *mut u8, class: usize) -> Option<RunDesc> {
        // R2-3: release-surviving class bound (replaces debug_assert!).
        assert!(class < SMALL_CLASS_COUNT, "class index out of range");
        let off = Self::OFF;
        let mut slot = 0usize;
        while slot < RUNSTACK_CAPACITY {
            let desc_off = off + (class * RUNSTACK_CAPACITY + slot) * size_of::<RunDesc>();
            let slot_ptr = Node::offset(base, desc_off) as *const RunDesc;
            let desc = Node::read_struct::<RunDesc>(slot_ptr);
            if desc.count != 0 {
                // Clear the slot so it is reusable.
                Node::write_struct(slot_ptr as *mut RunDesc, RunDesc::ZERO);
                return Some(desc);
            }
            slot += 1;
        }
        None
    }

    /// Peek at one non-empty descriptor for `class` without clearing it (for
    /// tests / debugging). Same scan order as [`pop`] (first non-empty slot
    /// from slot 0); returns `None` if no non-empty descriptor exists.
    ///
    /// [`pop`]: Self::pop
    #[doc(hidden)]
    pub fn peek(base: *mut u8, class: usize) -> Option<RunDesc> {
        // R2-3: release-surviving class bound (replaces debug_assert!).
        assert!(class < SMALL_CLASS_COUNT, "class index out of range");
        let off = Self::OFF;
        let mut slot = 0usize;
        while slot < RUNSTACK_CAPACITY {
            let desc_off = off + (class * RUNSTACK_CAPACITY + slot) * size_of::<RunDesc>();
            let slot_ptr = Node::offset(base, desc_off) as *const RunDesc;
            let desc = Node::read_struct::<RunDesc>(slot_ptr);
            if desc.count != 0 {
                return Some(desc);
            }
            slot += 1;
        }
        None
    }

    /// Whether `class` has zero non-empty descriptors.
    #[doc(hidden)]
    pub fn is_empty(base: *mut u8, class: usize) -> bool {
        // R2-3: release-surviving class bound (replaces debug_assert!).
        assert!(class < SMALL_CLASS_COUNT, "class index out of range");
        let off = Self::OFF;
        let mut slot = 0usize;
        while slot < RUNSTACK_CAPACITY {
            let desc_off = off + (class * RUNSTACK_CAPACITY + slot) * size_of::<RunDesc>();
            let slot_ptr = Node::offset(base, desc_off) as *const RunDesc;
            if Node::read_struct::<RunDesc>(slot_ptr).count != 0 {
                return false;
            }
            slot += 1;
        }
        true
    }

    /// Zero every descriptor for EVERY class in this segment (one
    /// memset-equivalent). Needed later by `decommit_empty_segment` (Ф4 — plan
    /// §2.5: a decommit MUST clear the `RunStack` so stale descriptors cannot
    /// point into the re-carved payload after recommit). Implemented now since
    /// it is trivial; nothing calls it outside tests in this phase.
    #[doc(hidden)]
    pub fn clear_all(base: *mut u8) {
        // Identical to init_in_place (both zero every descriptor); kept as a
        // separate name so the Ф4 call site reads as "clear" semantically, and
        // so a future specialisation (e.g. only clear classes that were
        // touched) can diverge without touching the bootstrap's init path.
        Self::init_in_place(base);
    }
}
