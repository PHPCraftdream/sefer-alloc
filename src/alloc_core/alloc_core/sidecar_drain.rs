//! Owner-only bounded drain of terminal route-sidecar publications.

use super::{AllocCore, LARGE_REMOTE_RETIREMENTS};
use crate::alloc_core::fatal::fatal;
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

/// Reservation disposition of a drained Small segment after
/// [`AllocCore::finish_small_drain`]. `Pooled`/`Released` both mean the
/// reservation left the live set this pass (a pooled segment stays
/// registered/committed, a released one is unmapped) — the same split the
/// discovery drain reports as `SidecarDrainOutcome::Decommitted`.
pub(crate) enum DrainEnd {
    Live,
    #[cfg(feature = "alloc-decommit")]
    Pooled,
    #[cfg(feature = "alloc-decommit")]
    Released,
}

/// Per-call-site bench observers for one Small cut-consume pass. Each owner
/// drain keeps exactly its own counters — the discovery drain's R19 summary
/// oracle ([`DiscoverySummaryObserver`]), the strict trim's `strict_visited`
/// oracle and record-consumed counter ([`StrictTrimObserver`]), and the
/// bounded maintenance step's no-op ([`NoSmallCutObserver`]). Pure numeric
/// observers; never read by production logic.
pub(crate) trait SmallCutObserver {
    /// One terminal cut was detached from the routed scan; `nonempty` says
    /// whether it carried at least one record.
    fn cut_visited(&mut self, nonempty: bool);
    /// One record was reclaimed into its segment's free state.
    fn record_reclaimed(&mut self);
    /// The pass finished; report accumulated totals.
    fn finish(&mut self);
}

/// The bounded maintenance step observes no Small-cut counters.
pub(crate) struct NoSmallCutObserver;

impl SmallCutObserver for NoSmallCutObserver {
    fn cut_visited(&mut self, _nonempty: bool) {}
    fn record_reclaimed(&mut self) {}
    fn finish(&mut self) {}
}

/// Discovery-drain ([`AllocCore::drain_segment_sidecar`]) observer: feeds
/// `Prf01SidecarOracle::completed` with the visited / nonempty / reclaimed
/// totals under `cfg(r19_sidecar_summary_oracle)`.
#[derive(Default)]
pub(crate) struct DiscoverySummaryObserver {
    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    visited: u64,
    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    nonempty: u64,
    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    reclaimed: u64,
}

impl SmallCutObserver for DiscoverySummaryObserver {
    fn cut_visited(&mut self, nonempty: bool) {
        #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
        {
            self.visited += 1;
            self.nonempty += u64::from(nonempty);
        }
    }

    fn record_reclaimed(&mut self) {
        #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
        {
            self.reclaimed += 1;
        }
    }

    fn finish(&mut self) {
        #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
        crate::alloc_core::remote_bitmap::Prf01SidecarOracle::completed(
            self.visited,
            self.nonempty,
            self.reclaimed,
        );
    }
}

/// Strict-trim ([`AllocCore::drain_sidecar_ingress`]) observer: per-pass
/// cut/record totals feed `Prf01SidecarOracle::strict_visited`, and every
/// reclaimed record also bumps `SIDECAR_INGRESS_RECORDS_CONSUMED` (the M-C
/// oracle's record counter).
#[derive(Default)]
pub(crate) struct StrictTrimObserver {
    #[cfg(feature = "bench-internals")]
    actual: u64,
    #[cfg(feature = "bench-internals")]
    records: u64,
}

impl SmallCutObserver for StrictTrimObserver {
    fn cut_visited(&mut self, _nonempty: bool) {
        #[cfg(feature = "bench-internals")]
        {
            self.actual += 1;
        }
    }

    fn record_reclaimed(&mut self) {
        #[cfg(feature = "bench-internals")]
        {
            self.records += 1;
        }
        #[cfg(all(
            feature = "alloc-global",
            feature = "alloc-xthread",
            feature = "bench-internals"
        ))]
        SIDECAR_INGRESS_RECORDS_CONSUMED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    fn finish(&mut self) {
        #[cfg(feature = "bench-internals")]
        crate::alloc_core::remote_bitmap::Prf01SidecarOracle::strict_visited(
            self.actual,
            self.records,
        );
    }
}

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

    /// Shared Small drain finalization — the reservation-lifetime boundary:
    /// every cut and every route borrow must already be consumed by the
    /// caller, because from here the segment's reservation can be released
    /// or pooled. Syncs the directory bitmap with the classes actually
    /// reclaimed, then — only when something was reclaimed — drops the empty
    /// segment's live count and releases/pools it.
    #[inline]
    pub(crate) fn finish_small_drain(
        &mut self,
        base: *mut u8,
        index: usize,
        changed_classes: u64,
    ) -> DrainEnd {
        #[cfg(feature = "alloc-segment-directory")]
        self.sync_directory_for_segment_classes(base, index, changed_classes);
        #[cfg(not(feature = "alloc-segment-directory"))]
        let _ = changed_classes;
        #[cfg(feature = "alloc-decommit")]
        if changed_classes != 0 && Self::dec_live_and_maybe_decommit(base, self.small_cur) {
            let pooled = self.release_or_pool_empty_segment(base);
            return if pooled {
                DrainEnd::Pooled
            } else {
                DrainEnd::Released
            };
        }
        DrainEnd::Live
    }

    /// Claim one pending Large route and physically retire it: the route
    /// descriptor surrenders its terminal obligation before the reservation
    /// is reclaimed. `true` means the retirement happened and was counted in
    /// `LARGE_REMOTE_RETIREMENTS`; callers add only their own bookkeeping on
    /// `true`.
    #[inline]
    pub(crate) fn retire_large_route(&mut self, index: usize, base: *mut u8) -> bool {
        if !self.table.claim_large_route(index, base) {
            return false;
        }
        self.reclaim_large_segment(base);
        LARGE_REMOTE_RETIREMENTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        true
    }

    /// Create the routed Small scan or abort. All three owner drains share
    /// one availability contract (a routed table must yield a scan); each
    /// call site keeps its own fatal text so evidence recipes stay
    /// byte-stable.
    #[inline]
    pub(crate) fn scan_small_route_or_fatal(
        &self,
        index: usize,
        base: *mut u8,
        high_water: usize,
        word: usize,
        unavailable: &'static [u8],
    ) -> crate::registry::segment_route::RouteScan<'_> {
        let Some(scan) = self
            .table
            .scan_small_route_from(index, base, high_water, word)
        else {
            fatal(unavailable);
        };
        scan
    }

    /// Consume every terminal cut the routed scan yields: pop each record
    /// into its segment's free state and accumulate `(changed_classes,
    /// reclaimed)` for the caller's finalization. The caller owns the scan
    /// and its fatal text; this pass ends all route borrows and detached
    /// cuts, so the caller's next call may release or pool the reservation
    /// ([`Self::finish_small_drain`]). `observer` carries the call site's
    /// bench counters only — the body itself touches none.
    #[inline]
    pub(crate) fn consume_small_cuts<O: SmallCutObserver>(
        base: *mut u8,
        scan: crate::registry::segment_route::RouteScan<'_>,
        observer: O,
    ) -> (u64, usize) {
        Self::consume_small_cuts_impl(base, scan, observer, false)
    }

    /// Exactly-one-cut variant for the bounded maintenance step: the word
    /// cursor advances one payload word per step, so a step detaches at most
    /// the scan's first cut — and aborts when the routed scan yields none.
    #[inline]
    pub(crate) fn consume_one_small_cut<O: SmallCutObserver>(
        base: *mut u8,
        scan: crate::registry::segment_route::RouteScan<'_>,
        observer: O,
    ) -> (u64, usize) {
        Self::consume_small_cuts_impl(base, scan, observer, true)
    }

    fn consume_small_cuts_impl<O: SmallCutObserver>(
        base: *mut u8,
        mut scan: crate::registry::segment_route::RouteScan<'_>,
        mut observer: O,
        exactly_one_cut: bool,
    ) -> (u64, usize) {
        let mut changed_classes = 0u64;
        let mut reclaimed = 0usize;
        loop {
            let Some(mut cut) = scan.next_cut() else {
                if exactly_one_cut {
                    fatal(b"sidecar_drain: sidecar scan cut unavailable\n");
                }
                break;
            };
            observer.cut_visited(!cut.is_empty());
            while let Some(record) = cut.pop() {
                if Self::reclaim_sidecar_record(base, record.offset, record.class) {
                    changed_classes |= 1u64 << record.class;
                    reclaimed += 1;
                    observer.record_reclaimed();
                }
            }
            if exactly_one_cut {
                break;
            }
        }
        observer.finish();
        (changed_classes, reclaimed)
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
                    let (changed_classes, cut_reclaimed) = {
                        let scan = self.scan_small_route_or_fatal(
                            index,
                            base,
                            high_water,
                            cursor.1,
                            b"sidecar_drain: routed scan unavailable\n",
                        );
                        Self::consume_one_small_cut(base, scan, NoSmallCutObserver)
                    };
                    reclaimed += cut_reclaimed;
                    units += 1;
                    cursor.1 += 1;
                    // All route borrows and detached records are consumed
                    // before a reservation can be released or pooled.
                    if !matches!(
                        self.finish_small_drain(base, index, changed_classes),
                        DrainEnd::Live
                    ) {
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
                    if self.retire_large_route(index, base) {
                        reclaimed += 1;
                    }
                    cursor.0 += 1;
                    cursor.1 = self.sidecar_word_hint(cursor.0);
                    units += 1;
                }
                SegmentKind::Unknown => fatal(b"sidecar_drain: unknown segment kind\n"),
            }
        }
        if cursor.0 == end {
            *cursor = (0, self.sidecar_word_hint(0));
        }
        (reclaimed, units)
    }

    /// Fixes the table high-water at entry and visits each live slot once.
    /// Strict trim cuts payload words only: producer rejects class-0 metadata
    /// (`sidecar_bitmap.rs:116-121`); metadata is >=64KiB/page-aligned
    /// (`segment_header_layout.rs:44-72,126-127`).
    /// The owner lease excludes issue/reuse throughout this pass. Each Small
    /// word is visited exactly once; post-cut publishers wait for a later
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
                    // Producer rejects class-0 metadata (sidecar_bitmap.rs:116-121); metadata is >=64 KiB, page aligned (segment_header_layout.rs:44-72, 126-127).
                    let end_word = high_water.div_ceil(MIN_BLOCK * 64);
                    let start = Self::sidecar_payload_start_word(base).min(end_word);
                    let (changed_classes, drained_records) = {
                        let scan = self.scan_small_route_or_fatal(
                            index,
                            base,
                            high_water,
                            start,
                            b"sidecar_drain: routed scan unavailable\n",
                        );
                        Self::consume_small_cuts(base, scan, StrictTrimObserver::default())
                    };
                    reclaimed += drained_records;
                    // Every cut and the route borrow are gone before table
                    // removal or payload decommit. A detached or unpublished
                    // valid instance still owns a credit, so only a fully
                    // consumed segment can reach zero here.
                    self.finish_small_drain(base, index, changed_classes);
                }
                SegmentKind::Large => {
                    if self.retire_large_route(index, base) {
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
                SegmentKind::Unknown => fatal(b"sidecar_drain: unknown segment kind\n"),
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
            // Invariant: `next_active(SegmentKind::Large, ..)` walks the
            // `ActiveKindIndex`, and the route slot at `index` stores the same
            // root (`claim_large_pending` aborts on `root != base`), so the
            // header kind re-read is only a debug check, not a release abort.
            debug_assert!(
                !base.is_null() && SegmentHeader::kind_at(base) == SegmentKind::Large,
                "active Large slot must hold a live Large segment root"
            );
            if self.retire_large_route(index, base) {
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
            // Invariant: `next_active(SegmentKind::Large, ..)` walks the
            // `ActiveKindIndex`, and the route slot at `index` stores the same
            // root (`claim_large_pending` aborts on `root != base`), so the
            // header kind re-read is only a debug check, not a release abort.
            debug_assert!(
                !base.is_null() && SegmentHeader::kind_at(base) == SegmentKind::Large,
                "active Large slot must hold a live Large segment root"
            );
            if self.retire_large_route(index, base) {
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
