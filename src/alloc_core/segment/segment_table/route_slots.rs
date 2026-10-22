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
use crate::alloc_core::segment_header::SegmentKind;
use crate::registry::segment_route::{RouteDirectory, RouteKind, RouteRegistration, RouteScan};

use super::{SmallIssueRun, MAX_SEGMENTS};

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

    /// D1′ staged issue (phase-2 OPT): the fatal slot guards run ONCE per
    /// run — this constructor — and the returned handle does only the cheap
    /// per-block sidecar operations (`issue` / `clear_pending_start_bit`).
    /// Same fatal guards as the former per-block `issue_small`:
    /// index < cap, occupied slot, root == base, Small sidecar present.
    pub(super) fn small_issue_run(&self, index: usize, base: *mut u8) -> SmallIssueRun<'_> {
        self.small_sidecar_or_fatal(index, base);
        // SAFETY: helper проверил index, occupied slot, root и Small-kind.
        let route = unsafe { &*self.slots.add(index) }
            .as_ref()
            .unwrap_or_else(|| fatal(b"route_slots: issue on empty route slot\n"));
        SmallIssueRun::routed(route)
    }

    /// D1′ phase-1a: owner-only guard helper resolving the live Small
    /// sidecar for `base`'s slot (same fatal guards as `issue_small` /
    /// `scan_small_from`).
    fn small_sidecar_or_fatal(
        &self,
        index: usize,
        base: *mut u8,
    ) -> &crate::registry::segment_route::SmallSidecar {
        if index >= self.cap {
            fatal(b"route_slots: small sidecar index out of range\n");
        }
        // SAFETY: owner-only access to an initialized live route slot.
        let route = unsafe { &*self.slots.add(index) }
            .as_ref()
            .unwrap_or_else(|| fatal(b"route_slots: small sidecar on empty route slot\n"));
        if route.root() != base {
            fatal(b"route_slots: small sidecar base does not match route root\n");
        }
        route
            .small_sidecar()
            .unwrap_or_else(|| fatal(b"route_slots: small sidecar on non-Small route\n"))
    }

    /// Reclaim's start-bit check: the published record's start granule MUST
    /// carry its start bit (a published pending bit with start bit 0 is
    /// corruption; the caller fatals on `false`).
    pub(super) fn reclaim_start_bit(&self, index: usize, base: *mut u8, offset: u32) -> bool {
        self.small_sidecar_or_fatal(index, base).start_bit(offset)
    }

    /// Reclaim's production overlap check: any start bit set in the OPEN
    /// interval `[start, end)` (start EXCLUDES the record's own granule,
    /// end EXCLUDES the block end) means the claimed block spans an issued
    /// neighbour — corruption; the caller fatals on `true`.
    pub(super) fn reclaim_start_overlap(
        &self,
        index: usize,
        base: *mut u8,
        start: u32,
        end: u32,
    ) -> bool {
        self.small_sidecar_or_fatal(index, base)
            .start_bits_between(start, end)
    }

    /// Retain-reset: atomically clear pending AND start bits over the
    /// granule range `[start_off, end_off)` (the state-byte reset range).
    // Only the retain-reset hook (bench-internals + internals +
    // alloc-decommit) reaches this seam; allow the production-only build.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub(super) fn clear_remote_bits(
        &self,
        index: usize,
        base: *mut u8,
        start_off: u32,
        end_off: u32,
    ) {
        self.small_sidecar_or_fatal(index, base)
            .clear_remote_bits_range(start_off, end_off);
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

        let sidecar = route.small_sidecar()?;
        // B4 U31 trigger observer (OPEN_ITEMS item 90): pure Relaxed
        // fetch_adds; no RMW on `Entry.refs` and no ordering effect on the
        // scan path. Base `refs` is 1 (owner reference); == 1 means no pins
        // (including R1 TLS pin-cache pins) are held anywhere.
        #[cfg(feature = "bench-internals")]
        {
            use core::sync::atomic::Ordering;
            crate::alloc_core::B4_U31_SCAN_SEGMENTS.fetch_add(1, Ordering::Relaxed);
            if route.dbg_entry_refs_relaxed() == 1 {
                crate::alloc_core::B4_U31_SCAN_HOLDERS_BASE.fetch_add(1, Ordering::Relaxed);
            }
        }
        sidecar.scan_from(high_water, word)
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
