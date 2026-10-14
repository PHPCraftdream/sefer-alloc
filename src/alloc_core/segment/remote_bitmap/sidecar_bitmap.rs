//! Single-bank pending bits and owner-issued classes, outside the reservation.
//!
//! The future directory supplies stable, disjoint atomic backing for the
//! entire reservation lifetime. It prepares that backing on an allocation
//! path, before the segment can be published. No method allocates. A masked
//! user pointer is only an integer lookup key; this primitive never receives
//! or dereferences one. The directory must pin the backing through all
//! unpublished producers, published bits, and detached cuts.

use super::{AtomicU64, AtomicU8, BitmapCut, BitmapScan, Ordering};

#[path = "sidecar_bitmap/leaf_classes.rs"]
#[cfg_attr(test, allow(dead_code))]
mod leaf_classes;
#[path = "sidecar_bitmap/summary_scan.rs"]
mod summary_scan;
pub(crate) use leaf_classes::ClassLeaves;
pub(crate) use summary_scan::SummaryScan;

use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::size_classes::{MIN_BLOCK, SMALL_CLASS_COUNT};

const GRANULES: usize = SEGMENT / MIN_BLOCK;
const WORDS: usize = GRANULES / 64;
const SUMMARY_WORDS: usize = 64;

/// A live segment's external ingress storage. Construction borrows backing;
/// it does not allocate, initialize, or bind a reservation pointer.
pub(crate) struct SidecarBitmap<'a> {
    pub(super) pending: &'a [AtomicU64],
    pub(super) classes: ClassMap<'a>,
}

#[derive(Clone, Copy)]
#[cfg_attr(test, allow(dead_code))]
pub(super) enum ClassMap<'a> {
    Dense(&'a [AtomicU8]),
    Leaves(&'a ClassLeaves),
}

impl ClassMap<'_> {
    pub(super) fn encoded(self, granule: usize) -> u8 {
        match self {
            Self::Dense(classes) => classes[granule].load(Ordering::Acquire),
            Self::Leaves(leaves) => leaves.encoded(granule),
        }
    }
}

impl<'a> SidecarBitmap<'a> {
    pub(crate) const WORDS: usize = WORDS;
    pub(crate) const GRANULES: usize = GRANULES;
    pub(crate) const SUMMARY_WORDS: usize = SUMMARY_WORDS;

    /// Backing must be separate from reservation bytes and zero-initialized
    /// before segment publication. Its lifetime must extend through all cuts.
    pub(crate) fn from_initialized(
        pending: &'a [AtomicU64],
        classes: &'a [AtomicU8],
    ) -> Option<Self> {
        (pending.len() == WORDS && classes.len() == GRANULES).then_some(Self {
            pending,
            classes: ClassMap::Dense(classes),
        })
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn from_leaves(pending: &'a [AtomicU64], classes: &'a ClassLeaves) -> Option<Self> {
        (pending.len() == WORDS).then_some(Self {
            pending,
            classes: ClassMap::Leaves(classes),
        })
    }

    /// Owner-only issue, before handing the allocation to another thread.
    /// The previous incarnation's bit must already be cut and reclaimed.
    /// A separate allocation handoff must carry this Release store to a
    /// producer; the bitmap does not itself establish that handoff.
    pub(crate) fn issue(&self, offset: u32, class: u8) -> bool {
        let Some(granule) = Self::granule(offset) else {
            return false;
        };
        if usize::from(class) >= SMALL_CLASS_COUNT {
            return false;
        }
        match self.classes {
            ClassMap::Dense(classes) => {
                classes[granule].store(class + 1, Ordering::Release);
                true
            }
            ClassMap::Leaves(leaves) => leaves.issue(granule, class),
        }
    }

    /// Fixed-only fixture helper; runtime routes must call `publish_hinted`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn publish(&self, offset: u32) -> bool {
        let Some(granule) = Self::granule(offset) else {
            return false;
        };
        if self.classes.encoded(granule) == 0 {
            return false;
        }
        self.pending[granule / 64].fetch_or(1 << (granule % 64), Ordering::AcqRel);
        true
    }

    /// Runtime Small/Primordial publication: terminal pending P2, then hint P3.
    #[inline]
    pub(crate) fn publish_hinted(&self, offset: u32) -> bool {
        self.publish_hinted_with_pause(offset, || {})
    }

    /// Test-only split-point witness; closure executes after P2 while caller retains RoutePin.
    #[cfg(any(test, all(feature = "bench-internals", feature = "internals")))]
    // Lib unit tests do not call it; the source-including Loom test does.
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn publish_hinted_with_pause_for_test(
        &self,
        offset: u32,
        after_p2: impl FnOnce(),
    ) -> bool {
        self.publish_hinted_with_pause(offset, after_p2)
    }

    #[inline]
    fn publish_hinted_with_pause(&self, offset: u32, after_p2: impl FnOnce()) -> bool {
        let Some(granule) = Self::granule(offset) else {
            return false;
        };
        if self.classes.encoded(granule) == 0 {
            return false;
        }
        let word = granule / 64;
        let pending = self.pending;
        self.pending[word].fetch_or(1u64 << (granule % 64), Ordering::AcqRel);
        after_p2();
        // Arithmetic on the slice base occurs after P2, without raw pointers.
        let summary = &pending[word / 64];
        #[cfg(all(
            feature = "bench-internals",
            any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
        ))]
        super::Prf01SidecarOracle::summary_hint_load();
        let summary_bits = summary.load(Ordering::Relaxed);
        if (summary_bits >> (word & 63)) & 1 == 0 {
            Self::set_summary_hint(summary, 1u64 << (word & 63));
        }
        true
    }

    #[cold]
    #[inline(never)]
    fn set_summary_hint(summary: &AtomicU64, bit: u64) {
        summary.fetch_or(bit, Ordering::Release);
        #[cfg(all(
            feature = "bench-internals",
            any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
        ))]
        super::Prf01SidecarOracle::summary_hint_rmw();
    }

    #[cfg(all(
        feature = "bench-internals",
        any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
    ))]
    pub(crate) fn quiescent_summary_missing_bits(&self, start_word: usize, end_word: usize) -> u64 {
        let mut missing = 0u64;
        for word in start_word..end_word {
            let pending = self.pending[word].load(Ordering::Relaxed);
            if pending != 0 {
                let summary_bits = self.pending[word / 64].load(Ordering::Relaxed);
                if summary_bits & (1u64 << (word & 63)) == 0 {
                    missing += 1;
                }
            }
        }
        missing
    }

    pub(crate) fn discover_from(
        &self,
        high_water: usize,
        start_word: usize,
    ) -> Option<SummaryScan<'a>> {
        SummaryScan::new(self.pending, self.classes, high_water, start_word)
    }

    /// Owner fixes an exclusive upper bound in bytes. The caller's high water
    /// must include every issued block that can still publish. This scans
    /// exactly `ceil(high_water / (64 * MIN_BLOCK))` words, without dirty hints
    /// or waiting for paused producers. An in-flight producer after a word's
    /// cut belongs to a later scan.
    pub(crate) fn scan(&self, high_water: usize) -> Option<BitmapScan<'a>> {
        self.scan_from(high_water, 0)
    }

    /// Owner resumes at a numeric word, never at a retained sidecar borrow.
    pub(crate) fn scan_from(&self, high_water: usize, start_word: usize) -> Option<BitmapScan<'a>> {
        if high_water > SEGMENT {
            return None;
        }
        let end_word = high_water.div_ceil(MIN_BLOCK * 64);
        if start_word > end_word {
            return None;
        }
        Some(BitmapScan {
            pending: self.pending.get(start_word..)?.iter(),
            classes: self.classes,
            next_word: start_word,
            end_word,
        })
    }

    fn granule(offset: u32) -> Option<usize> {
        let offset = offset as usize;
        (offset < SEGMENT && offset.is_multiple_of(MIN_BLOCK)).then_some(offset / MIN_BLOCK)
    }
}
