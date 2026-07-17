//! Single-threaded protocol tests for [`MpscRing`] and [`DirtyRouter`] — both
//! tiers (owned array + raw memory), both entries (`U32Entry`,
//! `UsizeU32Entry`), the drain-stop / drain-guard / return-actual-stop-position
//! contracts, and the cursor wrap.
//!
//! These do not run under `--cfg loom` (loom drives the concurrency proofs in
//! `loom_ring_mpsc.rs`); they are the fast isolation checks.

#![cfg(not(loom))]

use ring_mpsc::{DirtyRouter, Full, MpscRing, Owned, Raw, U32Entry, UsizeU32Entry, UsizeU32Slot};

// --- Owned tier, U32Entry ---------------------------------------------------

#[test]
fn owned_u32_push_drain_roundtrip() {
    let ring = MpscRing::<Owned<U32Entry, 4>>::new();
    assert!(ring.is_empty());
    ring.push(10).unwrap();
    ring.push(20).unwrap();
    let mut got = Vec::new();
    let stop = ring.drain(|v| got.push(v));
    assert_eq!(got, vec![10, 20]);
    assert_eq!(stop, 2);
    assert!(ring.is_empty());
}

#[test]
fn owned_u32_full_returns_err() {
    let ring = MpscRing::<Owned<U32Entry, 2>>::new();
    ring.push(1).unwrap();
    ring.push(2).unwrap();
    assert_eq!(ring.push(3), Err(Full));
    // Draining one frees exactly one slot.
    ring.drain(|_| {});
    ring.push(3).unwrap();
}

#[test]
fn drain_empty_is_noop() {
    let ring = MpscRing::<Owned<U32Entry, 4>>::new();
    let mut n = 0;
    ring.drain(|_| n += 1);
    assert_eq!(n, 0);
}

// --- The drain-stops-at-unpublished-slot + return-actual-stop contract ------

#[test]
fn drain_stops_at_unpublished_and_returns_actual_stop() {
    let ring = MpscRing::<Owned<U32Entry, 8>>::new();
    ring.push(100).unwrap();
    // Reserve one more slot WITHOUT publishing (simulates a producer mid-push).
    ring.dbg_reserve_unpublished();
    // Publish another AFTER the gap.
    ring.push(300).unwrap();

    let mut got = Vec::new();
    let stop = ring.drain(|v| got.push(v));
    // Only the first published entry drains; the drain stops at the unpublished
    // reservation and does NOT skip ahead to 300.
    assert_eq!(got, vec![100]);
    // R2-4: the returned stop position is the ACTUAL head (1), NOT the entry-time
    // tail (3). This is what keeps the guard re-draining.
    assert_eq!(stop, 1);
    let (head, tail) = ring.dbg_cursors();
    assert_eq!(head, 1);
    assert_eq!(tail, 3);
}

#[test]
fn drain_guard_tail_relaxed_matches_cached_stop_when_quiescent() {
    let ring = MpscRing::<Owned<U32Entry, 8>>::new();
    ring.push(1).unwrap();
    let cached = ring.drain(|_| {});
    // No push since the drain → tail_relaxed equals the cached stop, guard skips.
    assert_eq!(ring.tail_relaxed(), cached);
    ring.push(2).unwrap();
    // A push moved tail → guard now differs, forcing a real drain.
    assert_ne!(ring.tail_relaxed(), cached);
}

#[test]
fn head_relaxed_tracks_drain_progress() {
    let ring = MpscRing::<Owned<U32Entry, 8>>::new();
    assert_eq!(ring.head_relaxed(), 0);
    ring.push(1).unwrap();
    ring.push(2).unwrap();
    ring.drain(|_| {});
    assert_eq!(ring.head_relaxed(), 2);
}

// --- Cursor wrap ------------------------------------------------------------

#[test]
fn cursor_wrap_is_continuous() {
    let ring = MpscRing::<Owned<U32Entry, 4>>::new();
    // Drive the cursors near usize::MAX so the next pushes wrap.
    let near = usize::MAX - 1;
    ring.dbg_set_cursors(near, near);
    ring.push(7).unwrap();
    ring.push(8).unwrap();
    ring.push(9).unwrap(); // this one wraps tail past usize::MAX
    let mut got = Vec::new();
    ring.drain(|v| got.push(v));
    assert_eq!(got, vec![7, 8, 9]);
}

// --- Owned tier, UsizeU32Entry (two-word pair) ------------------------------

#[test]
fn owned_pair_push_drain_untorn() {
    let ring = MpscRing::<Owned<UsizeU32Entry, 4>>::new();
    ring.push((0x1000, 111)).unwrap();
    ring.push((0x2000, 222)).unwrap();
    let mut got = Vec::new();
    ring.drain(|pair| got.push(pair));
    assert_eq!(got, vec![(0x1000, 111), (0x2000, 222)]);
}

// --- Raw tier ---------------------------------------------------------------

#[test]
fn raw_u32_over_raw_roundtrip() {
    const CAP: usize = 8;
    type R = Raw<U32Entry, CAP>;
    let footprint = MpscRing::<R>::FOOTPRINT;
    let align = MpscRing::<R>::RAW_ALIGN;
    // Back it with an aligned owned buffer (a Vec of usize gives usize alignment,
    // which is >= RAW_ALIGN for these entries).
    assert!(align <= core::mem::align_of::<usize>());
    let words = footprint.div_ceil(core::mem::size_of::<usize>());
    let mut backing = vec![0usize; words];
    let base = backing.as_mut_ptr().cast::<u8>();
    // SAFETY: `backing` is exclusively owned, lives for the whole test, and is
    // `usize`-aligned (>= RAW_ALIGN) with at least `footprint` bytes.
    let ring = unsafe { MpscRing::<R>::over_raw(base) };
    ring.push(1).unwrap();
    ring.push(2).unwrap();
    ring.push(3).unwrap();
    let mut got = Vec::new();
    ring.drain(|v| got.push(v));
    assert_eq!(got, vec![1, 2, 3]);
    // Keep `backing` alive until here.
    drop(backing);
}

#[test]
fn raw_pair_over_raw_roundtrip() {
    const CAP: usize = 4;
    type R = Raw<UsizeU32Entry, CAP>;
    let footprint = MpscRing::<R>::FOOTPRINT;
    let words = footprint.div_ceil(core::mem::size_of::<usize>());
    let mut backing = vec![0usize; words + 1];
    let base = backing.as_mut_ptr().cast::<u8>();
    // SAFETY: as `raw_u32_over_raw_roundtrip`.
    let ring = unsafe { MpscRing::<R>::over_raw(base) };
    ring.push((0xAA00, 7)).unwrap();
    ring.push((0xBB00, 9)).unwrap();
    let mut got = Vec::new();
    ring.drain(|p| got.push(p));
    assert_eq!(got, vec![(0xAA00, 7), (0xBB00, 9)]);
    drop(backing);
}

#[test]
#[should_panic(expected = "non-null")]
fn over_raw_rejects_null() {
    // SAFETY: intentionally passing null to trigger the release-surviving guard;
    // the assert fires before any memory is touched.
    let _ = unsafe { MpscRing::<Raw<U32Entry, 4>>::over_raw(core::ptr::null_mut()) };
}

// --- UsizeU32Slot is public but opaque; smoke-check the type exists ---------

#[test]
fn slot_type_is_nameable() {
    // Just proves `UsizeU32Slot` is exported (used in `RingEntry::Slot`); no
    // construction — the ring builds these internally.
    fn _takes(_: &UsizeU32Slot) {}
}

// --- DirtyRouter ------------------------------------------------------------

#[test]
fn router_mark_and_for_each() {
    let router = DirtyRouter::<2>::new();
    assert_eq!(DirtyRouter::<2>::CAPACITY, 128);
    router.mark(0);
    router.mark(5);
    router.mark(64);
    router.mark(127);
    let mut seen = Vec::new();
    router.for_each_dirty(|k| seen.push(k));
    seen.sort_unstable();
    assert_eq!(seen, vec![0, 5, 64, 127]);
    // Consumed: a second pass sees nothing.
    let mut n = 0;
    router.for_each_dirty(|_| n += 1);
    assert_eq!(n, 0);
}

#[test]
fn router_is_idempotent_mark() {
    let router = DirtyRouter::<1>::new();
    router.mark(3);
    router.mark(3);
    assert!(router.dbg_is_marked(3));
    let mut seen = Vec::new();
    router.for_each_dirty(|k| seen.push(k));
    assert_eq!(seen, vec![3]);
}

#[test]
fn router_const_static() {
    // Proves `new()` is `const` (lives in a static) on a normal build.
    static R: DirtyRouter<1> = DirtyRouter::new();
    R.mark(1);
    assert!(R.dbg_is_marked(1));
}
