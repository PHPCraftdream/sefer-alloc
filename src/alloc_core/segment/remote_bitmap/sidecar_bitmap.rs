//! Single-bank pending bits and the owner's "start was issued" map, outside
//! the reservation.
//!
//! The future directory supplies stable, disjoint atomic backing for the
//! entire reservation lifetime. It prepares that backing on an allocation
//! path, before the segment can be published. No method allocates. A masked
//! user pointer is only an integer lookup key; this primitive never receives
//! or dereferences one. The directory must pin the backing through all
//! unpublished producers, published bits, and detached cuts.

#[cfg(any(test, loom))]
use super::{AtomicU64, BitmapScan, Ordering};
#[cfg(not(any(test, loom)))]
use super::{AtomicU64, Ordering};

#[path = "sidecar_bitmap/chunked_bank.rs"]
mod chunked_bank;
pub(crate) use chunked_bank::ChunkedBank;

#[cfg(any(test, loom))]
use crate::alloc_core::fatal::fatal;
#[cfg(any(test, loom))]
use crate::alloc_core::os::SEGMENT;
#[cfg(any(test, loom))]
use crate::alloc_core::segment_layout::SIDECAR_WORD_BYTES;
#[cfg(any(test, loom))]
use crate::alloc_core::size_classes::MIN_BLOCK;

#[cfg(any(test, loom))]
const GRANULES: usize = SEGMENT / MIN_BLOCK;
#[cfg(any(test, loom))]
const WORDS: usize = GRANULES / 64;

/// A live segment's external ingress storage. Construction borrows backing;
/// it does not allocate, initialize, or bind a reservation pointer.
#[cfg(any(test, loom))]
#[allow(dead_code)] // Плотный reference-oracle старых bitmap-тестов, не production.
pub(crate) struct SidecarBitmap<'a> {
    pub(super) pending: &'a [AtomicU64],
    /// D1′ phase-1a: the dense "start was issued" map, one bit per 16 B
    /// granule (32 KiB, `[AtomicU64; WORDS]`), written ONLY by the owner
    /// (single-writer `load(Acquire)` → `store(old | mask, Release)`) at
    /// exactly the per-block issue points (`SmallSidecar::issue`, reached
    /// from both the drain and the carve-batch paths through
    /// `SmallIssueRun::issue`), cleared by retain-reset together with
    /// `pending`. Producers read it (Acquire) as the publication gate below;
    /// the record's class lives only in the owner-private state byte.
    pub(super) starts: &'a [AtomicU64],
}

#[cfg(any(test, loom))]
#[allow(dead_code)] // Только плотный reference-oracle.
impl<'a> SidecarBitmap<'a> {
    pub(crate) const WORDS: usize = WORDS;
    pub(crate) const GRANULES: usize = GRANULES;

    /// Backing must be separate from reservation bytes and zero-initialized
    /// before segment publication. Its lifetime must extend through all cuts.
    // R6-T16: loom-shim / source-include constructor (`--cfg loom`, `cfg(test)`);
    // production bitmaps start zeroed via the primordial path.
    #[cfg_attr(not(loom), allow(dead_code))]
    pub(crate) fn from_pending(pending: &'a [AtomicU64], starts: &'a [AtomicU64]) -> Option<Self> {
        (pending.len() == WORDS && starts.len() == WORDS).then_some(Self { pending, starts })
    }

    /// Owner-only issue: set the "start was issued" bit for `offset`'s
    /// granule. The record's class lives ONLY in the owner-private state
    /// byte; nothing else is stored here.
    ///
    /// The previous incarnation's bit must already be cut and reclaimed.
    /// A separate allocation handoff must carry this Release store to a
    /// producer; the bitmap does not itself establish that handoff.
    ///
    /// D1′ phase-1b: infallible. The class-leaf spill OOM is gone with the
    /// leaves; the only owner-side error an issue could report (bad class
    /// code) no longer exists. An out-of-segment or misaligned offset on
    /// this owner-only path is metadata corruption and aborts.
    pub(crate) fn issue(&self, offset: u32) {
        let Some(granule) = Self::granule(offset) else {
            fatal(b"sidecar_bitmap: issue offset out of segment or misaligned\n");
        };
        // Single-writer load -> or -> Release store (the owner is the only
        // `starts` writer; producers only read). Monotone: a reissue from
        // the free list stores the already-set bit back.
        let mask = 1u64 << (granule % 64);
        let old = self.starts[granule / 64].load(Ordering::Acquire);
        self.starts[granule / 64].store(old | mask, Ordering::Release);
    }

    /// Producer terminal AcqRel RMW. No header or reservation access. The
    /// D1′ phase-1a producer filter replaces the former leaf-class read:
    /// (1) payload filter by route kind — a granule below the route's
    /// metadata end (`payload_start`, in bytes, supplied by the pin from the
    /// route descriptor) is rejected before any sidecar access; (2) alignment 16 (via [`Self::granule`]); (3) an
    /// Acquire load of the owner-written "start was issued" bit — a granule
    /// that was never issued (uncarved tail, 16-aligned interior pointer,
    /// gap) reads 0 and is dropped WITHOUT any RMW. Only then the same
    /// terminal `fetch_or(AcqRel)` runs — still exactly one anchor R5b-13
    /// per publication. The owner sets start bits no earlier than the
    /// former leaf-issue points (never before the block's own commit) and
    /// clears them only on retain-reset, so a re-registered route's freshly
    /// zeroed sidecar rejects every stale publication exactly as before.
    /// This pending RMW is the linearization point and
    /// last producer access to the reclaimable reservation and ledger.
    /// Afterwards only a discovery-summary bit in the SAME sidecar may be
    /// set, using an address obtained before this RMW while RoutePin is
    /// held, then the pin released. Strict trim and periodic maintenance
    /// ignore summary; this permission does not imply a runtime summary
    /// implementation. Exactly one producer publishes each valid allocation
    /// instance. An unpublished producer retains its segment credit; a
    /// published one retains it until reclaim. The public
    /// `foreign_or_unroutable_frees` semantics are unchanged: a `false`
    /// return counts one rejected publication.
    pub(crate) fn publish(&self, offset: u32, payload_start: usize) -> bool {
        let Some(granule) = Self::granule(offset) else {
            return false;
        };
        if granule < payload_start / MIN_BLOCK {
            return false;
        }
        if self.starts[granule / 64].load(Ordering::Acquire) & (1u64 << (granule % 64)) == 0 {
            return false;
        }
        self.pending[granule / 64].fetch_or(1 << (granule % 64), Ordering::AcqRel);
        true
    }

    /// Owner fixes an exclusive upper bound in bytes. The caller's high water
    /// must include every issued block that can still publish. This scans
    /// exactly `ceil(high_water / SIDECAR_WORD_BYTES)` words, without dirty hints
    /// or waiting for paused producers. An in-flight producer after a word's
    /// cut belongs to a later scan.
    // R6-T16: reached only through the internals/oracle `SmallSidecar::scan`.
    // `SmallSidecar` lives in `registry`, which compiles only under
    // `alloc-global`; without it this scan has no in-tree caller either.
    #[cfg_attr(
        any(not(feature = "internals"), not(feature = "alloc-global")),
        allow(dead_code)
    )]
    pub(crate) fn scan(&self, high_water: usize) -> Option<BitmapScan<'a>> {
        self.scan_from(high_water, 0)
    }

    /// Owner resumes at a numeric word, never at a retained sidecar borrow.
    pub(crate) fn scan_from(&self, high_water: usize, start_word: usize) -> Option<BitmapScan<'a>> {
        if high_water > SEGMENT {
            return None;
        }
        let end_word = high_water.div_ceil(SIDECAR_WORD_BYTES);
        if start_word > end_word {
            return None;
        }
        Some(BitmapScan {
            pending: self.pending.get(start_word..)?.iter(),
            chunked: None,
            next_word: start_word,
            end_word,
        })
    }

    fn granule(offset: u32) -> Option<usize> {
        let offset = offset as usize;
        (offset < SEGMENT && offset.is_multiple_of(MIN_BLOCK)).then_some(offset / MIN_BLOCK)
    }
}
