//! Single-bank pending bits and owner-issued classes, outside the reservation.
//!
//! The future directory supplies stable, disjoint atomic backing for the
//! entire reservation lifetime. It prepares that backing on an allocation
//! path, before the segment can be published. No method allocates. A masked
//! user pointer is only an integer lookup key; this primitive never receives
//! or dereferences one. The directory must pin the backing through all
//! unpublished producers, published bits, and detached cuts.

use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

#[path = "sidecar_bitmap/leaf_classes.rs"]
#[cfg_attr(test, allow(dead_code))]
mod leaf_classes;
pub(crate) use leaf_classes::ClassLeaves;

use super::BitmapScan;

use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::size_classes::{MIN_BLOCK, SMALL_CLASS_COUNT};

const GRANULES: usize = SEGMENT / MIN_BLOCK;
const WORDS: usize = GRANULES / 64;
const SUMMARY_WORDS: usize = WORDS / 64;
const _: () = {
    assert!(SUMMARY_WORDS == 64);
    assert!(SEGMENT == 4 * 1024 * 1024);
    assert!(MIN_BLOCK == 16);
    assert!(WORDS == 4096);
    assert!(GRANULES == 262_144);
    assert!(SMALL_CLASS_COUNT < u8::MAX as usize);
};

/// A live segment's external ingress storage. Construction borrows backing;
/// it does not allocate, initialize, or bind a reservation pointer.
pub(crate) struct SidecarBitmap<'a> {
    pub(super) pending: &'a [AtomicU64],
    pub(super) summary: &'a [AtomicU64],
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
    pub(crate) const SUMMARY_WORDS: usize = SUMMARY_WORDS;
    pub(crate) const WORDS: usize = WORDS;
    pub(crate) const GRANULES: usize = GRANULES;

    /// Backing must be separate from reservation bytes and zero-initialized
    /// before segment publication. Its lifetime must extend through all cuts.
    pub(crate) fn from_initialized(
        pending: &'a [AtomicU64],
        summary: &'a [AtomicU64],
        classes: &'a [AtomicU8],
    ) -> Option<Self> {
        (pending.len() == WORDS && summary.len() == SUMMARY_WORDS && classes.len() == GRANULES)
            .then_some(Self {
                pending,
                summary,
                classes: ClassMap::Dense(classes),
            })
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn from_leaves(
        pending: &'a [AtomicU64],
        summary: &'a [AtomicU64],
        classes: &'a ClassLeaves,
    ) -> Option<Self> {
        (pending.len() == WORDS && summary.len() == SUMMARY_WORDS).then_some(Self {
            pending,
            summary,
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

    /// Producer terminal AcqRel RMW. No header or reservation access; the
    /// class map is read (Acquire) once BEFORE the RMW, only to drop a granule
    /// that was never issued (segment metadata, uncarved tail) instead of
    /// publishing a record the owner's `BitmapCut::pop` would abort on. A
    /// uniform leaf reports its class for every granule it covers, so an
    /// interior pointer into an issued block is NOT rejected here; the producer
    /// cannot read block geometry without touching a segment that a malformed
    /// free may not keep alive. This pending RMW is the linearization point and
    /// last producer access to the reclaimable reservation and ledger. Afterwards
    /// only a discovery-summary bit in the SAME sidecar may be set, using an
    /// address obtained before this RMW while RoutePin is held, then the pin
    /// released. Strict trim and periodic maintenance ignore summary.
    /// Exactly one producer publishes each valid allocation instance. An
    /// unpublished producer retains its segment credit; a published one
    /// retains it until reclaim. Only the pending word's 0 -> non-0
    /// transition sets the discovery summary bit; a producer that observed a
    /// non-zero prev performs no summary access at all.
    #[inline(always)]
    pub(crate) fn publish(&self, offset: u32) -> bool {
        self.publish_sequence::<false>(offset).is_some()
    }

    // One order for production, paused first invocation and its continuation.
    // M5' swaps this constant to [false, true].
    const HALVES: [bool; 2] = [true, false];

    /// Both the ordinary publisher and the paused hook execute this sequence.
    /// The const pause removes the second invocation, not a copy of the LP.
    #[inline(always)]
    fn publish_sequence<const PAUSE: bool>(
        &self,
        offset: u32,
    ) -> Option<(&'a AtomicU64, u64, bool)> {
        let granule = Self::granule(offset)?;
        let word = granule / 64;
        // Prepare notification capability before any actual half invocation.
        let summary = &self.summary[word / 64];
        let summary_bit = 1u64 << (word % 64);
        let mut notify = false;
        for pending_half in Self::HALVES {
            if pending_half {
                let prev = self.publish_pending_half(granule)?;
                // C'': touch summary ONLY on the pending word's 0 -> non-0
                // transition. A producer that read a non-zero prev performs
                // no summary load and no address computation; it relies on
                // the producer that moved the word to non-0. A skipped or
                // missed bit can only DELAY a pending word, never lose it:
                // only an owner cut (AcqRel swap) clears pending bits and
                // every cut's bits are reclaimed, strict trim and bounded
                // maintenance run FULL pending scans, and after a discovery
                // word is missed the surviving pending bit is re-flagged by
                // the next 0 -> non-0 publication or consumed by the next
                // full scan.
                notify = prev == 0;
            } else if notify {
                Self::publish_summary_half(summary, summary_bit);
            }
            if PAUSE {
                break;
            }
        }
        Some((summary, summary_bit, notify))
    }

    #[inline(always)]
    fn publish_pending_half(&self, granule: usize) -> Option<u64> {
        if self.classes.encoded(granule) == 0 {
            return None;
        }
        // AcqRel, minimal for this design: Release publishes the freed block
        // to the owner's Acquire cut; the Acquire half fences the conditional
        // summary RMW BELOW this linearization point. A plain Release would
        // let the summary store be hoisted above the LP: an owner could then
        // clear summary, cut the still-empty word and never be re-flagged,
        // delaying the bit until a full scan. The RMW's atomicity already
        // guarantees prev is the latest word value.
        let prev = self.pending[granule / 64].fetch_or(1 << (granule % 64), Ordering::AcqRel);
        Some(prev)
    }

    #[inline(always)]
    fn publish_summary_half(summary: &AtomicU64, summary_bit: u64) {
        // No pre-load: a redundant fetch_or on an already-set bit is harmless
        // and cheaper than a load branch. Release is sufficient: the owner
        // clears a summary word with an AcqRel swap, so its subsequent cut of
        // a flagged word observes the pending RMW sequenced before this
        // store. Relaxed would break that pairing and could hide a published
        // bit for one full-scan round.
        summary.fetch_or(summary_bit, Ordering::Release);
        #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
        super::Prf01SidecarOracle::published(1, 0);
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[cfg_attr(test, allow(dead_code))] // Source-included fixtures have no route handles.
    #[inline(always)]
    pub(crate) fn pause_publish(&self, offset: u32) -> Option<(&'a AtomicU64, u64, bool)> {
        self.publish_sequence::<true>(offset)
    }

    /// Baseline continuation uses only the saved notification capability.
    /// The lazy pending capability is invoked ONLY under the reversed-order
    /// counterfactual, where the first invocation has not performed the LP.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[cfg_attr(test, allow(dead_code))] // Source-included fixtures have no route handles.
    #[inline(always)]
    pub(crate) fn resume_publish(
        summary: &AtomicU64,
        bit: u64,
        notify: bool,
        pending: impl FnOnce() -> bool,
    ) {
        if Self::HALVES[1] {
            if !pending() {
                std::process::abort();
            }
        } else if notify {
            Self::publish_summary_half(summary, bit);
        }
    }

    /// Test-only lazy continuation for M5'; no reservation access, and never
    /// called after the pending LP in the baseline publication order.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[cfg_attr(test, allow(dead_code))] // Source-included fixtures have no route handles.
    #[inline(always)]
    pub(crate) fn resume_pending_for_test(&self, offset: u32) -> bool {
        Self::granule(offset).is_some_and(|granule| self.publish_pending_half(granule).is_some())
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
            summarized: None,
            #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
            summary_visited: 0,
        })
    }

    /// Discovery only. Clear each summary word before any of its pending cuts.
    /// Concurrent notifications may wait for the next discovery; strict and
    /// bounded maintenance must continue to use scan/scan_from instead.
    #[cfg_attr(test, allow(dead_code))] // Fixed-only source-included primitive fixtures.
    pub(crate) fn scan_summarized_from(
        &self,
        high_water: usize,
        start_word: usize,
    ) -> Option<BitmapScan<'a>> {
        let mut scan = self.scan_from(high_water, start_word)?;
        scan.summarized = Some((
            self.summary,
            self.pending,
            start_word / 64,
            0,
            0,
            start_word,
        ));
        Some(scan)
    }

    fn granule(offset: u32) -> Option<usize> {
        let offset = offset as usize;
        (offset < SEGMENT && offset.is_multiple_of(MIN_BLOCK)).then_some(offset / MIN_BLOCK)
    }
}
