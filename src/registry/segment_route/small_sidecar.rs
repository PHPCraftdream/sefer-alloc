//! Owning backing for the single authoritative SidecarBitmap primitive.
#![allow(unsafe_code)]

use core::ptr;
use core::sync::atomic::AtomicU64;
#[cfg(feature = "internals")]
use core::sync::atomic::Ordering;

use crate::alloc_core::remote_bitmap::{ClassLeaves, SidecarBitmap};

use super::RouteScan;

#[repr(C)]
pub struct SmallSidecar {
    pending: [AtomicU64; SidecarBitmap::WORDS],
    classes: ClassLeaves,
    summary: [AtomicU64; SidecarBitmap::SUMMARY_WORDS],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<SmallSidecar>() == 41_984 + 512);

impl SmallSidecar {
    /// Process-wide RouteSlots System totals: (arrays allocated, arrays freed,
    /// bytes allocated, bytes freed). Read at quiescent checkpoints; separate
    /// relaxed loads are not a coherent concurrent snapshot. No counter reset.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_route_slots_system_totals() -> (usize, usize, usize, usize) {
        crate::alloc_core::segment_table::SegmentTable::route_slots_system_totals()
    }

    /// Actual Option<RouteRegistration> layout, not a fixed ABI/RSS estimate.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_route_slot_bytes() -> usize {
        crate::alloc_core::segment_table::SegmentTable::route_slot_bytes()
    }

    pub const WORDS: usize = SidecarBitmap::WORDS;

    /// # Safety
    /// `raw` is a private, aligned, writable System allocation with this
    /// exact layout. It may not be observed or dropped before return.
    /// OPT-04: the pending and summary words are NOT rewritten here. The ONLY
    /// creation path is `directory.rs` `EntryHandle::new`, which obtains the
    /// storage from `System.alloc_zeroed` and never recycles a previous
    /// sidecar (`free_sidecar` always `System.dealloc`s), so every pending
    /// and summary byte is already zero; writing ~4160 `AtomicU64::new(0)`
    /// values over it repeated provably redundant stores per registration.
    /// Debug builds still verify the zeroed backing, so replacing
    /// `alloc_zeroed` with `alloc` (or adding a recycle path without a fresh
    /// `alloc_zeroed`) fails loudly instead of publishing garbage bits.
    pub(crate) unsafe fn initialize_at(raw: *mut Self) {
        #[cfg(debug_assertions)]
        // SAFETY: caller grants exclusive writable SmallSidecar storage.
        unsafe {
            Self::assert_zeroed_backing(raw)
        };
        // SAFETY: disjoint classes field in the same private allocation;
        // publication is still excluded (the class map is genuinely
        // uninitialized storage, unlike the zeroed pending/summary words).
        unsafe { ClassLeaves::initialize_at(ptr::addr_of_mut!((*raw).classes)) };
    }

    #[cfg(debug_assertions)]
    unsafe fn assert_zeroed_backing(raw: *mut Self) {
        // SAFETY: exclusive writable storage with SmallSidecar's exact layout;
        // each projection stays inside the allocation.
        let pending = unsafe { ptr::addr_of_mut!((*raw).pending).cast::<AtomicU64>() };
        for i in 0..SidecarBitmap::WORDS {
            // SAFETY: i is within the pending field; the memory is allocated.
            let word = unsafe { (*pending.add(i)).load(core::sync::atomic::Ordering::Relaxed) };
            assert!(word == 0, "SmallSidecar backing must be zero-initialized");
        }
        // SAFETY: disjoint summary field in the same allocation.
        let summary = unsafe { ptr::addr_of_mut!((*raw).summary).cast::<AtomicU64>() };
        for i in 0..SidecarBitmap::SUMMARY_WORDS {
            // SAFETY: i is within the summary field.
            let word = unsafe { (*summary.add(i)).load(core::sync::atomic::Ordering::Relaxed) };
            assert!(word == 0, "SmallSidecar backing must be zero-initialized");
        }
    }

    /// Counterfactual hook for the OPT-04 zeroed-backing contract: exposes the
    /// private in-place construction so a test can poison the storage first
    /// and require the debug verification to reject it.
    ///
    /// # Safety
    /// Same contract as [`Self::initialize_at`]: `raw` is private, aligned,
    /// writable storage with this exact layout, not observed elsewhere.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub unsafe fn dbg_initialize_at_for_test(raw: *mut Self) {
        // SAFETY: identical exclusive-storage contract.
        unsafe { Self::initialize_at(raw) };
    }

    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_leaves(&self.pending, &self.summary, &self.classes)
            .unwrap_or_else(|| std::process::abort())
    }

    /// Crate-private owner-only, fallible preparation before any issue
    /// transaction mutates its freelist, bitmap, bump, live credit, directory,
    /// or output; public owner mutation is carried only by
    /// `RouteRegistration::prepare_small` (which is `!Sync` via its
    /// `PhantomData<Cell<()>>` owner marker, so exclusive-owner sharing across
    /// threads stays type-enforced).
    pub(crate) fn prepare(&self, offset: u32, class: u8) -> bool {
        let offset = offset as usize;
        let granule = crate::alloc_core::size_classes::MIN_BLOCK;
        offset < crate::alloc_core::os::SEGMENT
            && offset.is_multiple_of(granule)
            && self.classes.prepare(offset / granule, class)
    }

    /// Crate-private owner-only class issue before allocation handoff; public
    /// owner mutation is carried only by `RouteRegistration::issue_small` (the
    /// `!Sync` `PhantomData<Cell<()>>` owner marker keeps exclusive-owner
    /// sharing across threads type-enforced).
    pub(crate) fn issue(&self, offset: u32, class: u8) -> bool {
        (self.prepared(offset, class) || self.prepare(offset, class))
            && self.bitmap().issue(offset, class)
    }

    pub(crate) fn prepared(&self, offset: u32, class: u8) -> bool {
        let offset = offset as usize;
        let granule = crate::alloc_core::size_classes::MIN_BLOCK;
        offset < crate::alloc_core::os::SEGMENT
            && offset.is_multiple_of(granule)
            && self.classes.prepared(offset / granule, class)
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn class_at_for_test(&self, offset: u32) -> Option<u8> {
        let offset = offset as usize;
        let granule = crate::alloc_core::size_classes::MIN_BLOCK;
        if offset >= crate::alloc_core::os::SEGMENT || !offset.is_multiple_of(granule) {
            return None;
        }
        self.classes.encoded(offset / granule).checked_sub(1)
    }
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn mixed_leaves_for_test(&self) -> usize {
        self.classes.mixed_count()
    }
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn fail_spill_after_for_test(count: usize) {
        ClassLeaves::fail_spill_after(count);
    }
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn fail_next_spills_for_test(count: usize) {
        ClassLeaves::fail_next_spills(count);
    }
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn spill_failures_remaining_for_test() -> usize {
        ClassLeaves::spill_failures_remaining()
    }
    /// Owner metadata read through the directory's original reservation root.
    ///
    /// # Safety
    /// The caller exclusively owns the heap containing this live issued
    /// allocation throughout the read. A descriptor pin alone is insufficient.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub unsafe fn owner_state_for_test(address: usize, class: usize) -> Option<(usize, u32, u32)> {
        if class >= crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
            return None;
        }
        let pin =
            super::RouteDirectory::global().lookup(ptr::without_provenance_mut::<u8>(address))?;
        pin.entry.small_sidecar()?;
        // SAFETY: caller owns the live reservation; the root comes from the
        // counted descriptor, while address supplied only the lookup key.
        let meta = crate::alloc_core::segment_header::SegmentMeta::new(pin.entry.root());
        Some((
            meta.bump_of(),
            meta.live_count_of(),
            meta.bin_table().head(class),
        ))
    }
    /// Transaction fault injection; distinct from a required System spill.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn fail_prepare_after_for_test(count: usize) {
        ClassLeaves::fail_prepare_after(count);
    }
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    /// Requested System bytes: (alloc, zeroed, free, alloc calls, free calls).
    /// Excludes allocator overhead, RSS and the reservation itself.
    pub fn system_totals_for_test() -> (usize, usize, usize, usize, usize) {
        ClassLeaves::system_totals()
    }
    #[cfg(feature = "internals")]
    pub(super) fn pending_for_test(&self, offset: u32) -> bool {
        let bit = offset as usize / crate::alloc_core::size_classes::MIN_BLOCK;
        bit < SidecarBitmap::GRANULES
            && self.pending[bit / 64].load(Ordering::Acquire) & (1u64 << (bit % 64)) != 0
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(super) fn pause_publish(&self, offset: u32) -> Option<(&AtomicU64, u64, bool)> {
        self.bitmap().pause_publish(offset)
    }

    /// Remaining actual pending half for the reversed-order test counterfactual.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(super) fn resume_pending_for_test(&self, offset: u32) -> bool {
        self.bitmap().resume_pending_for_test(offset)
    }

    /// Actual discovery scanner, exposed only for deterministic owner tests.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn scan_summarized_for_test(&self, high_water: usize) -> Option<RouteScan<'_>> {
        self.scan_summarized_from(high_water, 0)
    }

    /// Producer terminal publication, called by consuming RoutePin.
    pub(super) fn publish(&self, offset: u32) -> bool {
        self.bitmap().publish(offset)
    }

    /// Owner-only bounded per-word cut through the existing bitmap scanner.
    pub fn scan(&self, high_water: usize) -> Option<RouteScan<'_>> {
        self.bitmap()
            .scan(high_water)
            .map(|inner| RouteScan { inner })
    }

    pub(crate) fn scan_summarized_from(
        &self,
        high_water: usize,
        word: usize,
    ) -> Option<RouteScan<'_>> {
        self.bitmap()
            .scan_summarized_from(high_water, word)
            .map(|inner| RouteScan { inner })
    }

    pub(crate) fn scan_from(&self, high_water: usize, word: usize) -> Option<RouteScan<'_>> {
        self.bitmap()
            .scan_from(high_water, word)
            .map(|inner| RouteScan { inner })
    }
}
