//! Owner-only bounded drain of terminal route-sidecar publications.

use super::{AllocCore, LARGE_REMOTE_RETIREMENTS};
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta};
use crate::alloc_core::size_classes::MIN_BLOCK;

#[cfg(feature = "bench-internals")]
pub(crate) static LARGE_SIDECAR_SLOT_INSPECTIONS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "fastbin", feature = "bench-internals"))]
pub(crate) static LARGE_SIDECAR_FULL_RESCUES: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// M-C oracle (Ph4c, receipt open question M-C): process-wide count of FULL
/// owner-side ingress drain passes (`drain_sidecar_ingress` — the strict
/// trim / re-claim pass, not the bounded worker step and not the Large
/// hot/rescue scans). `trim_for_recycle` performs EXACTLY ONE, so a mutant
/// that consumes ingress twice in one trim (or suppresses it) moves this
/// counter and goes red under
/// `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs`. Pure numeric
/// observer under `bench-internals` — never read by production logic.
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "bench-internals"
))]
pub(crate) static SIDECAR_INGRESS_DRAIN_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Ph4c mutant catcher #12: process-wide count of BOUNDED background
/// maintenance ingress steps (`HeapCore::background_maintenance_step` — one
/// call per maintained registry slot in `HeapRegistry::maintenance_pass`).
/// The bounded drain is cursor-idempotent (each record is consumed exactly
/// once no matter how many steps run), so the record/retirement counters
/// CANNOT distinguish one step from two; only this step-count observer can.
/// A mutant that runs `MaintenanceLease::with_core` twice per maintained
/// slot in `maintenance_pass` doubles this delta and goes red under
/// `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs`. Pure numeric
/// observer under `bench-internals` — never read by production logic.
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "bench-internals"
))]
pub(crate) static BACKGROUND_INGRESS_STEP_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// M-C oracle companion: process-wide count of individual sidecar records
/// consumed (Small cuts popped + Large routes claimed) by the full
/// [`AllocCore::drain_sidecar_ingress`](crate::alloc_core::alloc_core::AllocCore::drain_sidecar_ingress) pass. Distinguishes "0 trims" (delta 0 —
/// the receipt's M-C weakness: an idempotent trim is indistinguishable from
/// no trim) from "exactly 1" (delta = pending publications) and would expose
/// a double record consumption (delta above the pending count — impossible
/// today via the drain's CAS-based idempotence, which this counter pins
/// observably). Pure numeric observer under `bench-internals`.
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "bench-internals"
))]
pub(crate) static SIDECAR_INGRESS_RECORDS_CONSUMED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

// Provisional: four probes per miss; real L=8/64 cost and lag are tested.
#[cfg(feature = "fastbin")]
pub(crate) const LARGE_HOT_BUDGET: usize = 4;

impl AllocCore {
    /// Patch S: the word a resumed cursor starts at for slot `index` — the
    /// payload's first word of that slot, or 0 for a null/Large slot (its word
    /// component is unused and rewritten on the next advance).
    fn sidecar_word_hint(&self, index: usize) -> usize {
        if index >= self.table.count() as usize {
            return 0;
        }
        let base = self.table.base_at(index);
        if base.is_null() {
            return 0;
        }
        match SegmentHeader::kind_at(base) {
            SegmentKind::Small | SegmentKind::Primordial => Self::sidecar_payload_start_word(base),
            _ => 0,
        }
    }

    /// At most `budget` slot inspections or word cuts. Cursor holds no root.
    /// Returns (retired records, charged operations).
    pub(crate) fn drain_sidecar_ingress_bounded(
        &mut self,
        cursor: &mut (usize, usize),
        budget: usize,
    ) -> (usize, usize) {
        if !self.table.is_routed() || budget == 0 {
            return (0, 0);
        }
        let end = self.table.count() as usize;
        if cursor.0 >= end {
            *cursor = (0, self.sidecar_word_hint(0));
        }
        let mut reclaimed = 0;
        let mut units = 0;
        while cursor.0 < end && units < budget {
            let index = cursor.0;
            let base = self.table.base_at(index);
            if base.is_null() {
                cursor.0 += 1;
                cursor.1 = self.sidecar_word_hint(cursor.0);
                units += 1;
                continue;
            }
            match SegmentHeader::kind_at(base) {
                SegmentKind::Small | SegmentKind::Primordial => {
                    let high_water = SegmentMeta::new(base).bump_of();
                    let end_word = high_water.div_ceil(MIN_BLOCK * 64);
                    // Patch S: never scan below the payload's first word.
                    let start_word = Self::sidecar_payload_start_word(base);
                    if cursor.1 < start_word {
                        cursor.1 = start_word;
                    }
                    if cursor.1 >= end_word {
                        cursor.0 += 1;
                        cursor.1 = self.sidecar_word_hint(cursor.0);
                        units += 1;
                        continue;
                    }
                    let mut changed_classes = 0u64;
                    {
                        let Some(mut scan) = self
                            .table
                            .scan_small_route_from(index, base, high_water, cursor.1)
                        else {
                            std::process::abort();
                        };
                        let Some(mut cut) = scan.next_cut() else {
                            std::process::abort();
                        };
                        while let Some(record) = cut.pop() {
                            if Self::reclaim_sidecar_record(base, record.offset, record.class) {
                                changed_classes |= 1u64 << record.class;
                                reclaimed += 1;
                            }
                        }
                    }
                    units += 1;
                    cursor.1 += 1;
                    // All route borrows and detached records are consumed
                    // before a reservation can be released or pooled.
                    #[cfg(feature = "alloc-segment-directory")]
                    self.sync_directory_for_segment_classes(base, index, changed_classes);
                    #[cfg(not(feature = "alloc-segment-directory"))]
                    let _ = changed_classes;
                    #[cfg(feature = "alloc-decommit")]
                    if changed_classes != 0
                        && Self::dec_live_and_maybe_decommit(base, self.small_cur)
                    {
                        let _ = self.release_or_pool_empty_segment(base);
                        cursor.0 += 1;
                        cursor.1 = self.sidecar_word_hint(cursor.0);
                        continue;
                    }
                    if cursor.1 == end_word {
                        cursor.0 += 1;
                        cursor.1 = self.sidecar_word_hint(cursor.0);
                    }
                }
                SegmentKind::Large => {
                    if self.table.claim_large_route(index, base) {
                        self.reclaim_large_segment(base);
                        LARGE_REMOTE_RETIREMENTS
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        reclaimed += 1;
                    }
                    cursor.0 += 1;
                    cursor.1 = self.sidecar_word_hint(cursor.0);
                    units += 1;
                }
                SegmentKind::Unknown => std::process::abort(),
            }
        }
        if cursor.0 == end {
            *cursor = (0, self.sidecar_word_hint(0));
        }
        (reclaimed, units)
    }

    /// Fixes the table high-water at entry and visits each live slot once.
    /// The owner lease excludes issue/reuse throughout this pass. Each Small
    /// word is exchanged exactly once; post-cut publishers wait for a later
    /// pass and keep their outstanding credits. No dirty hint or producer
    /// quiescence is required. Only stored table roots access reservations.
    pub(crate) fn drain_sidecar_ingress(&mut self) -> usize {
        #[cfg(all(
            feature = "alloc-global",
            feature = "alloc-xthread",
            feature = "bench-internals"
        ))]
        SIDECAR_INGRESS_DRAIN_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if !self.table.is_routed() {
            return 0;
        }
        let end = self.table.count() as usize;
        let mut reclaimed = 0;
        for index in 0..end {
            let base = self.table.base_at(index);
            if base.is_null() {
                continue;
            }
            match SegmentHeader::kind_at(base) {
                SegmentKind::Small | SegmentKind::Primordial => {
                    let high_water = SegmentMeta::new(base).bump_of();
                    let mut changed_classes = 0u64;
                    {
                        let Some(mut scan) = self.table.scan_small_route(index, base, high_water)
                        else {
                            std::process::abort();
                        };
                        while let Some(mut cut) = scan.next_cut() {
                            while let Some(record) = cut.pop() {
                                if Self::reclaim_sidecar_record(base, record.offset, record.class) {
                                    changed_classes |= 1u64 << record.class;
                                    reclaimed += 1;
                                    #[cfg(all(
                                        feature = "alloc-global",
                                        feature = "alloc-xthread",
                                        feature = "bench-internals"
                                    ))]
                                    SIDECAR_INGRESS_RECORDS_CONSUMED
                                        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                                }
                            }
                        }
                    }
                    // Every cut and the route borrow are gone before table
                    // removal or payload decommit. A detached or unpublished
                    // valid instance still owns a credit, so only a fully
                    // consumed segment can reach zero here.
                    #[cfg(feature = "alloc-segment-directory")]
                    self.sync_directory_for_segment_classes(base, index, changed_classes);
                    #[cfg(not(feature = "alloc-segment-directory"))]
                    let _ = changed_classes;
                    #[cfg(feature = "alloc-decommit")]
                    if changed_classes != 0
                        && Self::dec_live_and_maybe_decommit(base, self.small_cur)
                    {
                        let _ = self.release_or_pool_empty_segment(base);
                    }
                }
                SegmentKind::Large => {
                    if self.table.claim_large_route(index, base) {
                        self.reclaim_large_segment(base);
                        LARGE_REMOTE_RETIREMENTS
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        reclaimed += 1;
                        #[cfg(all(
                            feature = "alloc-global",
                            feature = "alloc-xthread",
                            feature = "bench-internals"
                        ))]
                        SIDECAR_INGRESS_RECORDS_CONSUMED
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                }
                SegmentKind::Unknown => std::process::abort(),
            }
        }
        reclaimed
    }

    /// Large-only hot scan: the route descriptor must surrender its terminal
    /// obligation before physical retirement. Before publication its instance
    /// credit pins the reservation; afterward a pin retains only the descriptor.
    pub(crate) fn drain_large_sidecar_ingress(&mut self) -> usize {
        if !self.table.is_routed() {
            return 0;
        }
        let end = self.table.count() as usize;
        let mut reclaimed = 0;
        let mut from = 0;
        while let Some(index) = self.table.next_active(SegmentKind::Large, from) {
            if index >= end {
                break;
            }
            from = index + 1;
            #[cfg(feature = "bench-internals")]
            LARGE_SIDECAR_SLOT_INSPECTIONS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            let base = self.table.base_at(index);
            if base.is_null() || SegmentHeader::kind_at(base) != SegmentKind::Large {
                std::process::abort();
            }
            if self.table.claim_large_route(index, base) {
                self.reclaim_large_segment(base);
                LARGE_REMOTE_RETIREMENTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                reclaimed += 1;
            }
        }
        reclaimed
    }

    /// Inspect at most four active Large routes on a Small magazine miss.
    /// The owner retains only a numeric next slot, across refills and churn.
    #[cfg(feature = "fastbin")]
    pub(crate) fn drain_large_sidecar_ingress_hot_bounded(&mut self, cursor: &mut usize) -> usize {
        if !self.table.is_routed() {
            return 0;
        }
        let end = self.table.count() as usize;
        if end == 0 {
            return 0;
        }
        if *cursor >= end {
            *cursor = 0;
        }
        let start = *cursor;
        let mut from = start;
        let mut wrapped = false;
        let mut inspected = 0;
        let mut reclaimed = 0;
        while inspected < LARGE_HOT_BUDGET {
            let Some(index) = self.table.next_active(SegmentKind::Large, from) else {
                if wrapped {
                    break;
                }
                wrapped = true;
                from = 0;
                continue;
            };
            if index >= end || (wrapped && index >= start) {
                break;
            }
            // Advance before a successful claim can unregister this slot.
            from = index + 1;
            inspected += 1;
            #[cfg(feature = "bench-internals")]
            LARGE_SIDECAR_SLOT_INSPECTIONS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            let base = self.table.base_at(index);
            if base.is_null() || SegmentHeader::kind_at(base) != SegmentKind::Large {
                std::process::abort();
            }
            if self.table.claim_large_route(index, base) {
                self.reclaim_large_segment(base);
                LARGE_REMOTE_RETIREMENTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                reclaimed += 1;
            }
        }
        *cursor = if inspected == 0 {
            start
        } else if from == end {
            0
        } else {
            from
        };
        reclaimed
    }

    /// Full cold retry after a Small refill yielded no block.
    #[cfg(feature = "fastbin")]
    pub(crate) fn drain_large_sidecar_ingress_rescue(&mut self) -> usize {
        #[cfg(feature = "bench-internals")]
        LARGE_SIDECAR_FULL_RESCUES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.drain_large_sidecar_ingress()
    }

    #[cfg(all(feature = "fastbin", feature = "bench-internals"))]
    pub(crate) fn dbg_large_sidecar_full_rescues() -> u64 {
        LARGE_SIDECAR_FULL_RESCUES.load(core::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(all(feature = "fastbin", feature = "bench-internals"))]
    pub(crate) const fn dbg_large_hot_budget() -> usize {
        LARGE_HOT_BUDGET
    }
}
