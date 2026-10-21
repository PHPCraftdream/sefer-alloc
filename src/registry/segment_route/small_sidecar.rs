//! Owning backing for the single authoritative SidecarBitmap primitive.
#![allow(unsafe_code)]

use super::{AtomicU64, Ordering};
use core::ptr;

use crate::alloc_core::fatal::fatal;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::remote_bitmap::SidecarBitmap;
use crate::alloc_core::size_classes::MIN_BLOCK;

use super::RouteScan;

// Production-geometry pins for the sidecar primitive, moved verbatim from
// sidecar_bitmap.rs so the actual-source Loom harness can include it with a
// reduced SEGMENT (tests/loom_sidecar_bitmap_actual.rs).
const _: () = {
    assert!(SEGMENT == 4 * 1024 * 1024);
    assert!(MIN_BLOCK == 16);
    assert!(SidecarBitmap::WORDS == 4096);
    assert!(SidecarBitmap::GRANULES == 262_144);
};

/// `#[repr(C)]` external ingress backing: pending words, then the dense
/// "start was issued" map. The directory allocates it with `System.alloc`
/// and constructs it in place with `initialize_at`, which writes the
/// all-zero pattern — the valid initial state (zero pending words, zero
/// start bits: `SidecarBitmap::publish` drops any offset whose start bit is
/// 0). Total footprint is 65 536 B (two 32 KiB banks) in the SAME System
/// allocation. D1′ phase-1b: the
/// adaptive class leaves are GONE — the record's class lives only in the
/// owner-private state byte, and this sidecar carries no class state.
#[repr(C)]
pub struct SmallSidecar {
    pending: [AtomicU64; SidecarBitmap::WORDS],
    /// D1′ phase-1a: 1 bit per 16 B granule, "a fresh start was issued at
    /// this granule". Single writer: the owner sets a bit (Release store of
    /// `old | mask` after an Acquire load) exactly at the former leaf-issue
    /// points and never before the block's own commit; clears happen only on
    /// retain-reset (together with `pending`). Producers read it Acquire.
    starts: [AtomicU64; SidecarBitmap::WORDS],
}

// D1′ phase-1b: sidecar System accounting for ONE `SmallSidecar` per
// registered route. The former per-leaf promotion allocations (and their
// accounting) are gone with the class leaves; what remains is the fixed
// 65 536 B backing allocated in `EntryHandle::new` and freed by
// `SmallSidecar::dealloc_sidecar`. Counters are diagnostics only, read at
// quiescent checkpoints; relaxed ordering, wrapping totals.
#[cfg(feature = "internals")]
static SIDECAR_SYSTEM_TOTALS: [core::sync::atomic::AtomicUsize; 5] =
    [const { core::sync::atomic::AtomicUsize::new(0) }; 5];

/// Record one sidecar System allocation (`zeroed` is always `false` today:
/// `initialize_at` writes the zero pattern in plain stores, so the System
/// allocation itself is not a zeroed request).
#[cfg(feature = "internals")]
pub(crate) fn record_system_alloc(bytes: usize, zeroed: bool) {
    use core::sync::atomic::Ordering;
    SIDECAR_SYSTEM_TOTALS[0].fetch_add(bytes, Ordering::Relaxed);
    if zeroed {
        SIDECAR_SYSTEM_TOTALS[1].fetch_add(bytes, Ordering::Relaxed);
    }
    SIDECAR_SYSTEM_TOTALS[3].fetch_add(1, Ordering::Relaxed);
}

/// Record one sidecar System free.
#[cfg(feature = "internals")]
pub(crate) fn record_system_free(bytes: usize) {
    use core::sync::atomic::Ordering;
    SIDECAR_SYSTEM_TOTALS[2].fetch_add(bytes, Ordering::Relaxed);
    SIDECAR_SYSTEM_TOTALS[4].fetch_add(1, Ordering::Relaxed);
}

/// (alloc bytes, zeroed bytes, free bytes, alloc calls, free calls).
#[cfg(feature = "internals")]
pub(crate) fn system_totals() -> (usize, usize, usize, usize, usize) {
    use core::sync::atomic::Ordering;
    (
        SIDECAR_SYSTEM_TOTALS[0].load(Ordering::Relaxed),
        SIDECAR_SYSTEM_TOTALS[1].load(Ordering::Relaxed),
        SIDECAR_SYSTEM_TOTALS[2].load(Ordering::Relaxed),
        SIDECAR_SYSTEM_TOTALS[3].load(Ordering::Relaxed),
        SIDECAR_SYSTEM_TOTALS[4].load(Ordering::Relaxed),
    )
}

impl SmallSidecar {
    /// System-side teardown of one fully initialized `SmallSidecar` (moved
    /// verbatim from `directory.rs`'s `free_sidecar` so that file stays at
    /// its 1000-line cap; last-pin release guarantees no reference remains).
    pub(crate) fn dealloc_sidecar(sidecar: *mut SmallSidecar) {
        // SAFETY: pointer is a fully initialized SmallSidecar from System;
        // last-pin release ensures no remaining reference.
        unsafe { ptr::drop_in_place(sidecar) };
        #[cfg(feature = "internals")]
        record_system_free(core::mem::size_of::<SmallSidecar>());
        // SAFETY: pointer came from System.alloc with this exact Layout and
        // its mixed leaves were released by Drop above.
        use std::alloc::{GlobalAlloc, System};
        unsafe { System.dealloc(sidecar.cast(), core::alloc::Layout::new::<Self>()) };
    }
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
    /// `payload_start` is the registering route kind's metadata end in bytes.
    pub(crate) unsafe fn initialize_at(raw: *mut Self) {
        // SAFETY: caller grants exclusive writable SmallSidecar storage;
        // these raw field projections make no reference to uninitialized
        // data.
        let pending = unsafe { ptr::addr_of_mut!((*raw).pending).cast::<AtomicU64>() };
        for i in 0..SidecarBitmap::WORDS {
            // SAFETY: i is within the pending field and each atomic word is
            // initialized once before directory publication.
            unsafe { pending.add(i).write(AtomicU64::new(0)) };
        }
        // SAFETY: disjoint starts field in the same private allocation;
        // pending is initialized and publication is still excluded.
        let starts = unsafe { ptr::addr_of_mut!((*raw).starts).cast::<AtomicU64>() };
        for i in 0..SidecarBitmap::WORDS {
            // SAFETY: i is within the starts field; each word is written
            // exactly once before publication.
            unsafe { starts.add(i).write(AtomicU64::new(0)) };
        }
    }

    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_pending(&self.pending, &self.starts).unwrap_or_else(|| {
            fatal(
                b"small_sidecar: sidecar bitmap layout mismatch
",
            )
        })
    }

    /// Doc-hidden test-only forwarder (CLAUDE.md "test-only export pattern"):
    /// the owner issue point — set the record's start bit — for internals
    /// tests that drive publication without driving the allocator. Same
    /// single-writer store as the production issue; no allocation, no class
    /// carrier (D1′ phase-1b).
    /// `cfg(loom)`: loom model tests run with the canonical feature set
    /// (`alloc-core,alloc-xthread,tagged-index-stack/loom` — no `internals`),
    /// yet still need this hook to drive the owner issue point.
    #[cfg(any(feature = "internals", loom))]
    #[doc(hidden)]
    pub fn issue_for_test(&self, offset: u32) {
        self.issue(offset)
    }

    /// Crate-private owner-only, infallible issue: set the record's start
    /// bit exactly at the former leaf-issue point (single-writer load →
    /// or → Release store; monotone, so a free-list reissue stores the
    /// already-set bit back — no new record). D1′ phase-1b: the class-leaf
    /// spill OOM and the transactional prepare half are gone; there is
    /// nothing fallible left on this path.
    pub(crate) fn issue(&self, offset: u32) {
        self.bitmap().issue(offset);
    }

    /// Doc-hidden test-only mutator (CLAUDE.md "test-only export pattern"):
    /// unsets ONE start bit so tests can arm the reclaim
    /// "published record with start bit 0 → fatal" lane (a pending record
    /// whose start granule lost its start). Single-word `fetch_and` on the
    /// same atomic bank the owner writes; no allocation, no segment memory.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_unset_start_bit_for_test(&self, offset: u32) {
        let granule = offset as usize / MIN_BLOCK;
        debug_assert!(granule < SidecarBitmap::GRANULES);
        let mask = 1u64 << (granule % 64);
        self.starts[granule / 64].fetch_and(!mask, Ordering::AcqRel);
    }

    /// Owner-only Acquire read of one start bit (reclaim's start-bit check).
    pub(crate) fn start_bit(&self, offset: u32) -> bool {
        let granule = offset as usize / MIN_BLOCK;
        debug_assert!(granule < SidecarBitmap::GRANULES);
        self.starts[granule / 64].load(Ordering::Acquire) & (1u64 << (granule % 64)) != 0
    }

    /// Owner-only Acquire read: any start bit set in `[start_off, end_off)`;
    /// `end_off` is EXCLUDED (reclaim's open-interval overlap check).
    pub(crate) fn start_bits_between(&self, start_off: u32, end_off: u32) -> bool {
        bank_bits_between(&self.starts, start_off, end_off)
    }

    /// Owner-only carve-clear (design §2.4): drop the PENDING bit of the
    /// fresh block's start granule before the LIVE write. Only the RMW form
    /// is used (`load(Acquire)` to skip clean words, then
    /// `fetch_and(!mask, AcqRel)`) — a plain load+store would lose
    /// concurrent legitimate bits of neighbouring granules in the same word
    /// (the existing negative control
    /// `loom_actual_negative_cut_via_load_store_loses_concurrent_bit`).
    pub(crate) fn clear_pending_start_bit(&self, offset: u32) {
        let granule = offset as usize / MIN_BLOCK;
        debug_assert!(granule < SidecarBitmap::GRANULES);
        let mask = 1u64 << (granule % 64);
        #[cfg(any(feature = "internals", feature = "bench-internals"))]
        crate::alloc_core::CARVE_CLEAR_WORD_LOADS.fetch_add(1, Ordering::Relaxed);
        if self.pending[granule / 64].load(Ordering::Acquire) & mask != 0 {
            self.pending[granule / 64].fetch_and(!mask, Ordering::AcqRel);
            #[cfg(any(feature = "internals", feature = "bench-internals"))]
            crate::alloc_core::CARVE_CLEAR_BITS.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Owner-only retain-reset (design §2.6): atomically clear BOTH the
    /// pending bits and the start bits of every granule in
    /// `[start_off, end_off)` — the same range the state-byte reset zeroes
    /// (`[payload, old_bump)`). Masked `fetch_and(AcqRel)` per touched word;
    /// concurrent publications of granules OUTSIDE the cleared mask range
    /// but inside the same word keep their bits.
    // Only the retain-reset hook (bench-internals + internals +
    // alloc-decommit) reaches this seam; allow the production-only build.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub(crate) fn clear_remote_bits_range(&self, start_off: u32, end_off: u32) {
        // Both banks share the granule geometry, so one masked walk per bank
        // with the same per-word masks clears pending AND starts atomically.
        for_words_in_range(&self.pending, start_off, end_off, |w, mask| {
            let _ = self.pending[w].fetch_and(!mask, Ordering::AcqRel);
        });
        for_words_in_range(&self.starts, start_off, end_off, |w, mask| {
            let _ = self.starts[w].fetch_and(!mask, Ordering::AcqRel);
        });
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
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    /// Requested System bytes: (alloc, zeroed, free, alloc calls, free calls)
    /// for the sidecar backing itself — one `SmallSidecar` System
    /// allocation/free per registered route (D1′ phase-1b: no promoted
    /// class-leaf allocations exist anymore). Excludes allocator overhead,
    /// RSS and the reservation itself.
    pub fn system_totals_for_test() -> (usize, usize, usize, usize, usize) {
        system_totals()
    }
    #[cfg(feature = "internals")]
    pub(super) fn pending_for_test(&self, offset: u32) -> bool {
        let bit = offset as usize / MIN_BLOCK;
        bit < SidecarBitmap::GRANULES
            && self.pending[bit / 64].load(Ordering::Acquire) & (1u64 << (bit % 64)) != 0
    }

    /// Producer terminal publication, called by consuming RoutePin.
    /// `payload_start` is the registering route kind's metadata end in bytes
    /// (supplied by the consuming pin from the route descriptor; the sidecar
    /// itself stays exactly two 32 KiB banks).
    pub(super) fn publish(&self, offset: u32, payload_start: usize) -> bool {
        self.bitmap().publish(offset, payload_start)
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

/// Visit every word of `bank` intersecting the granule range
/// `[start_off, end_off)` (granules of `MIN_BLOCK` bytes; `end_off`
/// EXCLUDED) and call `f(word_index, word_mask)` with the sub-range mask of
/// the bits inside the range.
fn for_words_in_range<F: FnMut(usize, u64)>(
    bank: &[AtomicU64],
    start_off: u32,
    end_off: u32,
    mut f: F,
) {
    let granules = bank.len() * 64;
    let g0 = (start_off as usize / MIN_BLOCK).min(granules);
    let g1 = (end_off as usize / MIN_BLOCK).min(granules);
    if g1 <= g0 {
        return;
    }
    let (w0, b0) = (g0 / 64, g0 % 64);
    let (w1, b1) = ((g1 - 1) / 64, (g1 - 1) % 64);
    let mut w = w0;
    loop {
        let lo = if w == w0 { b0 } else { 0 };
        let hi = if w == w1 { b1 } else { 63 };
        let mask = (u64::MAX >> (63 - hi)) & !((1u64 << lo) - 1);
        f(w, mask);
        if w == w1 {
            return;
        }
        w += 1;
    }
}

/// Acquire read: any bit of `bank` set inside granule range
/// `[start_off, end_off)` (`end_off` excluded).
fn bank_bits_between(bank: &[AtomicU64], start_off: u32, end_off: u32) -> bool {
    let mut found = false;
    for_words_in_range(bank, start_off, end_off, |w, mask| {
        found |= bank[w].load(Ordering::Acquire) & mask != 0;
    });
    found
}
