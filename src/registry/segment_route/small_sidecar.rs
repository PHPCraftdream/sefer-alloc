//! Единственный System backing лениво конструируемых starts/pending чанков.
#![allow(unsafe_code)]

use core::ptr;

use crate::alloc_core::fatal::fatal;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::remote_bitmap::{BitmapScan, ChunkedBank};
use crate::alloc_core::size_classes::MIN_BLOCK;

use super::RouteScan;

// Production-geometry pins for the sidecar primitive, moved verbatim from
// sidecar_bitmap.rs so the actual-source Loom harness can include it with a
// reduced SEGMENT (tests/loom_sidecar_bitmap_actual.rs).
const _: () = {
    assert!(SEGMENT == 4 * 1024 * 1024);
    assert!(MIN_BLOCK == 16);
    assert!(ChunkedBank::WORDS == 4096);
    assert!(ChunkedBank::GRANULES == 262_144);
};

/// Единственный backing с лениво constructed starts/pending чанками.
#[repr(C)]
pub struct SmallSidecar {
    bank: ChunkedBank,
}

// Один System backing на маршрут: 65 600 B вместе с ready и выравниванием.
// Чанки не выделяют память. Счётчики читаются в quiescent-точках;
// relaxed ordering, wrapping totals.
#[cfg(feature = "internals")]
static SIDECAR_SYSTEM_TOTALS: [core::sync::atomic::AtomicUsize; 5] =
    [const { core::sync::atomic::AtomicUsize::new(0) }; 5];

/// Учёт System allocation; `zeroed` всегда false: ready и оболочки
/// инициализируются явно, payload чанков — позже, перед публикацией ready.
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
    ///
    /// # Safety
    ///
    /// `sidecar` MUST be a non-null, aligned pointer to a fully initialized
    /// `SmallSidecar` that was produced by `System.alloc` with
    /// `Layout::new::<SmallSidecar>()` (as `init_sidecar` does), and this
    /// MUST be the last release of the sidecar: the last-pin drop of the
    /// owning route entry must have already guaranteed that no reference
    /// (pin, descriptor or reader) remains, so dropping in place is safe and
    /// the deallocation matches the single prior allocation exactly.
    /// `sidecar` must not be used again after this call.
    pub(super) unsafe fn dealloc_sidecar(sidecar: *mut SmallSidecar) {
        // SAFETY: `sidecar` is a fully initialized `SmallSidecar` from
        // `System.alloc` and the last-pin release guarantees no remaining
        // reference (see # Safety above).
        unsafe { ptr::drop_in_place(sidecar) };
        #[cfg(feature = "internals")]
        record_system_free(core::mem::size_of::<SmallSidecar>());
        use std::alloc::{GlobalAlloc, System};
        // SAFETY: `sidecar` came from `System.alloc` with this exact layout
        // and was fully dropped by `drop_in_place` above (see # Safety).
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

    // R6-T16: no in-src reader (the struct field uses ChunkedBank::WORDS);
    // consumed by internals builds and source-included oracle crates only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub const WORDS: usize = ChunkedBank::WORDS;

    /// Число constructed чанков; read-only oracle, не pending-summary.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn initialized_chunks_for_test(&self) -> usize {
        self.bank.initialized_chunks()
    }

    /// # Safety
    /// `raw` — приватный выровненный System allocation с точным layout Self.
    /// До возврата его нельзя наблюдать или уничтожать. Инициализируются
    /// ready и оболочки; payload конструируется единственным owner позже.
    pub(crate) unsafe fn initialize_at(raw: *mut Self) {
        // SAFETY: приватный allocation Self содержит выровненное поле bank.
        unsafe { ChunkedBank::initialize_at(ptr::addr_of_mut!((*raw).bank)) };
    }

    /// Doc-hidden test-only forwarder (CLAUDE.md "test-only export pattern"):
    /// the owner issue point — set the record's start bit — for internals
    /// tests that drive publication without driving the allocator. Same
    /// single-writer store as the production issue; no allocation, no class
    /// carrier (D1′ phase-1b).
    /// `cfg(loom)`: loom model tests run with the canonical feature set
    /// (`alloc-core,alloc-xthread,tagged-index-stack/loom` — no `internals`),
    /// yet still need this hook to drive the owner issue point.
    /// # Safety
    /// Вызывающий — единственный owner; другие issue/retain исключены.
    #[cfg(any(feature = "internals", loom))]
    #[doc(hidden)]
    pub unsafe fn issue_for_test(&self, offset: u32) {
        // SAFETY: вызывающий сохраняет единственного конструктора по контракту.
        unsafe { self.issue(offset) };
    }

    /// Crate-private owner-only, infallible issue: set the record's start
    /// bit exactly at the former leaf-issue point (single-writer load →
    /// or → Release store; monotone, so a free-list reissue stores the
    /// already-set bit back — no new record). D1′ phase-1b: the class-leaf
    /// spill OOM and the transactional prepare half are gone; there is
    /// nothing fallible left on this path.
    /// # Safety
    /// Уникальная owner-capability, issue/retain последовательны до handoff.
    pub(crate) unsafe fn issue(&self, offset: u32) {
        // SAFETY: owner-capability вызывающего исключает параллельную конструкцию.
        unsafe { self.bank.issue(offset) };
    }

    /// Doc-hidden test-only mutator (CLAUDE.md "test-only export pattern"):
    /// unsets ONE start bit so tests can arm the reclaim
    /// "published record with start bit 0 → fatal" lane (a pending record
    /// whose start granule lost its start). Single-word `fetch_and` on the
    /// same atomic bank the owner writes; no allocation, no segment memory.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_unset_start_bit_for_test(&self, offset: u32) {
        let granule = ChunkedBank::granule(offset)
            .unwrap_or_else(|| fatal(b"small_sidecar: invalid start offset\n"));
        self.bank.clear_starts(granule / 64, 1u64 << (granule % 64));
    }

    /// Owner-only Acquire read of one start bit (reclaim's start-bit check).
    pub(crate) fn start_bit(&self, offset: u32) -> bool {
        ChunkedBank::granule(offset).is_some_and(|granule| {
            self.bank.starts_load(granule / 64) & (1u64 << (granule % 64)) != 0
        })
    }

    /// Owner-only Acquire read: any start bit set in `[start_off, end_off)`;
    /// `end_off` is EXCLUDED (reclaim's open-interval overlap check).
    pub(crate) fn start_bits_between(&self, start_off: u32, end_off: u32) -> bool {
        bank_bits_between(&self.bank, start_off, end_off)
    }

    /// Owner-only carve-clear (design §2.4): drop the PENDING bit of the
    /// fresh block's start granule before the LIVE write. Only the RMW form
    /// is used (`load(Acquire)` to skip clean words, then
    /// `fetch_and(!mask, AcqRel)`) — a plain load+store would lose
    /// concurrent legitimate bits of neighbouring granules in the same word
    /// (the existing negative control
    /// `loom_actual_negative_cut_via_load_store_loses_concurrent_bit`).
    pub(crate) fn clear_pending_start_bit(&self, offset: u32) {
        let granule = ChunkedBank::granule(offset)
            .unwrap_or_else(|| fatal(b"small_sidecar: invalid clear offset\n"));
        let mask = 1u64 << (granule % 64);
        #[cfg(any(feature = "internals", feature = "bench-internals"))]
        crate::alloc_core::CARVE_CLEAR_WORD_LOADS
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if self.bank.pending_load(granule / 64) & mask != 0 {
            self.bank.clear_pending(granule / 64, mask);
            #[cfg(any(feature = "internals", feature = "bench-internals"))]
            crate::alloc_core::CARVE_CLEAR_BITS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
        for_words_in_range(ChunkedBank::WORDS, start_off, end_off, |word, mask| {
            self.bank.clear_pending(word, mask);
            self.bank.clear_starts(word, mask);
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
        ChunkedBank::granule(offset).is_some_and(|granule| {
            self.bank.pending_load(granule / 64) & (1u64 << (granule % 64)) != 0
        })
    }

    /// Producer terminal publication, called by consuming RoutePin.
    /// `payload_start` — конец metadata из descriptor маршрута.
    /// Sidecar содержит два логических банка по 32 KiB и ready-mask 64 B.
    pub(super) fn publish(&self, offset: u32, payload_start: usize) -> bool {
        self.bank.publish(offset, payload_start)
    }

    /// Owner-only bounded per-word cut through the existing bitmap scanner.
    // R6-T16: internals-test/oracle surface only (production drains use
    // scan_from); suppressed exactly outside internals.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn scan(&self, high_water: usize) -> Option<RouteScan<'_>> {
        self.scan_from(high_water, 0)
    }

    pub(crate) fn scan_from(&self, high_water: usize, word: usize) -> Option<RouteScan<'_>> {
        BitmapScan::from_chunked(&self.bank, high_water, word).map(|inner| RouteScan { inner })
    }
}

/// Visit every word of `bank` intersecting the granule range
/// `[start_off, end_off)` (granules of `MIN_BLOCK` bytes; `end_off`
/// EXCLUDED) and call `f(word_index, word_mask)` with the sub-range mask of
/// the bits inside the range.
fn for_words_in_range<F: FnMut(usize, u64)>(words: usize, start_off: u32, end_off: u32, mut f: F) {
    let granules = words * 64;
    let g0 = (start_off as usize / MIN_BLOCK).min(granules);
    let g1 = (end_off as usize / MIN_BLOCK).min(granules);
    if g1 <= g0 {
        return;
    }
    for word in (g0 / 64)..=((g1 - 1) / 64) {
        f(word, ChunkedBank::range_mask(g0, g1, word));
    }
}

/// Acquire read: any bit of `bank` set inside granule range
/// `[start_off, end_off)` (`end_off` excluded).
fn bank_bits_between(bank: &ChunkedBank, start_off: u32, end_off: u32) -> bool {
    let mut found = false;
    for_words_in_range(ChunkedBank::WORDS, start_off, end_off, |w, mask| {
        found |= bank.starts_load(w) & mask != 0;
    });
    found
}
