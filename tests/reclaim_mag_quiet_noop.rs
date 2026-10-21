//! Round D1′ fix (reclaim item a): the MAG reject lane in
//! `alloc_core_small_reclaim` is a quiet COUNTED no-op in EVERY build —
//! the `#[cfg(feature = "fastbin")]` gate is gone.
//!
//! Mutants (each RED here):
//!  * restore the `fastbin` gate → in a no-`fastbin` build the MAG record
//!    falls through to the geometry/bin path (or fatals) — the drain count
//!    is no longer 0 / the process aborts → this test goes red.
//!  * delete the MAG branch (or `return accept`) → the corrupted MAG record
//!    is binned and its byte committed FREE in ANY build → the drain count
//!    is 1 and the MAG counter delta is 0 → red.
//!
//! The state-byte address is located by a transition oracle (the same
//! pattern as `oneclass_mutant_battery::child_overlap_claim_ml`): after a
//! reclaim the block's byte flips to exactly one window byte carrying
//! `encode(c, FREE)`.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;

const SEGMENT: usize = SegmentLayout::SEGMENT;
const BIG: usize = 64 * 1024;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn l(size: usize) -> Layout {
    Layout::from_size_align(size, 16).unwrap()
}

/// SAFETY: `p` is the caller's live issued allocation, transferred exactly
/// once (deliberate negative-probe shape from `oneclass_mutant_battery`).
unsafe fn try_publish(p: *mut u8) -> bool {
    let Some(pin) = RouteDirectory::global().lookup(p) else {
        return false;
    };
    if !matches!(pin.kind(), RouteKind::Small | RouteKind::Primordial) {
        return false;
    }
    let offset = (p.addr() & (SEGMENT - 1)) as u32;
    unsafe { pin.publish_small(offset) }
}

/// Force a genuinely fresh ordinary Small segment and return its FIRST
/// carved block (T4 pattern) — a virgin state-byte window for the oracle.
fn first_block_of_fresh_small_segment(heap: &mut HeapCore) -> *mut u8 {
    let reserved_before = sefer_alloc::AllocCore::dbg_segments_reserved_total();
    let mut a = None;
    let mut keep = Vec::new();
    for _ in 0..20_000 {
        let p = heap.alloc(l(BIG));
        assert!(!p.is_null());
        keep.push(p);
        if sefer_alloc::AllocCore::dbg_segments_reserved_total() > reserved_before {
            a = Some(p);
            break;
        }
    }
    let a = a.expect("a fresh Small segment must form within 20k allocs");
    keep.shrink_to_fit();
    core::mem::forget(keep); // the lease keeps them alive; never freed below
    a
}

#[test]
fn mag_record_is_a_quiet_counted_noop_in_every_build() {
    let _guard = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    // The oracle block IS the fresh segment's first BIG carve — its class
    // comes from the BIG layout, not from a 64-byte layout.
    let c64 = SegmentLayout::class_for(BIG, 16).expect("BIG is a small class") as u8;
    let live64 = c64 + 1; // encode(c64, LIVE)
    let free64 = (c64 + 1) | 0x80; // encode(c64, FREE)
    let mag64 = (c64 + 1) | 0x40; // encode(c64, MAG)

    let a = first_block_of_fresh_small_segment(heap);
    let base = a.addr() & !(SEGMENT - 1);
    let off_a = (a.addr() - base) as u32;

    // Legitimate reclaim once: the byte flips LIVE → encode(c64, FREE) and
    // the start bit survives (monotone).
    // Transition oracle: BEFORE the reclaim, exactly one window byte carries
    // encode(c64, LIVE); the reclaim must flip exactly THAT byte to
    // encode(c64, FREE).
    const WIN: usize = SegmentLayout::SMALL_META_END + 8192;
    let win = |addr: usize| -> u8 {
        // SAFETY: the lease exclusively owns the mapped segment; the window
        // covers metadata and the committed first payload chunk only.
        unsafe { (base as *const u8).byte_add(addr).read_volatile() }
    };
    // SAFETY: a is live and transferred exactly once.
    assert!(unsafe { try_publish(a) });
    let live_at: Vec<usize> = (0..WIN).filter(|&x| win(x) == live64).collect();
    assert_eq!(
        live_at.len(),
        1,
        "fixture: exactly one byte carries encode(c64, LIVE)"
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1, "reclaimed once");
    let a_true = live_at[0];
    assert_eq!(
        win(a_true),
        free64,
        "fixture: the reclaimed byte flipped to encode(c64, FREE)"
    );

    // Re-arm the record: pending bit set, start bit still set, byte now
    // deliberately corrupted to encode(c64, MAG).
    // SAFETY: deliberate stale-record probe; the verdict on the corrupted
    // byte is the behavior under test.
    assert!(unsafe { try_publish(a) }, "start bit survives the reclaim");
    // SAFETY: single-byte write into the lease-owned segment metadata; the
    // decisive reclaim runs before any other access to that byte.
    unsafe { (base as *mut u8).byte_add(a_true).write_volatile(mag64) };

    let (u0, f0, m0, _, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();
    let drained = heap.dbg_drain_sidecar_ingress();
    let (u1, f1, m1, _, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();
    assert_eq!(
        drained, 0,
        "MAG RED: the MAG record was accepted (binned), not quietly rejected"
    );
    assert!(
        m1 > m0,
        "MAG RED: REMOTE_REJECT_MAG did not count the quiet reject \
         (counters u{u0}->{u1} f{f0}->{f1} m{m0}->{m1})"
    );
    assert_eq!(u1, u0, "no UNISSUED fatal/counter activity");
    assert_eq!(f1, f0, "no FREE-reject activity on a MAG record");
    assert_eq!(
        heap.dbg_state_byte_state_for(a),
        Some(1),
        "the MAG byte is untouched by the quiet no-op"
    );
    let _ = off_a;
    // SAFETY: the lease drop tolerates the stale counted record (the same
    // shape as the detached-magazine probes).
    drop(lease);
}
