//! B4 (task b4-provenance, chunk 1 / review round R1): heap-private shadow
//! simulation of a Bonwick-2001 depot (spare half-magazines) per size class,
//! under `bench-internals` only. Zero code, zero state bytes in any build
//! without the feature (the module itself is cfg'd out in `state/mod.rs`).
//!
//! ## What is simulated
//!
//! On every magazine overflow-flush of `FLUSH_N` blocks the hypothetical
//! depot would first try to absorb the batch into a spare half-magazine of
//! capacity `K * FLUSH_N` (K ∈ {1, 2, 4, 8}); only the non-absorbed rest
//! would be really flushed to the bin (writing `next` into block bodies).
//! On every magazine refill of `n` blocks the depot would serve
//! `min(n, occupied_K)` of them; the rest is a real bin drain. Four
//! parallel occupancy counters (`occupied[K]`, in blocks) model this
//! without storing any pointer.
//!
//! ## R1 rework: static atomics table, no cell-of raw access
//!
//! Review round R1 replaced the former `UnsafeCell`-based slot storage
//! (whose `Sync` impl was the only item of that kind in the file) with a
//! process-static table of `AtomicU32` cells — the file now carries no
//! raw-access item of that kind (the crate denies the corresponding lint
//! in these configurations). Per slot row and size class `c`, ONE `AtomicU32`
//! packs the four `occupied[K]` bytes (byte `k` = K ∈ {1,2,4,8}, value
//! 0..=64 — the largest simulated capacity is `8 * FLUSH_N = 64` blocks,
//! well inside a byte). Updates are plain `Relaxed`
//! read-modify-write/load-store: the registry leases each heap row to at
//! most one thread at a time, so each row has a sole writer (the same
//! single-writer discipline that lets `Tcache` use plain fields); readers
//! take a `Relaxed` snapshot, which needs no ordering — the per-row
//! numbers are diagnostic-only. Cross-thread aggregate views live in the
//! process-global static mirrors of `diag/counters.rs`.
//!
//! ## Scalars are NOT per-slot
//!
//! Per-slot state is ONLY the occupied table. Every scalar aggregate the
//! former inline `DepotShadow` struct carried is already a process-wide
//! mirror in `diag/counters.rs`, and — because exactly the corresponding
//! path counter feeds each hook — the mirror IS the scalar, with the
//! identities documented on `dbg_depot_shadow_aggregates`:
//! - `flush_events` / `flush_blocks_total` ≡ `DEPOT_BIN_INGRESS_OWN_OVERFLOW_*`
//!   (only the overflow half-flush feeds `depot_on_overflow_flush`);
//! - `refill_events` / `refill_blocks_total` ≡ `DEPOT_REFILL_EVENTS` /
//!   `DEPOT_REFILL_BLOCKS_TOTAL` (the shadow refill hook fires at exactly
//!   those sites);
//! - `invalidation_events` ≡ `DEPOT_SHADOW_INVALIDATION_EVENTS`;
//! - absorbed/real/from_depot/drain per K ≡ the six `DEPOT_SHADOW_*`
//!   `[AtomicU64; 4]` mirrors.
//!
//! `dbg_depot_shadow_snapshot` therefore returns only the occupied table.
//!
//! ## Why counting WITHOUT block identity is exact here (and where it is not)
//!
//! The depot holds the thread's own most-recently flushed blocks, and a
//! refill of the same class on the same heap is served from the same LIFO
//! stream. For COUNTING purposes (how many blocks a depot of capacity
//! `K * FLUSH_N` would have absorbed / served) the identity of the
//! individual block is irrelevant — only the occupancy balance matters —
//! so a pure counter simulation is exact wherever every block that leaves
//! the magazine on a flush passes through the depot, and every refill block
//! that could be depot-resident is one the depot may hold. It becomes an
//! OVER-estimate of real depot benefit wherever blocks move WITHOUT passing
//! the depot: the R1-01 swap-top eviction, `dealloc_batch`'s staged
//! overflow flush, teardown/trim (`flush_all_tcache`), segment pool/trim
//! reclaim, remote frees, and virgin carves (a carve mints a block that was
//! never flushed, so it can never be absorbed). At any such event the
//! shadow RESETS all occupancies (`DEPOT_SHADOW_INVALIDATION_EVENTS`) — the
//! conservative direction: after a reset the simulation reports LESS
//! absorption, never more, so the numbers it does report stay a lower bound
//! for the contiguous runs it survived. `INVALIDATION` reasons are
//! commented at every reset call site. Carve events are flagged with
//! `DEPOT_REFILL_BLOCKS_CARVED` / `DEPOT_REFILL_CARVE_EVENTS`
//! (`diag/counters.rs`) but do NOT reset occupancy (a carve does not touch
//! depot-resident blocks).
//!
//! ## Why the shadow does NOT live in `HeapCore`
//!
//! The R34-18 compile-time pin in `heap_core/core.rs` caps
//! `size_of::<HeapCore>()` at 9216 B stack pressure, and the `--all-features`
//! composition of this tree already measures 9200 B of it — only 16 B of
//! headroom. ANY `HeapCore` field (even four `u8`s of occupancy, 4 B) could
//! trip the pin. The shadow therefore lives OUT of `HeapCore`, in the
//! process-static table below, indexed by a mapping of `HeapCore::id`.
//! Semantics are unchanged: the row is zeroed when a heap is constructed
//! on its id (fresh heap ⇒ fresh simulation, exactly what the former inline
//! field initialiser provided), and every write goes through `&mut self`
//! methods of the exclusively leased `HeapCore`. Like the rest of the
//! module, the whole table is compiled out without `bench-internals`.
//!
//! ## Row mapping (fallback + non-standard ids)
//!
//! `HeapCore::new`/`new_with_config` are also called with
//! `OWNER_ID_FALLBACK = 0x7FFF_FFFE` (`global/fallback.rs`), which is out
//! of range for the `MAX_HEAPS`-row registry table. The mapping
//! [`depot_shadow_row`] is:
//! - `id < MAX_HEAPS` → row `id` (registry slots);
//! - `id == OWNER_ID_FALLBACK` → the DEDICATED extra row `MAX_HEAPS`
//!   (the table has `MAX_HEAPS + 1` rows);
//! - any other `id >= MAX_HEAPS` → NOT simulated: the hooks skip the
//!   shadow and bump `DEPOT_SHADOW_UNMAPPED_EVENTS`
//!   (reader: `HeapCore::dbg_depot_shadow_unmapped`). In this tree no such
//!   id can reach a `HeapCore`, so the counter is a tripwire, not a fast
//!   path.

use crate::alloc_core::segment_header::OWNER_ID_FALLBACK;
use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;
use crate::registry::bootstrap::MAX_HEAPS;
use crate::registry::heap_core::state::tcache::FLUSH_N;
use crate::registry::heap_core::HeapCore;
use core::sync::atomic::{AtomicU32, Ordering};

/// Number of K levels simulated: capacities `K * FLUSH_N` for K ∈ {1,2,4,8}.
const DEPOT_K_LEVELS: usize = 4;

/// The dedicated shadow row of the fallback heap (`OWNER_ID_FALLBACK`).
const DEPOT_SHADOW_FALLBACK_ROW: usize = MAX_HEAPS;

/// Total shadow rows: every registry slot plus one dedicated fallback row.
const DEPOT_SHADOW_ROWS: usize = MAX_HEAPS + 1;

/// Map a `HeapCore::id` to its shadow row — see the module doc. `None`
/// means "not simulated" (the caller bumps `DEPOT_SHADOW_UNMAPPED_EVENTS`).
#[inline]
fn depot_shadow_row(id: u32) -> Option<usize> {
    let id = id as usize;
    if id < MAX_HEAPS {
        Some(id)
    } else if id == OWNER_ID_FALLBACK as usize {
        Some(DEPOT_SHADOW_FALLBACK_ROW)
    } else {
        None
    }
}

/// Unpack packed-K byte `k` from one `AtomicU32` cell's value.
#[inline]
fn depot_unpack(v: u32, k: usize) -> u8 {
    (v >> (8 * k)) as u8
}

/// Re-pack packed-K byte `k` with `b` (0..=64) into a cell value.
#[inline]
fn depot_repack(v: u32, k: usize, b: u8) -> u32 {
    (v & !(0xFFu32 << (8 * k))) | ((u32::from(b)) << (8 * k))
}

/// Process-static shadow occupancy table (see the module doc for why the
/// shadow cannot be a `HeapCore` field, and for the single-writer-per-row
/// concurrency discipline that makes plain `Relaxed` access sound WITHOUT
/// any such token). Row `MAX_HEAPS` is the dedicated fallback-heap row.
#[doc(hidden)]
static DEPOT_SHADOW_OCCUPIED: [[AtomicU32; SMALL_CLASS_COUNT]; DEPOT_SHADOW_ROWS] =
    [const { [const { AtomicU32::new(0) }; SMALL_CLASS_COUNT] }; DEPOT_SHADOW_ROWS];

/// Fresh-simulation reset for the heap `id`'s row — called when a heap is
/// constructed on that id (the exact equivalent of the former inline
/// `depot: DepotShadow::new()` field initialiser). Unmapped ids are not
/// simulated (tripwire counter, see the module doc).
pub(crate) fn depot_shadow_reset_for_heap(id: u32) {
    match depot_shadow_row(id) {
        Some(row) => {
            for cell in &DEPOT_SHADOW_OCCUPIED[row] {
                cell.store(0, Ordering::Relaxed);
            }
        }
        None => {
            crate::registry::heap_core::diag::DEPOT_SHADOW_UNMAPPED_EVENTS
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl HeapCore {
    /// This leased heap's shadow row (see the module doc).
    #[inline]
    fn depot_row(&self) -> Option<usize> {
        depot_shadow_row(self.id)
    }

    /// Per-class overflow-flush hook: absorb `n` blocks of class `c`.
    ///
    /// Sole-writer discipline: this runs under the exclusive `&mut` lease of
    /// the heap row, so the `Relaxed` load → store below races with no other
    /// writer of this row. Scalars are the process-global mirrors (see the
    /// module doc) — this hook IS the `DEPOT_BIN_INGRESS_OWN_OVERFLOW` site.
    #[inline(always)]
    pub(crate) fn depot_on_overflow_flush(&mut self, c: usize, n: u64) {
        use crate::registry::heap_core::diag::{
            DEPOT_SHADOW_FLUSH_BLOCKS_ABSORBED, DEPOT_SHADOW_FLUSH_BLOCKS_REAL,
            DEPOT_SHADOW_FLUSH_EVENTS_FULLY_ABSORBED,
        };
        let Some(row) = self.depot_row() else {
            crate::registry::heap_core::diag::DEPOT_SHADOW_UNMAPPED_EVENTS
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let cell = &DEPOT_SHADOW_OCCUPIED[row][c];
        let mut packed = cell.load(Ordering::Relaxed);
        for k in 0..DEPOT_K_LEVELS {
            let cap = ((1usize << k) * FLUSH_N) as u64;
            let occ = u64::from(depot_unpack(packed, k)).min(cap);
            let free = cap - occ;
            let absorbed = n.min(free);
            // Process-wide mirror (Relaxed fetch_add) — same event, same
            // delta as the row update below.
            DEPOT_SHADOW_FLUSH_BLOCKS_ABSORBED[k].fetch_add(absorbed, Ordering::Relaxed);
            DEPOT_SHADOW_FLUSH_BLOCKS_REAL[k].fetch_add(n - absorbed, Ordering::Relaxed);
            if absorbed == n {
                DEPOT_SHADOW_FLUSH_EVENTS_FULLY_ABSORBED[k].fetch_add(1, Ordering::Relaxed);
            }
            // `occ + absorbed <= cap <= 64`, so the byte never overflows.
            packed = depot_repack(packed, k, (occ + absorbed) as u8);
        }
        cell.store(packed, Ordering::Relaxed);
    }

    /// Refill hook: `n` blocks of class `c` parked into the magazine.
    /// Mirror scalars: see the module doc; single-writer discipline as in
    /// [`Self::depot_on_overflow_flush`].
    #[inline(always)]
    pub(crate) fn depot_on_refill(&mut self, c: usize, n: u64) {
        use crate::registry::heap_core::diag::{
            DEPOT_SHADOW_REFILL_BLOCKS_DRAIN, DEPOT_SHADOW_REFILL_BLOCKS_FROM_DEPOT,
            DEPOT_SHADOW_REFILL_FULLY_FROM_DEPOT,
        };
        let Some(row) = self.depot_row() else {
            crate::registry::heap_core::diag::DEPOT_SHADOW_UNMAPPED_EVENTS
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let cell = &DEPOT_SHADOW_OCCUPIED[row][c];
        let mut packed = cell.load(Ordering::Relaxed);
        for k in 0..DEPOT_K_LEVELS {
            let occ = u64::from(depot_unpack(packed, k));
            let from_depot = n.min(occ);
            // Process-wide mirror (see `depot_on_overflow_flush`).
            DEPOT_SHADOW_REFILL_BLOCKS_FROM_DEPOT[k].fetch_add(from_depot, Ordering::Relaxed);
            DEPOT_SHADOW_REFILL_BLOCKS_DRAIN[k].fetch_add(n - from_depot, Ordering::Relaxed);
            if from_depot == n {
                DEPOT_SHADOW_REFILL_FULLY_FROM_DEPOT[k].fetch_add(1, Ordering::Relaxed);
            }
            packed = depot_repack(packed, k, (occ - from_depot) as u8);
        }
        cell.store(packed, Ordering::Relaxed);
    }

    /// Residency invalidation (see module doc for the reasons). Zeroes every
    /// class's occupancy at every K; the event is counted in the
    /// process-global `DEPOT_SHADOW_INVALIDATION_EVENTS` mirror.
    #[inline(always)]
    pub(crate) fn depot_invalidate(&mut self) {
        let Some(row) = self.depot_row() else {
            crate::registry::heap_core::diag::DEPOT_SHADOW_UNMAPPED_EVENTS
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        for cell in &DEPOT_SHADOW_OCCUPIED[row] {
            cell.store(0, Ordering::Relaxed);
        }
        crate::registry::heap_core::diag::DEPOT_SHADOW_INVALIDATION_EVENTS
            .fetch_add(1, Ordering::Relaxed);
    }

    /// MEASUREMENT-ONLY snapshot of this heap's shadow occupancy —
    /// `[[u8; 4]; SMALL_CLASS_COUNT]`, class-major, K ∈ {1,2,4,8}
    /// innermost (in blocks). `#[doc(hidden)]`, not public API. All
    /// scalar aggregates (flush/refill/absorption/invalidation) live in
    /// the process-global mirrors — read
    /// [`HeapCore::dbg_depot_shadow_aggregates`] and the
    /// `dbg_depot_bin_ingress_*` / `dbg_depot_refill_stats` hooks
    /// alongside this snapshot. Unmapped ids read as all-zero.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_depot_shadow_snapshot(&mut self) -> [[u8; DEPOT_K_LEVELS]; SMALL_CLASS_COUNT] {
        let Some(row) = self.depot_row() else {
            return [[0; DEPOT_K_LEVELS]; SMALL_CLASS_COUNT];
        };
        let mut out = [[0; DEPOT_K_LEVELS]; SMALL_CLASS_COUNT];
        for (c, cell) in DEPOT_SHADOW_OCCUPIED[row].iter().enumerate() {
            let packed = cell.load(Ordering::Relaxed);
            for (k, slot) in out[c].iter_mut().enumerate().take(DEPOT_K_LEVELS) {
                *slot = depot_unpack(packed, k);
            }
        }
        out
    }

    /// MEASUREMENT-ONLY snapshot of the DEDICATED fallback-heap shadow row
    /// (`OWNER_ID_FALLBACK` → row `MAX_HEAPS`), same shape as
    /// [`Self::dbg_depot_shadow_snapshot`]. Static: the fallback heap
    /// cannot be claimed as a lease, so its row is read directly.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_depot_shadow_fallback_row() -> [[u8; DEPOT_K_LEVELS]; SMALL_CLASS_COUNT] {
        let mut out = [[0; DEPOT_K_LEVELS]; SMALL_CLASS_COUNT];
        for (c, cell) in DEPOT_SHADOW_OCCUPIED[DEPOT_SHADOW_FALLBACK_ROW]
            .iter()
            .enumerate()
        {
            let packed = cell.load(Ordering::Relaxed);
            for (k, slot) in out[c].iter_mut().enumerate().take(DEPOT_K_LEVELS) {
                *slot = depot_unpack(packed, k);
            }
        }
        out
    }
}
