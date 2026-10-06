//! Owner-written class codes in 4 KiB leaves. A producer's terminal
//! publication READS them (Acquire, before the pending-bit RMW) to drop a
//! never-issued granule; producers never write them.
#![allow(unsafe_code)]

use core::ptr;
#[cfg(feature = "internals")]
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(all(feature = "internals", feature = "bench-internals"))]
use std::cell::Cell;

use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

const LEAF_GRANULES: usize = 256;
const LEAVES: usize = 1024;

#[cfg(all(feature = "internals", feature = "bench-internals"))]
std::thread_local! {
    static FAIL_SPILL_AFTER: Cell<usize> = const { Cell::new(0) };
    static FAIL_NEXT_SPILLS: Cell<usize> = const { Cell::new(0) };
    static FAIL_PREPARE_AFTER: Cell<usize> = const { Cell::new(0) };
}

#[cfg(feature = "internals")]
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "internals")]
static ZEROED_BYTES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "internals")]
static FREE_BYTES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "internals")]
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "internals")]
static FREE_COUNT: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct ClassLeaves {
    uniform: [AtomicU8; LEAVES],
    mixed: [AtomicPtr<AtomicU8>; LEAVES],
}

impl ClassLeaves {
    /// # Safety
    /// `raw` is a private, aligned, writable ClassLeaves slot in the
    /// System-backed sidecar. No reference or Drop may access it until return.
    pub(crate) unsafe fn initialize_at(raw: *mut Self) {
        // SAFETY: caller grants exclusive writable storage of Self; field
        // addresses are formed without a reference to uninitialized bytes.
        let uniform = unsafe { ptr::addr_of_mut!((*raw).uniform).cast::<AtomicU8>() };
        // SAFETY: same exclusive Self storage and non-overlapping field.
        let mixed = unsafe { ptr::addr_of_mut!((*raw).mixed).cast::<AtomicPtr<AtomicU8>>() };
        for i in 0..LEAVES {
            // SAFETY: each index is within its field and written exactly once
            // before the enclosing route is published.
            unsafe {
                uniform.add(i).write(AtomicU8::new(0));
                mixed.add(i).write(AtomicPtr::new(ptr::null_mut()));
            }
        }
    }

    pub(crate) fn prepared(&self, granule: usize, class: u8) -> bool {
        if granule >= LEAVES * LEAF_GRANULES || usize::from(class) >= SMALL_CLASS_COUNT {
            return false;
        }
        let leaf = granule / LEAF_GRANULES;
        if !self.mixed[leaf].load(Ordering::Acquire).is_null() {
            return true;
        }
        let old = self.uniform[leaf].load(Ordering::Acquire);
        old == 0 || old == class + 1
    }

    pub(crate) fn prepare(&self, granule: usize, class: u8) -> bool {
        if granule >= LEAVES * LEAF_GRANULES || usize::from(class) >= SMALL_CLASS_COUNT {
            return false;
        }
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        if FAIL_PREPARE_AFTER
            .try_with(Self::fail_countdown)
            .unwrap_or(false)
        {
            return false;
        }
        if self.prepared(granule, class) {
            return true;
        }
        let leaf = granule / LEAF_GRANULES;
        let old = self.uniform[leaf].load(Ordering::Acquire);
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        if FAIL_NEXT_SPILLS.try_with(Self::fail_next).unwrap_or(false)
            || FAIL_SPILL_AFTER
                .try_with(Self::fail_countdown)
                .unwrap_or(false)
        {
            return false;
        }
        let layout =
            Layout::array::<AtomicU8>(LEAF_GRANULES).unwrap_or_else(|_| std::process::abort());
        // SAFETY: the System span has this exact layout and is owner-private
        // until every atomic byte is initialized and its pointer published.
        let fresh = unsafe { System.alloc(layout) }.cast::<AtomicU8>();
        if fresh.is_null() {
            return false;
        }
        #[cfg(feature = "internals")]
        Self::record_alloc(layout.size(), false);
        for i in 0..LEAF_GRANULES {
            // SAFETY: i is within the fresh aligned System span; each slot is
            // written once before publication and contains a valid atomic.
            unsafe { fresh.add(i).write(AtomicU8::new(old)) };
        }
        self.mixed[leaf].store(fresh, Ordering::Release);
        true
    }

    pub(crate) fn issue(&self, granule: usize, class: u8) -> bool {
        if granule >= LEAVES * LEAF_GRANULES || usize::from(class) >= SMALL_CLASS_COUNT {
            return false;
        }
        let leaf = granule / LEAF_GRANULES;
        let mixed = self.mixed[leaf].load(Ordering::Acquire);
        if !mixed.is_null() {
            // SAFETY: the owner published a fully initialized 256-atomic
            // System leaf, retained until the final route pin drops.
            unsafe { &*mixed.add(granule % LEAF_GRANULES) }.store(class + 1, Ordering::Release);
            return true;
        }
        let uniform = self.uniform[leaf].load(Ordering::Acquire);
        if uniform == 0 {
            self.uniform[leaf].store(class + 1, Ordering::Release);
            return true;
        }
        uniform == class + 1
    }

    pub(crate) fn encoded(&self, granule: usize) -> u8 {
        let leaf = granule / LEAF_GRANULES;
        let mixed = self.mixed[leaf].load(Ordering::Acquire);
        if mixed.is_null() {
            self.uniform[leaf].load(Ordering::Acquire)
        } else {
            // SAFETY: the mixed leaf was initialized before its Release
            // publication and remains owned through this route's last pin.
            unsafe { &*mixed.add(granule % LEAF_GRANULES) }.load(Ordering::Acquire)
        }
    }

    #[cfg(feature = "internals")]
    pub(crate) fn mixed_count(&self) -> usize {
        self.mixed
            .iter()
            .filter(|p| !p.load(Ordering::Acquire).is_null())
            .count()
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(crate) fn fail_spill_after(count: usize) {
        let _ = FAIL_NEXT_SPILLS.try_with(|n| n.set(0));
        let _ = FAIL_SPILL_AFTER.try_with(|n| n.set(count));
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(crate) fn fail_next_spills(count: usize) {
        let _ = FAIL_SPILL_AFTER.try_with(|n| n.set(0));
        let _ = FAIL_NEXT_SPILLS.try_with(|n| n.set(count));
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(crate) fn spill_failures_remaining() -> usize {
        FAIL_NEXT_SPILLS.try_with(Cell::get).unwrap_or(0)
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(crate) fn fail_prepare_after(count: usize) {
        let _ = FAIL_PREPARE_AFTER.try_with(|n| n.set(count));
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    fn fail_countdown(n: &Cell<usize>) -> bool {
        let left = n.get();
        if left != 0 {
            n.set(left - 1);
        }
        left == 1
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    fn fail_next(n: &Cell<usize>) -> bool {
        let left = n.get();
        if left != 0 {
            n.set(left - 1);
        }
        left != 0
    }

    #[cfg(feature = "internals")]
    pub(crate) fn record_alloc(bytes: usize, zeroed: bool) {
        ALLOC_BYTES.fetch_add(bytes, Ordering::Relaxed);
        if zeroed {
            ZEROED_BYTES.fetch_add(bytes, Ordering::Relaxed);
        }
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(feature = "internals")]
    pub(crate) fn record_free(bytes: usize) {
        FREE_BYTES.fetch_add(bytes, Ordering::Relaxed);
        FREE_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(feature = "internals")]
    pub(crate) fn system_totals() -> (usize, usize, usize, usize, usize) {
        (
            ALLOC_BYTES.load(Ordering::Relaxed),
            ZEROED_BYTES.load(Ordering::Relaxed),
            FREE_BYTES.load(Ordering::Relaxed),
            ALLOC_COUNT.load(Ordering::Relaxed),
            FREE_COUNT.load(Ordering::Relaxed),
        )
    }
}

impl Drop for ClassLeaves {
    fn drop(&mut self) {
        let layout =
            Layout::array::<AtomicU8>(LEAF_GRANULES).unwrap_or_else(|_| std::process::abort());
        for slot in &self.mixed {
            let p = slot.load(Ordering::Relaxed);
            if !p.is_null() {
                #[cfg(feature = "internals")]
                Self::record_free(layout.size());
                // SAFETY: each pointer is the genuine System allocation
                // published once by this route's owner, with this layout;
                // last-pin Drop is after unlink, so no scanner remains.
                unsafe { System.dealloc(p.cast(), layout) };
            }
        }
    }
}
