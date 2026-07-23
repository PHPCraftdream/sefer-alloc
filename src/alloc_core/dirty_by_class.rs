//! R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL): [`PerClassDirty`] — the
//! lazily-materialised per-(segment, class) dirty-bit sidecar that lets a
//! `drain_dirty_segments(class_idx)` call scan ONLY the segments dirty for
//! `class_idx`, instead of every segment dirty for ANY class (the O(D) vs
//! O(D_class) gap `docs/perf/R9_6_CLASS_AWARE_DIRTY_ROUTING_JUDGE.md`
//! measured at the counter level, and
//! `benches/r12_7_class_aware_dirty_wallclock.rs` confirmed translates into a
//! material wall-clock cost — see `docs/perf/R12_7_CLASS_AWARE_DIRTY_ROUTING_GATE.md`).
//!
//! ## Relationship to the existing per-segment `dirty_segments` bitmap
//!
//! This sidecar is STRICTLY ADDITIVE. The existing `HeapSlotRemote::
//! dirty_segments` bitmap (one bit per segment, set by ANY class's remote
//! free) is untouched and still set on every successful ring push — it
//! remains the fallback/ground-truth signal the non-`class-aware-dirty` drain
//! path uses. This sidecar layers a FINER-GRAINED hint on top: bit
//! `segment_id % 64` of word `class_idx * WORDS_PER_CLASS + segment_id / 64`
//! is set when a producer pushes a ring entry of THAT class into THAT
//! segment.
//!
//! A per-class bit is a HINT to visit, never a promise of what will be found
//! there and never load-bearing for correctness: `drain_dirty_segments`'s
//! drain body is UNCHANGED — once a candidate segment is picked (by whichever
//! bitmap is being scanned), the ENTIRE ring is drained in one pass exactly
//! as before, reclaiming every class's entries it holds and publishing them
//! to the directory via the existing `changed_classes`-driven
//! `sync_directory_for_segment_classes`. This is what makes the lost-wakeup
//! argument trivial: a per-class bit only decides WHICH segments get VISITED
//! for a given class_idx search; it never decides what the visit itself does
//! once a segment is chosen, so a stale or redundant bit can cost at most one
//! wasted (already-drained) visit — it can never cause an entry to be
//! silently skipped.
//!
//! ## Sizing and lazy materialisation
//!
//! `SMALL_CLASS_COUNT * WORDS_PER_CLASS` `AtomicU64` words — with the default
//! 49-class table and `WORDS_PER_CLASS = 16` (`MAX_SEGMENTS / 64`), that is
//! 784 words = 6,272 bytes = 6.1 KiB per materialised heap (58 classes under
//! `medium-classes`: 7,040 bytes). Reserved via the SAME
//! `aligned_vmem::leak_zeroed_pages` M5-clean direct-VM-reservation pattern
//! `segment_directory`'s owner-only sidecar and `registry::heap_overflow`'s
//! `HeapOverflowSidecar` both use, but published through
//! [`racy_ptr_cell::RacyPtrCell`] (the extracted, independently
//! loom-verified `UNINIT -> INITIALIZING -> READY` CAS-publish state machine
//! — see `crates/racy-ptr-cell`) rather than a hand-rolled sentinel protocol,
//! because — unlike `SegmentDirectory` (owner-only, single-writer, no
//! atomics needed) — this sidecar is written by ANY cross-thread producer
//! (the same reason `HeapOverflowSidecar` needs CAS-publish). Reusing
//! `RacyPtrCell` means the pointer-materialisation race itself is already
//! loom-proven by that crate's own suite
//! (`crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs`); this task's NEW loom
//! coverage (`tests/loom_class_aware_dirty.rs`) therefore models only the
//! genuinely new protocol surface — the per-(segment,class) bit-set/scan race
//! — not the pointer-publish race, which would be redundant re-verification
//! of an already-proven primitive.
//!
//! ## Placement
//!
//! Lives in `alloc_core` (not `registry`) even though the CELL it is stored
//! behind (`RacyPtrCell<PerClassDirty>`) is a field of
//! `registry::heap_slot::HeapSlotRemote` — mirroring `segment_directory`'s
//! own placement (also `alloc_core`, also referenced from `registry` for its
//! `WORDS_PER_CLASS` constant): `alloc_core` never depends on `registry`
//! (the dependency is one-directional, `registry -> alloc_core`), but
//! `AllocCore` (in `alloc_core`) needs to hold a `&'static
//! RacyPtrCell<PerClassDirty>` handle (bound at claim time, same discipline
//! as `AllocCore::dirty_segments`), so the TYPE must live somewhere
//! `alloc_core` can name without a reverse-direction `use crate::registry`.
//!
//! One `RacyPtrCell` per slot (`MAX_HEAPS = 4096` slots) rather than an eager
//! inline array: an eager `[AtomicU64; SMALL_CLASS_COUNT * WORDS_PER_CLASS]`
//! in EVERY slot would add ~6.1 KiB * 4096 = ~25 MiB to the registry's fixed
//! address-space layout (committed RSS only for touched pages, per this
//! crate's lazy-commit discipline, but still a real one-time layout tax on
//! every build with the feature on). The lazy sidecar costs nothing until a
//! heap's FIRST class-routed cross-thread free, mirroring `HeapOverflow`'s
//! own two-tier "cheap until genuinely needed" discipline.
//!
//! `class-aware-dirty` is EXPERIMENTAL: opt-in, additive over `alloc-xthread`
//! + `alloc-segment-directory`, NOT part of `production`. With the feature
//! OFF, none of this module's code exists in the binary and
//! `HeapSlotRemote`/`AllocCore` are byte-for-byte unchanged.

// This file is a named `unsafe` seam (mirrors `alloc_core::os`'s directory-
// sidecar reservation functions and `registry::bootstrap`'s overflow-sidecar
// reservation): the SINGLE documented reason to hold `unsafe` here is
// dereferencing the `RacyPtrCell`-published `*mut PerClassDirty` as
// `&'static PerClassDirty` — sound because `RacyPtrCell::get_or_try_init`
// only ever returns a pointer this module itself produced via
// `aligned_vmem::leak_zeroed_pages` (non-null, valid for
// `size_of::<PerClassDirty>()` bytes, OS-zeroed, leaked for the process
// lifetime — see that function's own safety contract).
//
// R14-9 (task #294) audit note: `ensure_per_class_dirty`/`get_per_class_dirty`
// below are safe `fn`s that internally use `unsafe { ptr.as_ref() }`, NOT
// `unsafe fn` boundaries like `alloc_core::sidecar::deref[_mut]` or
// `os::deref_directory_sidecar[_mut]`-turned-`sidecar::deref[_mut]`. This is
// a DELIBERATE, narrower exemption from that pattern, not an inconsistency:
// those other two sidecars (`SegmentDirectory`, `LargeCacheExtension`) hand
// out `&'static mut T` at some call site, so two safe-looking calls back to
// back can materialise ALIASING `&'static`/`&'static mut` references — real
// UB the type system cannot catch, which is exactly why their deref boundary
// must be `unsafe fn` (the aliasing discipline has to be re-justified at
// every call site). `PerClassDirty` is NEVER dereferenced as `&mut` anywhere
// in this crate (`grep -rn "PerClassDirty" src/` shows only `&'static
// PerClassDirty`/`&RacyPtrCell<PerClassDirty>` — its sole field is `[AtomicU64;
// _]`, and every mutation goes through `fetch_or`/`swap` on the atomics, never
// through a `&mut PerClassDirty`). Arbitrarily many `&PerClassDirty` may
// therefore safely coexist and be read/written concurrently through their
// interior mutability — the aliasing hazard the other two sidecars' `unsafe
// fn` signature guards against does not exist here, so forcing the same
// signature would be a stylistic-only widening of the `unsafe` surface with
// no corresponding safety gap closed.
#![allow(unsafe_code)]

use core::sync::atomic::AtomicU64;

use racy_ptr_cell::RacyPtrCell;

use super::segment_directory::WORDS_PER_CLASS;
use super::size_classes::SMALL_CLASS_COUNT;

/// Total `AtomicU64` words in one [`PerClassDirty`] sidecar:
/// `SMALL_CLASS_COUNT * WORDS_PER_CLASS`. Class `c`'s slice is words
/// `[c * WORDS_PER_CLASS, (c + 1) * WORDS_PER_CLASS)`.
pub(crate) const PER_CLASS_DIRTY_WORDS: usize = SMALL_CLASS_COUNT * WORDS_PER_CLASS;

/// The lazily-materialised per-(segment, class) dirty-bit sidecar. See the
/// module doc for the full design.
///
/// Plain `AtomicU64` words (not wrapped further) — producers `fetch_or` their
/// class's word (`Release`), the owner's drain `swap(0, Acquire)`s a class's
/// `WORDS_PER_CLASS`-word slice exactly as it already does for the shared
/// per-segment `dirty_segments` bitmap. `align_of::<PerClassDirty>() == 8 >=
/// 2`, satisfying `RacyPtrCell<T>`'s alignment precondition.
pub(crate) struct PerClassDirty {
    pub(crate) words: [AtomicU64; PER_CLASS_DIRTY_WORDS],
}

/// Byte size of one [`PerClassDirty`], rounded up to a multiple of
/// `aligned_vmem::PAGE` — mirrors `alloc_core::os`'s `DIRECTORY_SIDECAR_SIZE`
/// identical rounding for the same `leak_zeroed_pages` size contract.
const PER_CLASS_DIRTY_SIZE: usize = {
    let raw = core::mem::size_of::<PerClassDirty>();
    if raw == 0 {
        aligned_vmem::PAGE
    } else {
        let page = aligned_vmem::PAGE;
        (raw + page - 1) & !(page - 1)
    }
};

/// Resolve (or lazily materialise) the [`PerClassDirty`] sidecar behind
/// `cell`. Returns `None` only on OOM (sidecar OOM is NOT allocator OOM — the
/// per-class routing mechanism simply stays off for this heap and the caller
/// must fall back to the existing per-segment bitmap; see the call site in
/// `registry::heap_core_xthread::set_dirty_bit_for_segment`).
///
/// Thin wrapper over `RacyPtrCell::get_or_try_init`, whose `UNINIT ->
/// INITIALIZING -> READY` protocol (CAS-publish, OOM rollback, loser re-race)
/// is independently loom-verified by `crates/racy-ptr-cell`'s own suite — see
/// the module doc's "Sizing and lazy materialisation" section.
#[inline]
pub(crate) fn ensure_per_class_dirty(
    cell: &RacyPtrCell<PerClassDirty>,
) -> Option<&'static PerClassDirty> {
    let ptr = cell.get_or_try_init(|| {
        let base = aligned_vmem::leak_zeroed_pages(PER_CLASS_DIRTY_SIZE)?;
        Some(base.cast::<PerClassDirty>())
    })?;
    // SAFETY: `ptr` was produced by THIS closure via `leak_zeroed_pages`
    // (never by any other caller of this cell — `set_dirty_bit_for_segment`
    // and `drain_dirty_segments` are the only two call sites, and both go
    // through this module's two public functions), so it is non-null,
    // `PAGE`-aligned (>= `align_of::<PerClassDirty>() == 8`), valid for
    // `size_of::<PerClassDirty>()` bytes, OS-zeroed (all-zero is a fully
    // valid initial state for an array of `AtomicU64`), and leaked for the
    // process lifetime (`leak_zeroed_pages` never frees its reservation) —
    // `&'static` is sound.
    Some(unsafe { ptr.as_ref() })
}

/// Read-only resolve: `Some(&PerClassDirty)` iff the sidecar is already
/// materialised, `None` otherwise (never runs the init closure). Used by the
/// drain side (`AllocCore::drain_dirty_segments`), which must not pay a
/// reservation cost just to discover "nothing has ever been marked dirty for
/// this heap" — a heap that has received zero class-routed cross-thread
/// frees never materialises the sidecar at all, so the drain simply has
/// nothing to scan (falls back to the existing per-segment path via the
/// caller's own `#[cfg]` branch).
#[inline]
pub(crate) fn get_per_class_dirty(
    cell: &RacyPtrCell<PerClassDirty>,
) -> Option<&'static PerClassDirty> {
    let ptr = cell.get()?;
    // SAFETY: identical proof to `ensure_per_class_dirty` above — `get()`
    // only ever returns a pointer this module's own `ensure_per_class_dirty`
    // published.
    Some(unsafe { ptr.as_ref() })
}
