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
//! class_nonempty: [[u64; WORDS_PER_CLASS]; SMALL_CLASS_COUNT]
//!
//! WORDS_PER_CLASS = MAX_SEGMENTS / 64 = 16
//! SMALL_CLASS_COUNT = 49 (default) or 55 (medium-classes)
//!
//! Total: 49 * 16 * 8 = 6,272 B = 6.1 KiB  (default)
//!        55 * 16 * 8 = 7,040 B = 6.9 KiB  (medium-classes)
//! ```
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

/// Number of `u64` words per class in the `class_nonempty` bitmap.
/// `MAX_SEGMENTS = 1024`, so 1024 / 64 = 16 words cover the full slot space.
pub(crate) const WORDS_PER_CLASS: usize = MAX_SEGMENTS / 64;

// Compile-time check: MAX_SEGMENTS must be a multiple of 64 for the bitmap
// to cover every slot exactly (no partial word at the tail).
const _: () = assert!(
    MAX_SEGMENTS.is_multiple_of(64),
    "MAX_SEGMENTS must be a multiple of 64 for the directory bitmap"
);

/// Per-class segment directory — the owner-only `class_nonempty` bitmap.
///
/// One file, one export (CLAUDE.md). See the module doc for the full design.
///
/// The struct is `repr(C)` so its layout is deterministic for the
/// `aligned_vmem::reserve_aligned` in-place-init pattern (OS-zeroed pages
/// are a fully valid initial state: every bit zero = "no class in any
/// segment is nonempty" = the pre-rebuild state).
#[repr(C)]
pub(crate) struct SegmentDirectory {
    /// `class_nonempty[c][w]`: bit `j` is set iff segment-table slot
    /// `w * 64 + j` is a live Small/Primordial segment with
    /// `BinTable::head(c) != FREE_LIST_NULL`.
    ///
    /// Plain `u64` (not `AtomicU64`): owner-only, single-writer — see the
    /// module doc.
    pub(crate) class_nonempty: [[u64; WORDS_PER_CLASS]; SMALL_CLASS_COUNT],
}

impl SegmentDirectory {
    /// Set the bit for slot `slot_idx` in class `class_idx`.
    #[inline]
    pub(crate) fn set_bit(&mut self, class_idx: usize, slot_idx: usize) {
        debug_assert!(class_idx < SMALL_CLASS_COUNT);
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let word = slot_idx / 64;
        let bit = slot_idx % 64;
        self.class_nonempty[class_idx][word] |= 1u64 << bit;
    }

    /// Clear the bit for slot `slot_idx` in class `class_idx`.
    #[inline]
    #[allow(dead_code)] // A2 scope — wired when transitions are centralised.
    pub(crate) fn clear_bit(&mut self, class_idx: usize, slot_idx: usize) {
        debug_assert!(class_idx < SMALL_CLASS_COUNT);
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let word = slot_idx / 64;
        let bit = slot_idx % 64;
        self.class_nonempty[class_idx][word] &= !(1u64 << bit);
    }

    /// Read the bit for slot `slot_idx` in class `class_idx`.
    #[inline]
    pub(crate) fn get_bit(&self, class_idx: usize, slot_idx: usize) -> bool {
        debug_assert!(class_idx < SMALL_CLASS_COUNT);
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let word = slot_idx / 64;
        let bit = slot_idx % 64;
        (self.class_nonempty[class_idx][word] >> bit) & 1 != 0
    }

    /// Clear ALL classes for a given slot (used on segment recycle — A2
    /// scope, but the primitive belongs here alongside the other bit ops).
    #[inline]
    #[allow(dead_code)] // A2 scope — wired when transitions are centralised.
    pub(crate) fn clear_slot(&mut self, slot_idx: usize) {
        debug_assert!(slot_idx < MAX_SEGMENTS);
        let word = slot_idx / 64;
        let mask = !(1u64 << (slot_idx % 64));
        for c in 0..SMALL_CLASS_COUNT {
            self.class_nonempty[c][word] &= mask;
        }
    }

    /// One-time full rebuild: walk every registered segment, read each
    /// class's `BinTable` head, set the exact `class_nonempty` bits.
    ///
    /// Called once on first materialisation. Skips null (recycled) slots and
    /// non-Small/Primordial (Large) segments.
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
            for c in 0..SMALL_CLASS_COUNT {
                if bt.head(c) != FREE_LIST_NULL {
                    self.set_bit(c, i);
                }
            }
        }
    }
}
