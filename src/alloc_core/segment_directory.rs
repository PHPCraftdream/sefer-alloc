//! [`SegmentDirectory`] — per-class `class_nonempty` bitmap sidecar for
//! O(1) directory-driven segment lookup (task R7-A1).
//!
//! ## Design
//!
//! A flat 2-D bitmap: `class_nonempty[SMALL_CLASS_COUNT][WORDS_PER_CLASS]`
//! where each `u64` word covers 64 segment-table slot indices. Bit `j` of
//! word `w` in class `c` is set iff `SegmentTable::base_at(w * 64 + j)` is a
//! live Small/Primordial segment whose `BinTable::head(c) != FREE_LIST_NULL`.
//!
//! ## Owner-only
//!
//! The directory is written ONLY by the owning thread's alloc/dealloc path
//! (the same single-writer discipline `AllocCore` itself enforces). All
//! fields are plain `u64`, not `AtomicU64` — no cross-thread reader ever
//! touches this bitmap (the A4 dirty-routing mechanism is a SEPARATE
//! structure; this bitmap is the owner's private index into its own
//! `SegmentTable`). This is P-rule-correct and eliminates any atomic-RMW
//! overhead from what will become the inner loop of the A3 directory lookup.
//!
//! ## Layout
//!
//! ```text
//! class_nonempty_by_node: [[[u64; WORDS_PER_CLASS]; SMALL_CLASS_COUNT]; NODE_BITMAPS]
//!
//! WORDS_PER_CLASS = MAX_SEGMENTS / 64 = 16
//! SMALL_CLASS_COUNT = 49 (default) or 55 (medium-classes)
//! NODE_BITMAPS = 1 (non-NUMA) or MAX_NODES + 1 (numa-aware)
//!
//! Total (non-NUMA): 1 * 49 * 16 * 8 = 6,272 B = 6.1 KiB  (default)
//!                    1 * 55 * 16 * 8 = 7,040 B = 6.9 KiB  (medium-classes)
//! Total (numa-aware, MAX_NODES=8):
//!                    9 * 49 * 16 * 8 = 56,448 B = 55.1 KiB (default)
//! ```
//!
//! ### R11-6 NUMA node-indexed variant
//!
//! Under `numa-aware`, the outer `[NODE_BITMAPS]` dimension indexes per-node
//! bitmaps. Bucket `[0, MAX_NODES)` holds segments whose `node_id` matches
//! that node. Bucket `[MAX_NODES]` (the "unknown" bucket) holds segments with
//! `node_id == NO_NODE_RAW` or `node_id >= MAX_NODES` (defensive clamp for
//! hosts with more nodes than `MAX_NODES`). Under non-`numa-aware`,
//! `NODE_BITMAPS == 1` and the structure is byte-for-byte the pre-R11-6 flat
//! 2-D bitmap (`[0]` is the only bucket) — no memory tax on non-NUMA builds.
//! See `docs/perf/R10_6_NUMA_DIRECTORY_JUDGE.md` §3.2 (Approach A) for the
//! design. The node-indexed variant was wired in task R11-6 to close the
//! ~140× scan-cliff that existed because the directory lookup was compiled out
//! entirely under `numa-aware` (every free-list miss fell back to an O(S)
//! linear scan with two-pass NUMA preference).
//!
//! ## Lazy materialisation
//!
//! NOT placed inline in every `AllocCore` / `HeapSlot`. Instead, a plain
//! `*mut SegmentDirectory` in `AllocCore` starts null and is populated via
//! the same M5-clean direct-VM reservation pattern R6 established in
//! `registry::bootstrap` / `registry::heap_overflow`
//! (`aligned_vmem::reserve_aligned` + `mem::forget`). The directory is
//! owner-only (single-writer, single-reader — the owning thread), so no
//! `AtomicPtr` or CAS protocol is needed (unlike the `HeapOverflow` sidecar,
//! which is cross-thread and needs CAS-publish). The VM reservation and raw
//! pointer dereference live in the existing `alloc_core::os`
//! `#![allow(unsafe_code)]` seam (`reserve_directory_sidecar` /
//! `deref_directory_sidecar[_mut]`).
//!
//! The sidecar is materialised ONLY after `table.count() >=
//! DIRECTORY_MATERIALIZE_THRESHOLD` (= 32, chosen from A0 data — see
//! `docs/perf/R7_DIRECTORY_BASELINE.md` §3). Below the threshold the
//! current linear scan is used unchanged — this is A1 scope only (storage +
//! lazy materialisation + rebuild); the directory is NOT queried for lookups
//! yet (that is A3).
//!
//! Sidecar OOM is NOT allocator OOM: on reserve failure, the pointer stays
//! null and the mechanism is simply off (falls back to the linear scan).
//! Never abort.
//!
//! Pointer stable until heap death; `mem::forget`-leaked for the process
//! lifetime (same discipline as `RegistryChunk` / `HeapOverflowSidecar`).

use super::segment_header::{SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL};
use super::segment_table::{SegmentTable, MAX_SEGMENTS};
use super::size_classes::SMALL_CLASS_COUNT;

/// Threshold (in registered segment-table slots) at which the directory
/// sidecar is materialised. Below this count, the linear scan is cheap
/// enough (~442 ns worst-case at S=32 per A0 baseline data) that the
/// directory's fixed overhead is not justified.
///
/// Chosen from `docs/perf/R7_DIRECTORY_BASELINE.md` §3: at S=32 the scan
/// costs ~442 ns mean and the p99 touches 1 us — a clear win for a ~100 ns
/// directory lookup. The A6 GO gate (`S <= 16 not worse than 2%`) is
/// satisfied by definition: S < 32 keeps the current linear scan unchanged.
pub(crate) const DIRECTORY_MATERIALIZE_THRESHOLD: u32 = 32;

/// R8-2 (task #215) / R9-8 (task #230): once the directory is materialised, a
/// genuine directory MISS (no candidate validated) for a GIVEN class is trusted
/// authoritative for this many consecutive misses OF THAT CLASS before a full
/// linear-scan re-validation pass runs. The streak is tracked PER-CLASS
/// (`AllocCore::directory_miss_streak: [u8; SMALL_CLASS_COUNT]`), so a
/// drift-affected class trips its OWN rescan promptly regardless of how often
/// other (healthy) classes miss — directly bounding the worst case of a
/// directory-invariant violation to `DIRECTORY_MISS_FULL_SCAN_PERIOD` wasted
/// segments for the drifted class (not diluted by cross-class traffic, as the
/// pre-R9-8 single shared counter was).
///
/// This bounds the cost of any undiscovered incremental-sync drift (task #214's
/// test suite establishes the directory tracks true state correctly in every
/// tested scenario, but a periodic safety net catches anything that isn't) —
/// see `AllocCore::find_segment_with_free_impl`'s directory-miss handling, and
/// the R9-8 rescue scan that runs before OOM as a second backstop.
///
/// ## Why 64 (was 256 pre-R9-8)
///
/// Pre-R9-8 the streak was a SINGLE `u32` shared across every size class, so
/// 256 was the TOTAL across all classes — a drifted class that was only a
/// fraction of miss traffic could wait far longer than 256 of ITS OWN misses
/// before the shared counter tripped a rescan. R9-8 makes the counter
/// per-class, so this constant is now the PER-CLASS threshold. A shorter value
/// than 256 is defensible because a single class's own miss traffic is a much
/// smaller fraction of total allocator activity than all classes combined: 64
/// per-class achieves comparable wall-clock detection latency to the old global
/// 256 under realistic multi-class load (a drifted class contributing ~1/4 of
/// misses trips at its own 64th miss ≈ the same wall-clock point the global
/// 256 would have), while STRICTLY improving detection for a low-activity
/// drifted class (which the shared counter could starve indefinitely under busy
/// healthy-class traffic). Worst case caps at 64 wasted 4 MiB segments = 256 MiB
/// of address space (4× tighter than the pre-R9-8 1 GiB), before the R9-8
/// rescue scan backstops the OOM path on top. For a SINGLE active class, 64
/// trips 4× sooner than the old 256 — strictly better detection latency (the
/// R9-8 requirement "equivalent-or-better when one class is active"), at the
/// cost of ~1/64 ≈ 1.5% of that class's misses running a re-validation scan
/// that (for a healthy directory) finds nothing — negligible.
///
/// Must fit in a `u8` (the per-class streak storage); the const-assert in
/// `AllocCore` pins this.
pub(crate) const DIRECTORY_MISS_FULL_SCAN_PERIOD: u32 = 64;

// R9-8: the per-class streak is stored as `u8` (keeps it at
// `SMALL_CLASS_COUNT` bytes total). Pin at compile time that the period never
// exceeds `u8::MAX` so a future bump cannot silently wrap the per-class counter.
const _: () = assert!(
    DIRECTORY_MISS_FULL_SCAN_PERIOD <= u8::MAX as u32,
    "DIRECTORY_MISS_FULL_SCAN_PERIOD must fit in the u8 per-class streak storage"
);

/// Number of `u64` words per class in the `class_nonempty` bitmap.
/// `MAX_SEGMENTS = 1024`, so 1024 / 64 = 16 words cover the full slot space.
pub(crate) const WORDS_PER_CLASS: usize = MAX_SEGMENTS / 64;

// Compile-time check: MAX_SEGMENTS must be a multiple of 64 for the bitmap
// to cover every slot exactly (no partial word at the tail).
const _: () = assert!(
    MAX_SEGMENTS.is_multiple_of(64),
    "MAX_SEGMENTS must be a multiple of 64 for the directory bitmap"
);

// ── R11-6: NUMA node-indexed directory dimensions ──────────────────────────
//
// Under `numa-aware`, the directory gains an outer `[NODE_BITMAPS]` dimension
// indexing per-node bitmaps (R10-6 §3.2 Approach A). `MAX_NODES` is the number
// of distinct real-node buckets; bucket index `MAX_NODES` is the "unknown"
// bucket for `NO_NODE_RAW` / out-of-range node ids. Under non-`numa-aware`,
// `NODE_BITMAPS == 1` (single bucket, byte-for-byte the pre-R11-6 layout).

/// Maximum number of distinct NUMA node buckets in the directory. R10-6 §3.2
/// recommends 8 (covers current x86 server topologies; `numa-shim` itself
/// caps its sysfs scan at 64 nodes — we do not need to match that ceiling).
/// Segments with `node_id >= MAX_NODES` are defensively clamped into the
/// unknown bucket rather than rejected.
#[cfg(feature = "numa-aware")]
pub(crate) const MAX_NODES: usize = 8;

/// Number of per-node bitmaps in the directory: `MAX_NODES` real-node buckets
/// plus one "unknown node" bucket (for `NO_NODE_RAW` / out-of-range ids). Under
/// non-`numa-aware`, degenerates to 1 (the single pre-R11-6 bucket) so non-NUMA
/// memory is byte-for-byte unaffected.
#[cfg(not(feature = "numa-aware"))]
pub(crate) const NODE_BITMAPS: usize = 1;

#[cfg(feature = "numa-aware")]
pub(crate) const NODE_BITMAPS: usize = MAX_NODES + 1;

/// Map a segment `node_id` (as returned by `SegmentMeta::node_id_of`) to its
/// directory bucket index. Real node ids `[0, MAX_NODES)` map directly;
/// `NO_NODE_RAW` (`u32::MAX`) and any id `>= MAX_NODES` map to the unknown
/// bucket (`MAX_NODES`). Under non-`numa-aware`, always returns 0 (the single
/// bucket) regardless of `node_id`.
#[inline]
pub(crate) fn node_bucket(node_id: u32) -> usize {
    #[cfg(not(feature = "numa-aware"))]
    {
        let _ = node_id;
        0
    }
    #[cfg(feature = "numa-aware")]
    {
        if (node_id as usize) >= MAX_NODES {
            MAX_NODES
        } else {
            node_id as usize
        }
    }
}

/// Per-class segment directory — the owner-only `class_nonempty` bitmap.
///
/// One file, one export (CLAUDE.md). See the module doc for the full design.
///
/// The struct is `repr(C)` so its layout is deterministic for the
/// `aligned_vmem::reserve_aligned` in-place-init pattern (OS-zeroed pages
/// are a fully valid initial state: every bit zero = "no class in any
/// segment is nonempty" = the pre-rebuild state).
///
/// R11-6: under `numa-aware`, `class_nonempty_by_node` gains an outer
/// `[NODE_BITMAPS]` dimension indexing per-node bitmaps. Under
/// non-`numa-aware`, `NODE_BITMAPS == 1` so the field is byte-for-byte the
/// pre-R11-6 flat 2-D bitmap.
#[repr(C)]
pub(crate) struct SegmentDirectory {
    /// `class_nonempty_by_node[node_bucket][class_idx][word]`: bit `j` is set
    /// iff segment-table slot `word * 64 + j` is a live Small/Primordial
    /// segment with `BinTable::head(class_idx) != FREE_LIST_NULL` and
    /// `node_id` mapping to `node_bucket`.
    ///
    /// Plain `u64` (not `AtomicU64`): owner-only, single-writer — see the
    /// module doc.
    pub(crate) class_nonempty_by_node: [[[u64; WORDS_PER_CLASS]; SMALL_CLASS_COUNT]; NODE_BITMAPS],
}

impl SegmentDirectory {
    /// Set the bit for slot `slot_idx` in class `class_idx` in the bitmap for
    /// the node derived from `node_id`. R11-6: `node_id` selects the per-node
    /// bucket under `numa-aware`; under non-`numa-aware` it is ignored (single
    /// bucket).
    #[inline]
    pub(crate) fn set_bit(&mut self, node_id: u32, class_idx: usize, slot_idx: usize) {
        debug_assert!(class_idx < SMALL_CLASS_COUNT);
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let nb = node_bucket(node_id);
        let word = slot_idx / 64;
        let bit = slot_idx % 64;
        self.class_nonempty_by_node[nb][class_idx][word] |= 1u64 << bit;
    }

    /// Clear the bit for slot `slot_idx` in class `class_idx` in the bitmap for
    /// the node derived from `node_id`.
    #[inline]
    pub(crate) fn clear_bit(&mut self, node_id: u32, class_idx: usize, slot_idx: usize) {
        debug_assert!(class_idx < SMALL_CLASS_COUNT);
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let nb = node_bucket(node_id);
        let word = slot_idx / 64;
        let bit = slot_idx % 64;
        self.class_nonempty_by_node[nb][class_idx][word] &= !(1u64 << bit);
    }

    /// Read the bit for slot `slot_idx` in class `class_idx` in the bitmap for
    /// the node derived from `node_id`. Only used by
    /// `AllocCore::dbg_directory_get_bit_for_node`, itself `numa-aware`-only
    /// (per-node reads are meaningless in the single-bucket non-NUMA
    /// layout — `dbg_directory_get_bit`/`dbg_directory_get_bit_bucket` cover
    /// that case instead).
    #[cfg(all(feature = "alloc-segment-directory", feature = "numa-aware"))]
    #[inline]
    pub(crate) fn get_bit(&self, node_id: u32, class_idx: usize, slot_idx: usize) -> bool {
        debug_assert!(class_idx < SMALL_CLASS_COUNT);
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let nb = node_bucket(node_id);
        let word = slot_idx / 64;
        let bit = slot_idx % 64;
        (self.class_nonempty_by_node[nb][class_idx][word] >> bit) & 1 != 0
    }

    /// R11-6: clear the bit for `(class_idx, slot_idx)` across ALL node
    /// buckets. Used when the caller cannot determine the segment's node
    /// (e.g. clearing a stale bit whose base is null — the segment was
    /// recycled). Under non-`numa-aware` (`NODE_BITMAPS == 1`) this is
    /// identical to `clear_bit`.
    #[inline]
    pub(crate) fn clear_bit_all_nodes(&mut self, class_idx: usize, slot_idx: usize) {
        debug_assert!(class_idx < SMALL_CLASS_COUNT);
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let word = slot_idx / 64;
        let mask = !(1u64 << (slot_idx % 64));
        for nb in 0..NODE_BITMAPS {
            self.class_nonempty_by_node[nb][class_idx][word] &= mask;
        }
    }

    /// Clear ALL classes for a given slot across ALL node buckets (used on
    /// segment recycle — A2 scope, but the primitive belongs here alongside
    /// the other bit ops). R11-6: iterates all node buckets so a reused slot
    /// does not inherit stale bits from any node's bitmap.
    #[inline]
    pub(crate) fn clear_slot(&mut self, slot_idx: usize) {
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let word = slot_idx / 64;
        let mask = !(1u64 << (slot_idx % 64));
        for nb in 0..NODE_BITMAPS {
            for c in 0..SMALL_CLASS_COUNT {
                self.class_nonempty_by_node[nb][c][word] &= mask;
            }
        }
    }

    /// One-time full rebuild: walk every registered segment, read each
    /// class's `BinTable` head, set the exact `class_nonempty` bits.
    ///
    /// Called once on first materialisation. Skips null (recycled) slots and
    /// non-Small/Primordial (Large) segments.
    ///
    /// R11-6: under `numa-aware`, reads each segment's `node_id_of()` and
    /// places bits in the correct per-node bucket. Under non-`numa-aware`,
    /// all bits go in bucket 0.
    ///
    /// The `table` reference is the owning `AllocCore`'s `SegmentTable` — the
    /// rebuild reads from it, never writes it. The directory (`&mut self`) is
    /// freshly OS-zeroed (all bits already 0), so only non-empty heads need
    /// to be SET — no pre-clear needed.
    pub(crate) fn rebuild_from_table(&mut self, table: &SegmentTable) {
        let n = table.count() as usize;
        for i in 0..n {
            let base = table.base_at(i);
            if base.is_null() {
                continue;
            }
            if !matches!(
                SegmentHeader::kind_at(base),
                SegmentKind::Small | SegmentKind::Primordial
            ) {
                continue;
            }
            let meta = SegmentMeta::new(base);
            let bt = meta.bin_table();
            // R11-6: derive the node bucket from the segment header under
            // numa-aware; non-numa-aware uses bucket 0 (node_id_of is
            // cfg-gated out).
            #[cfg(feature = "numa-aware")]
            let node_id = meta.node_id_of();
            #[cfg(not(feature = "numa-aware"))]
            let node_id = 0u32;
            for c in 0..SMALL_CLASS_COUNT {
                if bt.head(c) != FREE_LIST_NULL {
                    self.set_bit(node_id, c, i);
                }
            }
        }
    }
}
