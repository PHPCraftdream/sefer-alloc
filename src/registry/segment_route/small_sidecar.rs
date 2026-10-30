//! Owning backing for the single authoritative SidecarBitmap primitive.
#![allow(unsafe_code)]

use super::AtomicU64;
#[cfg(feature = "internals")]
use super::Ordering;
use core::ptr;

use crate::alloc_core::fatal::fatal;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::remote_bitmap::{ClassLeaves, SidecarBitmap};
use crate::alloc_core::size_classes::{MIN_BLOCK, SMALL_CLASS_COUNT};

use super::RouteScan;

// Production-geometry pins for the sidecar primitive, moved verbatim from
// sidecar_bitmap.rs so the actual-source Loom harness can include it with a
// reduced SEGMENT (tests/loom_sidecar_bitmap_actual.rs).
const _: () = {
    assert!(SEGMENT == 4 * 1024 * 1024);
    assert!(MIN_BLOCK == 16);
    assert!(SidecarBitmap::WORDS == 4096);
    assert!(SidecarBitmap::GRANULES == 262_144);
    assert!(SMALL_CLASS_COUNT < u8::MAX as usize);
};

/// `#[repr(C)]` external ingress backing: pending words, then the adaptive
/// class leaves. The directory allocates it with `System.alloc` and
/// constructs it in place with `initialize_at`, which writes the all-zero
/// pattern — the valid initial state (null leaf pointers, zero pending words:
/// `SidecarBitmap::publish` rejects a zero encoded class — and zero presence
/// bits).
#[repr(C)]
pub struct SmallSidecar {
    pending: [AtomicU64; SidecarBitmap::WORDS],
    classes: ClassLeaves,
    /// SC-6 oracle: owner epoch at this route's last counted notification.
    #[cfg(all(g1_ingress_oracle, feature = "bench-internals"))]
    oracle_seen: AtomicU64,
}

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

    // R6-T16: no in-src reader (the struct field uses SidecarBitmap::WORDS);
    // consumed by internals builds and source-included oracle crates only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
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
        // SAFETY: disjoint oracle field in the same private allocation.
        #[cfg(all(g1_ingress_oracle, feature = "bench-internals"))]
        unsafe {
            ptr::addr_of_mut!((*raw).oracle_seen).write(AtomicU64::new(u64::MAX))
        };
    }

    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_leaves(&self.pending, &self.classes)
            .unwrap_or_else(|| fatal(b"small_sidecar: sidecar bitmap leaf layout mismatch\n"))
    }

    /// Owner-only batch-issue seam over the class leaves; producers never
    /// touch it.
    pub(crate) fn classes(&self) -> &ClassLeaves {
        &self.classes
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

    /// Producer terminal publication, called by consuming RoutePin.
    pub(super) fn publish(&self, offset: u32) -> bool {
        self.bitmap().publish(offset)
    }

    /// SC-6 oracle: count a successful publication, and a notification when
    /// the owner's refill-miss epoch moved since this route last notified.
    #[cfg(all(g1_ingress_oracle, feature = "bench-internals"))]
    pub(super) fn oracle_after_publish(&self, owner: usize) {
        use core::sync::atomic::Ordering::Relaxed;
        crate::alloc_core::G1_PUBLISHES.fetch_add(1, Relaxed);
        let epoch = crate::alloc_core::G1_OWNER_EPOCH[owner & 1023].load(Relaxed);
        if self.oracle_seen.swap(epoch, Relaxed) != epoch {
            crate::alloc_core::G1_NOTIFIES.fetch_add(1, Relaxed);
        }
    }

    /// Owner-only bounded per-word cut through the existing bitmap scanner.
    // R6-T16: internals-test/oracle surface only (production drains use
    // scan_from); suppressed exactly outside internals.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
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
