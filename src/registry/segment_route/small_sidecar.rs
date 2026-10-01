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
}

impl SmallSidecar {
    pub const WORDS: usize = SidecarBitmap::WORDS;

    /// # Safety
    /// `raw` is a private, aligned, writable System allocation with this
    /// exact layout. It may not be observed or dropped before return.
    pub(crate) unsafe fn initialize_at(raw: *mut Self) {
        // SAFETY: caller grants exclusive writable SmallSidecar storage;
        // this raw field projection makes no reference to uninitialized data.
        let pending = unsafe { ptr::addr_of_mut!((*raw).pending).cast::<AtomicU64>() };
        for i in 0..SidecarBitmap::WORDS {
            // SAFETY: i is within the pending field and each atomic word is
            // initialized once before directory publication.
            unsafe { pending.add(i).write(AtomicU64::new(0)) };
        }
        // SAFETY: disjoint classes field in the same private allocation;
        // pending is initialized and publication is still excluded.
        unsafe { ClassLeaves::initialize_at(ptr::addr_of_mut!((*raw).classes)) };
    }

    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_leaves(&self.pending, &self.classes)
            .unwrap_or_else(|| std::process::abort())
    }

    /// Owner-only, fallible preparation before any issue transaction mutates
    /// its freelist, bitmap, bump, live credit, directory, or output.
    pub fn prepare(&self, offset: u32, class: u8) -> bool {
        let offset = offset as usize;
        let granule = crate::alloc_core::size_classes::MIN_BLOCK;
        offset < crate::alloc_core::os::SEGMENT
            && offset.is_multiple_of(granule)
            && self.classes.prepare(offset / granule, class)
    }

    /// Owner-only class issue before allocation handoff.
    pub fn issue(&self, offset: u32, class: u8) -> bool {
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

    pub(crate) fn scan_from(&self, high_water: usize, word: usize) -> Option<RouteScan<'_>> {
        self.bitmap()
            .scan_from(high_water, word)
            .map(|inner| RouteScan { inner })
    }
}
