//! Owner-only route handles, allocated through System to avoid `GlobalAlloc` recursion.
#![allow(unsafe_code)]

use core::ptr;
use std::alloc::{GlobalAlloc, Layout, System};

use crate::alloc_core::segment_header::SegmentKind;
use crate::registry::segment_route::{RouteDirectory, RouteKind, RouteRegistration};

use super::MAX_SEGMENTS;

pub(super) struct RouteSlots {
    slots: *mut Option<RouteRegistration<'static>>,
    cap: usize,
    owner: usize,
}

impl RouteSlots {
    pub(super) fn new(owner: u32, primordial: *mut u8, segment_len: usize) -> Option<Self> {
        let mut slots = Self {
            slots: ptr::null_mut(),
            cap: 0,
            owner: owner as usize,
        };
        let route = slots.prepare(1, primordial, segment_len, SegmentKind::Primordial)?;
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
                .unwrap_or_else(|_| std::process::abort());
            // SAFETY: every old slot was moved out, and this is System's exact
            // original allocation and layout.
            unsafe { System.dealloc(self.slots.cast(), old_layout) };
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
    ) -> Option<RouteRegistration<'static>> {
        self.ensure_capacity(needed)?;
        let kind = match kind {
            SegmentKind::Primordial => RouteKind::Primordial,
            SegmentKind::Small => RouteKind::Small,
            SegmentKind::Large => RouteKind::Large,
            SegmentKind::Unknown => return None,
        };
        RouteDirectory::global()
            .register(base, len, base, self.owner, kind)
            .ok()
    }

    pub(super) fn put(&mut self, index: usize, route: RouteRegistration<'static>) {
        if index >= self.cap {
            std::process::abort();
        }
        // SAFETY: owner-only access, initialized slot, and index < cap.
        let slot = unsafe { &mut *self.slots.add(index) };
        if slot.is_some() {
            std::process::abort();
        }
        *slot = Some(route);
    }

    pub(super) fn issue_small(&self, index: usize, base: *mut u8, offset: u32, class: u8) {
        if index >= self.cap {
            std::process::abort();
        }
        // SAFETY: owner-only access to an initialized slot in the live array.
        let route = unsafe { &*self.slots.add(index) }
            .as_ref()
            .unwrap_or_else(|| std::process::abort());
        if route.root() != base || !route.issue_small(offset, class) {
            std::process::abort();
        }
    }

    pub(super) fn remove(&mut self, index: usize) {
        if index >= self.cap {
            std::process::abort();
        }
        // SAFETY: owner-only access to an initialized slot in the live array.
        let slot = unsafe { &mut *self.slots.add(index) };
        if slot.take().is_none() {
            std::process::abort();
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
            .unwrap_or_else(|_| std::process::abort());
        // SAFETY: all handles were dropped; System owns this exact allocation.
        unsafe { System.dealloc(self.slots.cast(), layout) };
    }
}
