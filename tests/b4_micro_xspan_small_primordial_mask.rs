//! X-span: in `canonical_block_of` the non-Large branch derives the offset
//! as `ptr.addr() & (SEGMENT - 1)` instead of a full `ptr - root`
//! subtraction (Large keeps the exact `off >= span` bound, T22).
//!
//! Counterfactual mutants (both verified RED during authoring):
//! - shrink the mask to `& (SEGMENT / 2 - 1)`: every Small/Primordial block
//!   with offset ≥ 2 MiB resolves to the WRONG granule in its segment — the
//!   free is rejected (state byte of the aliased granule does not match a
//!   live block of the class), `LOCAL_REJECT_FREES` bumps and the state-byte
//!   oracle still reads LIVE after the dealloc;
//! - drop the mask entirely (`off = ptr.addr()`): the offset is astronomically
//!   out of the segment, every small free is rejected — same signals;
//! - weaken the Large span bound (`span_usable_at(base) + 4 MiB`, run under
//!   `exact-span-large`): the live-probed in-window foreign address resolves
//!   past the guard and is rejected only as an interior Large free —
//!   `LOCAL_REJECT_FREES` bumps, the test goes red (verified RED).
//!
//! With the correct mask all frees below land exactly, including blocks at
//! the deepest offsets of the Primordial (first) and of later Small
//! segments, and the Large span bound still accepts exact starts and rejects
//! in-window foreign addresses (counter deltas stay flat).

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

const SEGMENT: usize = SegmentLayout::SEGMENT; // 4 MiB, power of two
const BLOCK: usize = 4096; // small class
/// Blocks of `BLOCK` bytes per full Small segment payload.
const PER_SEG: usize = SEGMENT / BLOCK;
/// Enough blocks to span several segments; the LAST block sits at the
/// deepest offset (~4 MiB - 4 KiB) of its segment.
const N: usize = 3 * PER_SEG + 7;

fn layout4096() -> Layout {
    Layout::from_size_align(BLOCK, BLOCK).unwrap()
}

fn granule_of(p: *mut u8) -> usize {
    ((p as usize) & (SEGMENT - 1)) / 16 // MIN_BLOCK granule
}

fn dump(ac: &AllocCore, anchor: *mut u8) -> Vec<u8> {
    let mut v = vec![0u8; SEGMENT / 16];
    // SAFETY: `anchor` is a live allocation owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(anchor, &mut v) };
    v
}

/// Small + Primordial blocks at every offset, including deep ones, free
/// exactly: no local rejects, no foreign routing, state bytes read FREE.
#[test]
fn xspan_small_and_primordial_deeps_free_exactly() {
    let mut ac = AllocCore::new().expect("primordial");
    let l = layout4096();

    let mut ps = Vec::with_capacity(N);
    for _ in 0..N {
        let p = ac.alloc(l);
        assert!(!p.is_null());
        ps.push(p);
    }
    // Sanity: we really span multiple segments and cover deep offsets.
    let deep = *ps.iter().max_by_key(|p| p.addr() & (SEGMENT - 1)).unwrap();
    assert!(
        deep.addr() & (SEGMENT - 1) >= SEGMENT / 2,
        "a deep block must sit past the 2 MiB half of its segment"
    );
    // Primordial = the segment of the very first block of a fresh core.
    let first = ps[0];
    let primordial = ps
        .iter()
        .filter(|p| p.addr() & !(SEGMENT - 1) == first.addr() & !(SEGMENT - 1))
        .max_by_key(|p| p.addr() & (SEGMENT - 1))
        .copied()
        .unwrap();
    assert!(primordial.addr() & (SEGMENT - 1) >= SEGMENT / 4);
    // At least one full Small (non-Primordial) segment is involved.
    assert!(ps
        .iter()
        .any(|p| p.addr() & !(SEGMENT - 1) != first.addr() & !(SEGMENT - 1)));

    let reject_before = AllocCore::dbg_local_reject_frees();
    let foreign_before = AllocCore::dbg_foreign_or_unroutable_frees();

    for &p in &ps {
        // SAFETY: exact start of a live allocation.
        unsafe { ac.dealloc(p, l) };
    }

    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        reject_before,
        "every small free must land exactly — a wrong mask rejects deep blocks"
    );
    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees(),
        foreign_before,
        "every small free must stay own-routed"
    );

    // State-byte oracle: sampled blocks across the Primordial segment, the
    // first full Small segment after it, and the deepest offset read FREE.
    let anchors = [first, primordial, deep];
    for &a in &anchors {
        // SAFETY: the block was freed above; the segment is still reserved.
        let buf = dump(&ac, a);
        let g = granule_of(a);
        assert_ne!(buf[g], 0, "granule {g} must have a state byte");
        assert_eq!(
            buf[g] >> 6,
            2,
            "block at off {:#x} of segment {:#x} must read FREE after dealloc",
            a.addr() & (SEGMENT - 1),
            a.addr() & !(SEGMENT - 1),
        );
    }

    // The freed memory is actually reusable.
    let p = ac.alloc(l);
    assert!(!p.is_null());
}

/// The Large branch is untouched: an exact Large start still resolves (its
/// free succeeds silently), and `off >= span` still rejects in-window
/// foreign addresses (both stay counter-flat, no rejects, no state damage).
///
/// The foreign address is built WITHOUT pointer arithmetic on the (freed)
/// allocation: `core::ptr::without_provenance_mut(p.addr() + OFFSET)` — a
/// bare address (usize), zero provenance, so no `byte_add` past the
/// allocation's lifetime (that would be UB). The Large allocation (3 MiB,
/// 4 MiB reservation) keeps `p.addr() + 3.5 MiB` INSIDE the same 4 MiB key
/// window: `segment_base_of_ptr` yields the same candidate as `p`.
///
/// WHERE the rejection happens is build-dependent, and the test probes the
/// genuinely-claimed path whenever the build makes it observable:
/// - **`exact-span-large`** (usable span == the 3 MiB request < SEGMENT): the
///   in-window foreign address is probed while `p` is STILL LIVE and its
///   segment registered — the lookup reaches the Large branch and the
///   address is rejected purely by `off >= SegmentHeader::span_usable_at`
///   (T22). Without this feature the usable span is the whole segment, so no
///   in-window past-span address exists and the probe would resolve (and be
///   rejected only later as an interior Large free).
/// - otherwise: the same address is probed AFTER the exact free (which
///   deregisters the segment) — the rejection is then the key-window miss,
///   counter-flat all the same.
#[test]
fn xspan_large_span_bound_untouched() {
    let mut ac = AllocCore::new().expect("primordial");
    // 3 MiB: Large-classified (> SMALL_MAX), reserves a 4 MiB segment; under
    // `exact-span-large` the usable span (3 MiB) stays below SEGMENT, leaving
    // in-window past-span addresses (e.g. offset 3.5 MiB).
    let big = Layout::from_size_align(3 * 1024 * 1024, 4096).unwrap(); // > SMALL_MAX
    let p = ac.alloc(big);
    assert!(!p.is_null(), "large alloc");

    let reject_before = AllocCore::dbg_local_reject_frees();
    let foreign_before = AllocCore::dbg_foreign_or_unroutable_frees();

    #[cfg(feature = "exact-span-large")]
    {
        // In-window foreign address probed while LIVE (see doc): same 4 MiB
        // key window (same mask -> same candidate), offset 3.5 MiB >= the
        // 3 MiB usable span — must reject via `off >= span` WITHOUT bumping
        // LOCAL_REJECT (it never resolves to a canonical block at all).
        // Built from the bare ADDRESS only (`without_provenance_mut`), no
        // arithmetic on the live pointer.
        let q: *mut u8 = core::ptr::without_provenance_mut(p.addr() + 3 * 1024 * 1024 + 512 * 1024);
        // SAFETY: deliberately foreign (provenance-free) pointer exercising
        // the T22 span guard; it is never dereferenced.
        unsafe { ac.dealloc(q, big) };
    }

    // Exact start: must resolve through the Large span check.
    // SAFETY: exact start of a live Large allocation.
    unsafe { ac.dealloc(p, big) };

    #[cfg(not(feature = "exact-span-large"))]
    {
        // Same in-window address, probed after the exact free (the segment
        // is deregistered by then, so the rejection is the key-window miss;
        // the span bound itself is covered by the `exact-span-large` arm
        // above). Still provenance-free — no arithmetic on the freed block.
        let q: *mut u8 = core::ptr::without_provenance_mut(p.addr() + 3 * 1024 * 1024 + 512 * 1024);
        // SAFETY: deliberately foreign pointer; never dereferenced.
        unsafe { ac.dealloc(q, big) };
    }

    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        reject_before,
        "exact Large free must not be rejected"
    );
    // Without the xthread routing tier (`alloc-core`-only config) the
    // in-window foreign address never resolves to a canonical block at all
    // (`off >= span` / key-window miss guard) and no counter moves. Under
    // `alloc-xthread` + `alloc-global` the SAME unresolvable free is counted
    // in FOREIGN_OR_UNROUTABLE_FREES by design (the free tail), so assert
    // the configuration-appropriate outcome; LOCAL_REJECT stays flat in both.
    #[cfg(not(feature = "alloc-xthread"))]
    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees(),
        foreign_before,
        "both Large-path frees must stop at canonical_block_of's span/miss guard, never routed as foreign"
    );
    #[cfg(feature = "alloc-xthread")]
    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees(),
        foreign_before + 1,
        "the unresolvable in-window free must be counted exactly once by the routing tier"
    );
}
