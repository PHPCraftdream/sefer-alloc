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

use super::small_sidecar_backing::SmallSidecarBacking;
use super::RouteScan;
use core::sync::atomic::Ordering as CoreOrdering;
use std::alloc::{GlobalAlloc, Layout, System};

// Task #223 path-activation oracle: process-lifetime backing counters
// (Relaxed diagnostics; no protocol reads them). `VMEM_*` count sidecars
// taken through the OS-zeroed-span path (no userspace zero-fill); `HEAP_*`
// count the `System` fallback that constructs every atomic in place (the
// historical full zero-fill). A registration on the new path bumps the VMEM
// pair and leaves the HEAP pair — and the ClassLeaves System byte totals —
// untouched; see `sidecar_backing_counts_for_test`.
//
// Release-side semantics (task #223 review round 3): the counters are bumped
// by `release_taken_backing` — the owner-side release that runs AFTER
// `drop_in_place::<SmallSidecar>` has returned, when no reference into the
// storage is live any more.
//
// Task #223 counterfactual oracle for the zero-fill itself: a relaxed count
// of u64-granule payload words explicitly zero-filled in USERSPACE at
// sidecar construction. Bumped at exactly two sites, and the fill and its
// accounting are ONE operation each: (1) the interpreter build's cfg-gated
// helper `explicit_zero_fill_words` (called from the single `allocate`
// VM-span site; counts `size_of::<Self>() / 8` — that build's VM backend is
// System-backed and does not hand back zeroed bytes), and (2)
// `initialize_at`'s per-word pending construction (counts
// `SidecarBitmap::WORDS` — the `System` fallback builds every atomic in
// place). A registration on the OS-zeroed-span path bumps NOTHING here on
// every native target, so a delta of
// exactly 0 proves no userspace zero-fill ran; reintroducing a full
// `write_bytes` zero-fill on the VM path would make the delta nonzero and
// redden `tests/p223_small_sidecar_zero_free_backing.rs` (byte-content
// assertions alone cannot distinguish "skipped because OS-zeroed" from
// "zeroed redundantly"). Relaxed diagnostics; no protocol reads them.
// R6-T16: written by build-cfg-gated sites, read by the internals accessor;
// suppressed exactly where the accessor is absent.
#[cfg_attr(not(feature = "internals"), allow(dead_code))]
static EXPLICIT_SIDECAR_FILL_WORDS: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
static VMEM_BACKED_SIDECARS: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
static VMEM_SIDECAR_RELEASES: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
static HEAP_SIDECARS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static HEAP_SIDECAR_RELEASES: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

// The OS-zeroed VM backing is available on every native target; builds
// whose atomics must be constructed explicitly opt out with a cfg-gated
// definition of this constant.
const VMEM_BACKING_AVAILABLE: bool = true;

std::thread_local! {
    /// Test-only countdown forcing the next N sidecar allocations onto the
    /// `System` fallback (see `fail_vmem_reserve_for_test`); mirrors
    /// `route_directory.rs`'s `FAIL_NEXT_REGISTRATION`. Always compiled (a
    /// `Cell`, not an atomic) so `allocate` needs no cfg matrix.
    static FAIL_VMEM_SIDECAR_RESERVE: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

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

/// `#[repr(C)]` external ingress backing: backing tag, pending words, then
/// the adaptive class leaves. Task #223: the default path reserves an
/// OS-zeroed VM span (`SmallSidecarBacking::Vmem`), so the all-zero valid
/// initial state comes from the OS — no userspace zero-fill runs, and the
/// backing tag is the only field written at registration. The `System`
/// fallback (reservation OOM, or model-checker builds whose atomics must be
/// constructed in place) uses `initialize_at`, which writes the all-zero
/// pattern field by field (null leaf pointers, zero pending words:
/// `SidecarBitmap::publish` rejects a zero encoded class — and zero
/// presence bits). BOTH fields that need release-time action (`backing` and
/// `classes`) are `ManuallyDrop`:
/// - `Drop for SmallSidecar` drops ONLY the payload (`classes`) while the
///   storage is still live; it never touches the backing storage.
/// - The owner (`EntryHandle::free_sidecar`) first MOVES the backing tag out
///   via `take_backing`, then runs `drop_in_place`, and only after that
///   returns releases the storage via `release_taken_backing` (span munmap
///   or `System.dealloc`). No code path unmaps/deallocates while any
///   reference into the storage is live.
///
/// Both fields being `ManuallyDrop` means NO automatic field drop glue runs
/// after `Drop`'s body (automatic glue for `backing` would read the enum
/// discriminant out of already-released storage).
#[repr(C)]
pub struct SmallSidecar {
    /// First field: written at registration, so only the page holding it is
    /// touched before first use (the historical path memset-dirtied the
    /// whole struct). Never valid at all-zero; always written explicitly.
    /// `ManuallyDrop` so the backing tag survives `Drop for SmallSidecar`:
    /// the owner moves it OUT via `take_backing` BEFORE `drop_in_place` runs
    /// and releases the storage via `release_taken_backing` only AFTER the
    /// payload drop returned — so no release happens while a reference into
    /// the storage is live. After the body no automatic glue runs (an
    /// automatic `backing` drop would read the enum discriminant from
    /// released memory; `System` carries no payload, so nothing is left
    /// unreleased by the tag never being dropped as a value).
    backing: core::mem::ManuallyDrop<SmallSidecarBacking>,
    pending: [AtomicU64; SidecarBitmap::WORDS],
    /// `ManuallyDrop` so `Drop for SmallSidecar` releases the promoted-leaf
    /// chunks while the backing storage is still mapped/allocated (the
    /// compiler's automatic field drop would run `classes`' Drop after this
    /// struct's own `Drop` body — after the owner-side storage release —
    /// reading released memory).
    classes: core::mem::ManuallyDrop<ClassLeaves>,
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

    /// Private storage for one sidecar: OS-zeroed VM span when available,
    /// `System` heap fallback otherwise (model-checker builds, reservation
    /// OOM).
    /// Null means OOM (both backends refused) — the caller keeps the
    /// historical `RouteError::OutOfMemory` behavior.
    pub(crate) fn allocate() -> *mut u8 {
        const { assert!(core::mem::align_of::<Self>() <= aligned_vmem::PAGE) };
        if VMEM_BACKING_AVAILABLE
            && FAIL_VMEM_SIDECAR_RESERVE
                .try_with(|left| {
                    let remaining = left.get();
                    left.set(remaining.saturating_sub(1));
                    remaining == 0
                })
                .unwrap_or(true)
        {
            if let Some(vm) = SmallSidecarBacking::reserve_zeroed_span(core::mem::size_of::<Self>())
            {
                let raw = vm.as_ptr().cast::<Self>();
                // SAFETY: single documented reason: `raw` is a private,
                // page-aligned, OS-zeroed (or, in the interpreter build,
                // explicitly zeroed) span with this
                // exact layout (`ManuallyDrop` is layout-transparent), and
                // the backing tag is the one field whose all-zero bytes are
                // not a valid value; nothing observes or drops the span
                // before this write.
                unsafe {
                    ptr::addr_of_mut!((*raw).backing)
                        .write(core::mem::ManuallyDrop::new(SmallSidecarBacking::Vmem(vm)))
                };
                VMEM_BACKED_SIDECARS.fetch_add(1, CoreOrdering::Relaxed);
                return raw.cast();
            }
        }
        // SAFETY: System is independent of the installed allocator; the
        // span stays private until `initialize_at` builds every atomic.
        let ptr = unsafe { System.alloc(Layout::new::<Self>()) }.cast::<Self>();
        if !ptr.is_null() {
            #[cfg(feature = "internals")]
            ClassLeaves::record_alloc(core::mem::size_of::<Self>(), false);
            // SAFETY: fresh private storage has SmallSidecar's exact
            // layout; in-place construction initializes every atomic and
            // the backing tag.
            unsafe { Self::initialize_at(ptr) };
            HEAP_SIDECARS.fetch_add(1, CoreOrdering::Relaxed);
        }
        ptr.cast()
    }

    /// # Safety
    /// `raw` is a private, aligned, writable storage span with this exact
    /// layout (the `System` fallback allocation). It may not be observed or
    /// dropped before return.
    pub(crate) unsafe fn initialize_at(raw: *mut Self) {
        // SAFETY: caller grants exclusive writable SmallSidecar storage;
        // this raw field projection makes no reference to uninitialized
        // data. The backing tag is never valid at all-zero and is written
        // explicitly first (`ManuallyDrop` is layout-transparent; the
        // wrapper is dropped manually by `SmallSidecar::drop`).
        unsafe {
            ptr::addr_of_mut!((*raw).backing)
                .write(core::mem::ManuallyDrop::new(SmallSidecarBacking::System))
        };
        // SAFETY: caller grants exclusive writable SmallSidecar storage;
        // this raw field projection makes no reference to uninitialized data.
        let pending = unsafe { ptr::addr_of_mut!((*raw).pending).cast::<AtomicU64>() };
        for i in 0..SidecarBitmap::WORDS {
            // SAFETY: i is within the pending field and each atomic word is
            // initialized once before directory publication.
            unsafe { pending.add(i).write(AtomicU64::new(0)) };
        }
        EXPLICIT_SIDECAR_FILL_WORDS.fetch_add(SidecarBitmap::WORDS, CoreOrdering::Relaxed);
        // SAFETY: disjoint classes field in the same private storage;
        // pending is initialized and publication is still excluded. The
        // field is `ManuallyDrop<ClassLeaves>` (same layout, dropped
        // manually by `SmallSidecar::drop` before the backing release).
        unsafe {
            ClassLeaves::initialize_at(ptr::addr_of_mut!((*raw).classes).cast::<ClassLeaves>())
        };
    }

    /// Owner-side backing extraction: moves the `ManuallyDrop<SmallSidecarBacking>`
    /// tag OUT of the sidecar BEFORE `drop_in_place` runs, so `Drop for
    /// SmallSidecar` (which drops only the payload) never releases storage
    /// while a `&mut` into it is live, and `release_taken_backing` runs only
    /// after that drop returned.
    ///
    /// # Safety
    /// `raw` points to a fully initialized `SmallSidecar` whose backing tag
    /// was written exactly once, is not yet released, and is extracted
    /// exactly once (last-reference release exclusion per the P5 funnel).
    pub(crate) unsafe fn take_backing(raw: *mut Self) -> SmallSidecarBacking {
        // SAFETY: single documented reason: per the contract above, `raw` is
        // a valid, exclusively owned, fully initialized sidecar whose
        // `backing` field is initialized exactly once and moved out exactly
        // once here, before the payload drop runs; after this move the field
        // bytes are never read as a value again.
        unsafe { core::mem::ManuallyDrop::take(&mut *ptr::addr_of_mut!((*raw).backing)) }
    }

    /// Owner-side backing release; called ONLY after `drop_in_place` for the
    /// same sidecar has RETURNED, so no reference into the storage is live.
    /// Must be called exactly once with the `SmallSidecarBacking` returned by
    /// `take_backing` for that sidecar: `Vmem` drops the `Reservation`
    /// (munmap) and bumps `VMEM_SIDECAR_RELEASES`; `System` runs the
    /// `ClassLeaves::record_free` bookkeeping plus `System.dealloc` with the
    /// allocating layout and bumps `HEAP_SIDECAR_RELEASES`.
    pub(crate) fn release_taken_backing(backing: SmallSidecarBacking, raw: *mut u8) {
        match backing {
            SmallSidecarBacking::Vmem(reservation) => {
                // Dropping the reservation unmaps the span; the payload drop
                // already returned above, so no reference into the span is
                // live here.
                drop(reservation);
                VMEM_SIDECAR_RELEASES.fetch_add(1, CoreOrdering::Relaxed);
            }
            SmallSidecarBacking::System => {
                #[cfg(feature = "internals")]
                ClassLeaves::record_free(core::mem::size_of::<Self>());
                // SAFETY: single documented reason: `raw` is the System
                // allocation this sidecar exclusively owned (created in
                // `allocate`, never copied or moved); the payload drop has
                // already returned, so the allocation is freed exactly once
                // here, with its allocating layout, with no live reference.
                unsafe { System.dealloc(raw, Layout::new::<Self>()) };
                HEAP_SIDECAR_RELEASES.fetch_add(1, CoreOrdering::Relaxed);
            }
        }
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
    /// A promoted leaf counts as one logical 256-byte request even though
    /// leaves share System chunks; see `chunk_totals_for_test` for the actual
    /// chunk requests. Excludes allocator overhead, RSS and the reservation.
    pub fn system_totals_for_test() -> (usize, usize, usize, usize, usize) {
        ClassLeaves::system_totals()
    }
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    /// Actual System chunk requests of the leaf slab: (allocations, frees).
    pub fn chunk_totals_for_test() -> (usize, usize) {
        ClassLeaves::chunk_totals()
    }
    /// Task #223 path-activation oracle: process-lifetime sidecar backing
    /// counters (vmem registrations, vmem releases, heap-fallback
    /// registrations, heap-fallback releases). Relaxed reads at quiescent
    /// checkpoints. Counterfactual: on the new path a Small registration
    /// bumps the VMEM pair and leaves the HEAP pair — and the ClassLeaves
    /// System byte totals — untouched, proving the full-struct zero-fill
    /// `System` allocation did not happen.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn sidecar_backing_counts_for_test() -> (usize, usize, usize, usize) {
        (
            VMEM_BACKED_SIDECARS.load(CoreOrdering::Relaxed),
            VMEM_SIDECAR_RELEASES.load(CoreOrdering::Relaxed),
            HEAP_SIDECARS.load(CoreOrdering::Relaxed),
            HEAP_SIDECAR_RELEASES.load(CoreOrdering::Relaxed),
        )
    }

    /// Task #223 counterfactual oracle: relaxed count of u64-granule payload
    /// words explicitly zero-filled in userspace at sidecar construction
    /// (the interpreter build's full-span `write_bytes` plus `initialize_at`'s pending
    /// words; see the static's documentation). A registration on the
    /// OS-zeroed-span path must leave this counter untouched; see
    /// `tests/p223_small_sidecar_zero_free_backing.rs`.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn explicit_sidecar_fill_words_for_test() -> usize {
        EXPLICIT_SIDECAR_FILL_WORDS.load(CoreOrdering::Relaxed)
    }

    /// Exhaustive-zero-state surface: raw pending word `word` (all-zero in
    /// the documented initial state).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn pending_word_for_test(&self, word: usize) -> u64 {
        assert!(word < SidecarBitmap::WORDS);
        self.pending[word].load(Ordering::Acquire)
    }

    /// Exhaustive-zero-state surface: raw leaf-presence word `word` of the
    /// class leaves (all-zero in the documented initial state).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn leaf_presence_word_for_test(&self, word: usize) -> u64 {
        self.classes.presence_word_for_test(word)
    }

    /// Exhaustive-zero-state surface: raw mixed-leaf slot address for `leaf`
    /// (null, i.e. `0`, in the documented initial state).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn mixed_leaf_slot_addr_for_test(&self, leaf: usize) -> usize {
        self.classes.mixed_slot_addr_for_test(leaf)
    }

    /// Exhaustive-zero-state surface: raw chunk-slot address for `chunk`
    /// (null, i.e. `0`, in the documented initial state).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn chunk_slot_addr_for_test(&self, chunk: usize) -> usize {
        self.classes.chunk_slot_addr_for_test(chunk)
    }

    /// Force the next `count` sidecar allocations onto the `System` heap
    /// fallback (0 restores the default OS-zeroed path).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn fail_vmem_reserve_for_test(count: usize) {
        FAIL_VMEM_SIDECAR_RESERVE.with(|cell| cell.set(count));
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

impl Drop for SmallSidecar {
    /// Payload-only drop: releases the promoted-leaf System chunks while the
    /// backing storage is still mapped/allocated. The backing storage is NOT
    /// touched here — it is extracted by `take_backing` and released by
    /// `release_taken_backing` on the owner side (`EntryHandle::free_sidecar`)
    /// AFTER this drop returns, so nothing unmaps/deallocates while a `&mut`
    /// reference into the storage is live. Both fields are `ManuallyDrop`, so
    /// no automatic field drop glue runs after this body either.
    fn drop(&mut self) {
        // SAFETY: single documented reason: `self.classes` is initialized
        // exactly once (by `allocate`'s OS-zeroed span or
        // `initialize_at`), `Drop for SmallSidecar` runs exactly once, and
        // the `ManuallyDrop` wrapper prevents the compiler's automatic
        // field drop from running it a second time — so dropping it here
        // frees the promoted-leaf System chunks while their owner storage is
        // still mapped/allocated (the owner-side storage release happens
        // only after this body returns).
        unsafe { core::mem::ManuallyDrop::drop(&mut self.classes) };
    }
}
