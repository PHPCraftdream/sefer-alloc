//! Owner-written class codes in 4 KiB leaves. A class code is stored ONLY in
//! the start granule of an issued block; interior and never-issued granules
//! read 0, so a producer's terminal publication (Acquire, before the
//! pending-bit RMW) drops foreign frees of interior pointers without reading
//! segment state. Producers never write them.
//!
//! # Construction contract
//! The directory allocates the backing with `System.alloc` and constructs it
//! in place with `initialize_at`, which writes the all-zero pattern — the
//! valid initial state (null leaf pointers, zero presence bits).
#![allow(unsafe_code)]
use core::ptr;
use core::ptr::NonNull;
#[cfg(feature = "internals")]
use core::sync::atomic::AtomicUsize;
use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(all(feature = "internals", feature = "bench-internals"))]
use std::cell::Cell;

use super::{AtomicPtr, AtomicU64, AtomicU8, Ordering};
use crate::alloc_core::fatal::fatal;
use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

const LEAF_GRANULES: usize = 256;
const LEAVES: usize = 1024;
const PRESENT_WORDS: usize = LEAVES / u64::BITS as usize;

/// Run-level batch-issue cursor: the leaf a refill batch is currently issuing
/// into, its backing pointer resolved once at the leaf CHANGE (via
/// [`ClassLeaves::prepare_leaf`] or [`ClassLeaves::resolve_leaf`]) and reused
/// for every same-leaf granule afterwards. Private to the batch-issue seam.
#[derive(Clone, Copy)]
pub(crate) struct LeafCursor {
    leaf_idx: usize,
    leaf: NonNull<AtomicU8>,
}

impl LeafCursor {
    /// Leaf index this cursor's backing pointer belongs to.
    pub(crate) fn leaf_idx(&self) -> usize {
        self.leaf_idx
    }

    /// Leaf index of a granule (one leaf covers `LEAF_GRANULES` granules).
    pub(crate) fn leaf_index(granule: usize) -> usize {
        granule / LEAF_GRANULES
    }
}

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
    mixed: [AtomicPtr<AtomicU8>; LEAVES],
    /// Owner-private leaf-presence bits, maintained only by `prepare`
    /// (promotion); producers never read them. A set bit implies the leaf
    /// pointer in `mixed` was Release-published.
    present: [AtomicU64; PRESENT_WORDS],
}

impl ClassLeaves {
    /// # Safety
    /// `raw` is a private, aligned, writable ClassLeaves slot in the
    /// System-backed sidecar. No reference or Drop may access it until return.
    pub(crate) unsafe fn initialize_at(raw: *mut Self) {
        // SAFETY: caller grants exclusive writable storage of Self; field
        // addresses are formed without a reference to uninitialized bytes.
        let mixed = unsafe { ptr::addr_of_mut!((*raw).mixed).cast::<AtomicPtr<AtomicU8>>() };
        for i in 0..LEAVES {
            // SAFETY: each index is within its field and written exactly once
            // before the enclosing route is published.
            unsafe { mixed.add(i).write(AtomicPtr::new(ptr::null_mut())) };
        }
        let present = unsafe { ptr::addr_of_mut!((*raw).present).cast::<AtomicU64>() };
        for i in 0..PRESENT_WORDS {
            // SAFETY: disjoint presence field in the same private storage;
            // written once before the enclosing route is published.
            unsafe { present.add(i).write(AtomicU64::new(0)) };
        }
    }

    pub(crate) fn prepared(&self, granule: usize, class: u8) -> bool {
        if granule >= LEAVES * LEAF_GRANULES || usize::from(class) >= SMALL_CLASS_COUNT {
            return false;
        }
        // A leaf is prepared once the owner's fallible prepare promoted it;
        // per-granule stores overwrite, so no per-class check remains.
        !self.mixed[granule / LEAF_GRANULES]
            .load(Ordering::Acquire)
            .is_null()
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
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        if FAIL_NEXT_SPILLS.try_with(Self::fail_next).unwrap_or(false)
            || FAIL_SPILL_AFTER
                .try_with(Self::fail_countdown)
                .unwrap_or(false)
        {
            return false;
        }
        self.promote(leaf)
    }

    /// Owner-only leaf promotion: rare (once per 256 granules), kept out of
    /// line so the hot `prepare` path stays small.
    #[cold]
    #[inline(never)]
    fn promote(&self, leaf: usize) -> bool {
        let layout = Layout::array::<AtomicU8>(LEAF_GRANULES)
            .unwrap_or_else(|_| fatal(b"leaf_classes: sidecar bitmap layout overflow\n"));
        // SAFETY: the System span has this exact layout and is owner-private
        // until every atomic byte is initialized and its pointer published.
        let fresh = unsafe { System.alloc(layout) }.cast::<AtomicU8>();
        if fresh.is_null() {
            return false;
        }
        #[cfg(feature = "internals")]
        Self::record_alloc(layout.size(), false);
        // SAFETY: the fresh aligned System span holds LEAF_GRANULES bytes; a
        // zero byte is a valid `core::sync::atomic::AtomicU8`, written once
        // before publication.
        #[cfg(not(loom))]
        unsafe {
            core::ptr::write_bytes(fresh, 0, LEAF_GRANULES)
        };
        // Loom atomics are not plain bytes: construct each one.
        #[cfg(loom)]
        for i in 0..LEAF_GRANULES {
            // SAFETY: i is within the fresh aligned System span; each slot is
            // written once before publication and contains a valid atomic.
            unsafe { fresh.add(i).write(AtomicU8::new(0)) };
        }
        self.mixed[leaf].store(fresh, Ordering::Release);
        // Owner-only promotion bookkeeping: set the presence bit only after the
        // leaf pointer is Release-published, so any future reader that sees the
        // bit also sees the fully initialized leaf. Producers never read it;
        // Drop (also owner-only, after the last pin) walks exactly these bits.
        self.present[leaf / 64].fetch_or(1u64 << (leaf % 64), Ordering::Release);
        true
    }

    /// Batch twin of [`Self::prepare`]: same guard order and fault-injection
    /// hook placement, but returns the promoted leaf's cursor so a whole
    /// refill run can issue into it without re-resolving per block. `None` is
    /// the identical spill OOM (`prepare` returning false).
    pub(crate) fn prepare_leaf(&self, granule: usize, class: u8) -> Option<LeafCursor> {
        if granule >= LEAVES * LEAF_GRANULES || usize::from(class) >= SMALL_CLASS_COUNT {
            return None;
        }
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        if FAIL_PREPARE_AFTER
            .try_with(Self::fail_countdown)
            .unwrap_or(false)
        {
            return None;
        }
        if !self.prepared(granule, class) {
            let leaf = granule / LEAF_GRANULES;
            #[cfg(all(feature = "internals", feature = "bench-internals"))]
            if FAIL_NEXT_SPILLS.try_with(Self::fail_next).unwrap_or(false)
                || FAIL_SPILL_AFTER
                    .try_with(Self::fail_countdown)
                    .unwrap_or(false)
            {
                return None;
            }
            if !self.promote(leaf) {
                return None;
            }
        }
        let leaf_idx = granule / LEAF_GRANULES;
        let ptr = self.mixed[leaf_idx].load(Ordering::Acquire);
        // prepare/promote above succeeded, so the leaf pointer is published.
        NonNull::new(ptr).map(|leaf| LeafCursor { leaf_idx, leaf })
    }

    /// Re-resolve a leaf's cursor without promoting; `None` if the leaf was
    /// never promoted (the batch commit trap: commit never blindly stores
    /// through a stale or absent backing).
    pub(crate) fn resolve_leaf(&self, granule: usize) -> Option<LeafCursor> {
        if granule >= LEAVES * LEAF_GRANULES {
            return None;
        }
        let leaf_idx = granule / LEAF_GRANULES;
        NonNull::new(self.mixed[leaf_idx].load(Ordering::Acquire))
            .map(|leaf| LeafCursor { leaf_idx, leaf })
    }

    /// Per-block batch issue: one Release store, no promotion and no prepared
    /// re-check (that is the point of the run hoist).
    pub(crate) fn issue_in(&self, cursor: &LeafCursor, granule: usize, class: u8) {
        debug_assert_eq!(cursor.leaf_idx(), LeafCursor::leaf_index(granule));
        debug_assert!(usize::from(class) < SMALL_CLASS_COUNT);
        debug_assert!(granule < LEAVES * LEAF_GRANULES);
        // SAFETY: the cursor's leaf pointer was Release-published by the
        // owner's promotion and stays owned through the route's last pin,
        // exactly like `issue()`'s deref; the run holds `&self`, so the leaf
        // cannot be dropped mid-run.
        unsafe { &*cursor.leaf.as_ptr().add(granule % LEAF_GRANULES) }
            .store(class + 1, Ordering::Release);
    }

    pub(crate) fn issue(&self, granule: usize, class: u8) -> bool {
        if granule >= LEAVES * LEAF_GRANULES || usize::from(class) >= SMALL_CLASS_COUNT {
            return false;
        }
        let leaf = granule / LEAF_GRANULES;
        let mixed = self.mixed[leaf].load(Ordering::Acquire);
        if mixed.is_null() {
            // Commit-time issue never promotes: the owner's fallible prepare
            // must have spilled this leaf first (the commit guard aborts
            // otherwise, so this is unreachable on protocol-correct paths).
            return false;
        }
        // SAFETY: the owner published a fully initialized 256-atomic
        // System leaf, retained until the final route pin drops.
        unsafe { &*mixed.add(granule % LEAF_GRANULES) }.store(class + 1, Ordering::Release);
        true
    }

    pub(crate) fn encoded(&self, granule: usize) -> u8 {
        let leaf = granule / LEAF_GRANULES;
        let mixed = self.mixed[leaf].load(Ordering::Acquire);
        if mixed.is_null() {
            // A never-promoted leaf has no issued start granule.
            0
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

    /// Per-block prepare fault point for batch runs: a same-leaf block skips
    /// `prepare_leaf`, so the run consumes the `fail_prepare_after` countdown
    /// here to keep the injected refusal on the same block as the scalar path.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(crate) fn fail_prepare_injected() -> bool {
        FAIL_PREPARE_AFTER
            .try_with(Self::fail_countdown)
            .unwrap_or(false)
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
        let layout = Layout::array::<AtomicU8>(LEAF_GRANULES)
            .unwrap_or_else(|_| fatal(b"leaf_classes: sidecar bitmap layout overflow\n"));
        for (word, bits) in self.present.iter().enumerate() {
            let mut bits = bits.load(Ordering::Relaxed);
            while bits != 0 {
                let leaf = word * 64 + bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let ptr = self.mixed[leaf].load(Ordering::Relaxed);
                debug_assert!(
                    !ptr.is_null(),
                    "present bit without a leaf pointer: leaf {leaf}"
                );
                #[cfg(feature = "internals")]
                Self::record_free(layout.size());
                // SAFETY: promotion published a genuine System allocation with
                // this layout for every presence bit; Drop runs after unlink,
                // so no scanner or pin still references the leaf.
                unsafe { System.dealloc(ptr.cast(), layout) };
            }
        }
        #[cfg(debug_assertions)]
        for (leaf, slot) in self.mixed.iter().enumerate() {
            let present =
                self.present[leaf / 64].load(Ordering::Relaxed) & (1u64 << (leaf % 64)) != 0;
            debug_assert_eq!(
                present,
                !slot.load(Ordering::Relaxed).is_null(),
                "leaf presence invariant broken: leaf {leaf}"
            );
        }
    }
}
