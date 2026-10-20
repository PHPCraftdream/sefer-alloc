//! Owner-only route handles, allocated through System to avoid `GlobalAlloc` recursion.
#![allow(unsafe_code)]

use core::ptr;
#[cfg(feature = "bench-internals")]
use core::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::{GlobalAlloc, Layout, System};

// Diagnostic totals only, never publication/synchronization. Snapshots are
// meaningful at quiescent checkpoints; totals wrap like AtomicUsize::fetch_add.
#[cfg(feature = "bench-internals")]
static SYSTEM_TOTALS: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];

use crate::alloc_core::fatal::fatal;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::remote_bitmap::{ClassLeaves, LeafCursor};
use crate::alloc_core::segment_header::SegmentHeader;
use crate::alloc_core::segment_header::SegmentKind;
use crate::alloc_core::size_classes::MIN_BLOCK;
use crate::registry::segment_route::{RouteDirectory, RouteKind, RouteRegistration, RouteScan};

use super::issue_transaction::IssueTransaction;
use super::MAX_SEGMENTS;

pub(super) struct RouteSlots {
    slots: *mut Option<RouteRegistration<'static>>,
    cap: usize,
    owner: usize,
}

impl RouteSlots {
    #[cfg(feature = "bench-internals")]
    pub(super) fn system_totals() -> (usize, usize, usize, usize) {
        (
            SYSTEM_TOTALS[0].load(Ordering::Relaxed),
            SYSTEM_TOTALS[1].load(Ordering::Relaxed),
            SYSTEM_TOTALS[2].load(Ordering::Relaxed),
            SYSTEM_TOTALS[3].load(Ordering::Relaxed),
        )
    }

    #[cfg(feature = "bench-internals")]
    pub(super) fn slot_bytes() -> usize {
        core::mem::size_of::<Option<RouteRegistration<'static>>>()
    }

    pub(super) fn new(owner: u32, primordial: *mut u8, segment_len: usize) -> Option<Self> {
        let mut slots = Self {
            slots: ptr::null_mut(),
            cap: 0,
            owner: owner as usize,
        };
        let route = slots.prepare(
            1,
            primordial,
            segment_len,
            SegmentKind::Primordial,
            primordial,
        )?;
        slots.put(0, route);
        Some(slots)
    }

    fn ensure_capacity(&mut self, needed: usize) -> Option<()> {
        if needed <= self.cap {
            return Some(());
        }
        let cap = needed.checked_next_power_of_two()?.max(8);
        if cap > MAX_SEGMENTS {
            return None;
        }
        let layout = Layout::array::<Option<RouteRegistration<'static>>>(cap).ok()?;
        // SAFETY: System bypasses the installed global allocator. The allocation
        // is private until every slot has been initialized below.
        let fresh = unsafe { System.alloc(layout) }.cast::<Option<RouteRegistration<'static>>>();
        if fresh.is_null() {
            return None;
        }
        #[cfg(feature = "bench-internals")]
        {
            SYSTEM_TOTALS[0].fetch_add(1, Ordering::Relaxed);
            SYSTEM_TOTALS[2].fetch_add(layout.size(), Ordering::Relaxed);
        }
        for i in 0..cap {
            // SAFETY: i < cap, the allocation has the exact Option layout,
            // and each slot is written once before any read.
            unsafe { fresh.add(i).write(None) };
        }
        for i in 0..self.cap {
            // SAFETY: owner-exclusive access; old[i] is initialized and moved
            // exactly once into initialized fresh[i]. None has no destructor.
            unsafe { fresh.add(i).write(self.slots.add(i).read()) };
        }
        if self.cap != 0 {
            let old_layout = Layout::array::<Option<RouteRegistration<'static>>>(self.cap)
                .unwrap_or_else(|_| fatal(b"route_slots: route-slot array layout overflow\n"));
            // SAFETY: all handles were moved; System owns this exact allocation.
            unsafe { System.dealloc(self.slots.cast(), old_layout) };
            #[cfg(feature = "bench-internals")]
            {
                SYSTEM_TOTALS[1].fetch_add(1, Ordering::Relaxed);
                SYSTEM_TOTALS[3].fetch_add(old_layout.size(), Ordering::Relaxed);
            }
        }
        self.slots = fresh;
        self.cap = cap;
        Some(())
    }

    pub(super) fn prepare(
        &mut self,
        needed: usize,
        base: *mut u8,
        len: usize,
        kind: SegmentKind,
        payload: *mut u8,
    ) -> Option<RouteRegistration<'static>> {
        self.ensure_capacity(needed)?;
        let kind = match kind {
            SegmentKind::Primordial => RouteKind::Primordial,
            SegmentKind::Small => RouteKind::Small,
            SegmentKind::Large => RouteKind::Large,
            SegmentKind::Unknown => return None,
        };
        RouteDirectory::global()
            .register_route(base, len, payload, self.owner, kind)
            .ok()
    }

    pub(super) fn put(&mut self, index: usize, route: RouteRegistration<'static>) {
        if index >= self.cap {
            fatal(b"route_slots: put index out of range\n");
        }
        // SAFETY: owner-only access, initialized slot, and index < cap.
        let slot = unsafe { &mut *self.slots.add(index) };
        if slot.is_some() {
            fatal(b"route_slots: put into occupied route slot\n");
        }
        *slot = Some(route);
    }

    pub(super) fn issue_small(&self, index: usize, base: *mut u8, offset: u32, class: u8) {
        if index >= self.cap {
            fatal(b"route_slots: issue_small index out of range\n");
        }
        // SAFETY: owner-only access to an initialized slot in the live array.
        let route = unsafe { &*self.slots.add(index) }
            .as_ref()
            .unwrap_or_else(|| fatal(b"route_slots: issue_small on empty route slot\n"));
        // Commit cannot allocate: the owner completed every fallible prepare.
        if route.root() != base
            || !route
                .small_sidecar()
                .is_some_and(|sidecar| sidecar.prepared(offset, class))
            || !route.issue_small(offset, class)
        {
            fatal(b"route_slots: small issue commit rejected\n");
        }
    }

    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(super) fn prepare_small_issue(
        &self,
        index: usize,
        base: *mut u8,
        offset: u32,
        class: u8,
    ) -> Option<IssueTransaction> {
        if index >= self.cap {
            fatal(b"route_slots: prepare index out of range\n");
        }
        // SAFETY: owner-only access to an initialized live route slot.
        let route = unsafe { &*self.slots.add(index) }
            .as_ref()
            .unwrap_or_else(|| fatal(b"route_slots: prepare on empty route slot\n"));
        if route.root() != base {
            fatal(b"route_slots: prepare base does not match route root\n");
        }
        let prepared = route.prepare_small(offset, class);
        if !prepared && route.small_sidecar().is_none() {
            fatal(b"route_slots: prepare_small failed without sidecar\n");
        }
        prepared.then_some(IssueTransaction::prepared(index, base, offset, class))
    }

    /// Run-level twin of [`Self::prepare_small_issue`]: the fatal guards (live
    /// slot, matching root, Small sidecar) run ONCE per refill batch; the
    /// per-block OOM moves to `RoutedIssueRun::prepare_for` (leaf changes).
    pub(super) fn prepare_small_issue_run(&self, base: *mut u8, class: u8) -> RoutedIssueRun<'_> {
        let index = SegmentHeader::segment_id_at(base) as usize;
        if index >= self.cap {
            fatal(b"route_slots: issue run index out of range\n");
        }
        // SAFETY: owner-only access to an initialized live route slot.
        let route = unsafe { &*self.slots.add(index) }
            .as_ref()
            .unwrap_or_else(|| fatal(b"route_slots: issue run on empty route slot\n"));
        if route.root() != base {
            fatal(b"route_slots: issue run base does not match route root\n");
        }
        let sidecar = route
            .small_sidecar()
            .unwrap_or_else(|| fatal(b"route_slots: issue run on non-Small route\n"));
        RoutedIssueRun {
            classes: sidecar.classes(),
            class,
            cursor: None,
        }
    }

    pub(super) fn scan_small_from(
        &self,
        index: usize,
        base: *mut u8,
        high_water: usize,
        word: usize,
    ) -> Option<RouteScan<'_>> {
        if index >= self.cap {
            fatal(b"route_slots: scan index out of range\n");
        }
        // SAFETY: owner-only read of an initialized live route slot.
        let route = unsafe { &*self.slots.add(index) }
            .as_ref()
            .unwrap_or_else(|| fatal(b"route_slots: scan on empty route slot\n"));
        if route.root() != base {
            fatal(b"route_slots: scan base does not match route root\n");
        }
        route.small_sidecar()?.scan_from(high_water, word)
    }

    pub(super) fn claim_large_pending(&self, index: usize, base: *mut u8) -> bool {
        if index >= self.cap {
            fatal(b"route_slots: claim index out of range\n");
        }
        // SAFETY: owner-only read of an initialized live route slot.
        let route = unsafe { &*self.slots.add(index) }
            .as_ref()
            .unwrap_or_else(|| fatal(b"route_slots: claim on empty route slot\n"));
        if route.root() != base {
            fatal(b"route_slots: claim base does not match route root\n");
        }
        route.claim_large_pending().is_some()
    }

    pub(super) fn remove(&mut self, index: usize) {
        if index >= self.cap {
            fatal(b"route_slots: remove index out of range\n");
        }
        // SAFETY: owner-only access to an initialized slot in the live array.
        let slot = unsafe { &mut *self.slots.add(index) };
        if slot.take().is_none() {
            fatal(b"route_slots: remove of empty route slot\n");
        }
    }

    pub(super) fn close_all(&mut self) {
        for i in 0..self.cap {
            // SAFETY: owner-only access to initialized slots in the live array.
            let slot = unsafe { &mut *self.slots.add(i) };
            let _ = slot.take();
        }
    }
}

/// Whole-batch issue run over one route's class leaves: the fatal route
/// guards run once in [`RouteSlots::prepare_small_issue_run`], leaf backing
/// is resolved once per 256-granule leaf CHANGE, and each block costs one
/// Release store ([`Self::commit`]).
pub(super) struct RoutedIssueRun<'a> {
    classes: &'a ClassLeaves,
    class: u8,
    cursor: Option<LeafCursor>,
}

impl RoutedIssueRun<'_> {
    /// Fallible per-block half: promote the block's leaf if the run has not
    /// seen it yet (OOM-shaped `false`, same as the per-block prepare).
    pub(super) fn prepare_for(&mut self, offset: u32) -> bool {
        let off = offset as usize;
        let granule = off / MIN_BLOCK;
        if off >= SEGMENT || !off.is_multiple_of(MIN_BLOCK) {
            return false;
        }
        // The hoist: same-leaf blocks cost two shifts + compare, zero atomics.
        if self
            .cursor
            .as_ref()
            .is_some_and(|c| c.leaf_idx() == LeafCursor::leaf_index(granule))
        {
            #[cfg(all(feature = "internals", feature = "bench-internals"))]
            if ClassLeaves::fail_prepare_injected() {
                return false;
            }
            return true;
        }
        match self.classes.prepare_leaf(granule, self.class) {
            Some(cursor) => {
                self.cursor = Some(cursor);
                true
            }
            None => false,
        }
    }

    /// Infallible per-block half: one Release class store. On a leaf CHANGE
    /// (non-monotonic freelist offsets, or a skipped prepare) the leaf is
    /// re-resolved first, and a never-promoted leaf fatals — a skipped
    /// prepare cannot silently store.
    pub(super) fn commit(&mut self, offset: u32) {
        let off = offset as usize;
        debug_assert!(off < SEGMENT, "batch issue commit offset out of segment");
        debug_assert!(
            off.is_multiple_of(MIN_BLOCK),
            "batch issue commit offset not block aligned"
        );
        let granule = off / MIN_BLOCK;
        let same_leaf = self
            .cursor
            .as_ref()
            .is_some_and(|c| c.leaf_idx() == LeafCursor::leaf_index(granule));
        if !same_leaf {
            let cursor = self
                .classes
                .resolve_leaf(granule)
                .unwrap_or_else(|| fatal(b"route_slots: batch issue commit on unprepared leaf\n"));
            self.cursor = Some(cursor);
        }
        self.classes
            .issue_in(self.cursor.as_ref().unwrap(), granule, self.class);
    }
}

impl Drop for RouteSlots {
    fn drop(&mut self) {
        self.close_all();
        if self.cap == 0 {
            return;
        }
        let layout = Layout::array::<Option<RouteRegistration<'static>>>(self.cap)
            .unwrap_or_else(|_| fatal(b"route_slots: route-slot array layout overflow\n"));
        // SAFETY: all handles were dropped; System owns this exact allocation.
        unsafe { System.dealloc(self.slots.cast(), layout) };
        #[cfg(feature = "bench-internals")]
        {
            SYSTEM_TOTALS[1].fetch_add(1, Ordering::Relaxed);
            SYSTEM_TOTALS[3].fetch_add(layout.size(), Ordering::Relaxed);
        }
    }
}
