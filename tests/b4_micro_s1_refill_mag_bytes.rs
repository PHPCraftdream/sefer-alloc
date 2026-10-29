//! B4-M / S1 — magazine refill commits MAG at the substrate (parameterized
//! drain/carve), `finish_magazine_refill` flips ONLY the issued block to LIVE.
//!
//! Checks:
//! 1. Magazine-miss refill: the issued block's state byte is LIVE (0) — the
//!    refill tail flipped MAG → LIVE exactly once (`dbg_state_byte_state_for`).
//!    A `debug_assert` in `finish_magazine_refill` additionally pins that the
//!    issued block ARRIVES MAG (so a mutant reverting the drain to LIVE-bytes
//!    panics in the debug test build instead of passing silently).
//! 2. Magazine HIT issues of previously-parked blocks still end LIVE
//!    (MAG → LIVE via `clear_magazine_on_issue` — unchanged behaviour).
//! 3. Batch-API paths stay LIVE: `dbg_carve_batch` / `dbg_drain_freelist_batch`
//!    (the direct-to-user batch surface) leave LIVE bytes, not MAG.
//! 4. Retained-set invariant: after the miss-refill, of the drained `live`
//!    addresses (the very blocks the refill drains from the bins) EXACTLY
//!    ONE reads LIVE — the issued block itself — and all others read MAG.
//! 5. Counter oracle (P1-2, needs `bench-internals`): the substrate refill
//!    wrote MAG to exactly `retained` state bytes (`S1_MAG_BYTE_WRITES`
//!    delta == retained == refill_n_after − refill_n_before − 1). Under the
//!    FULL mutant (drain LIVE + the old `finish_magazine_refill` MAG loop)
//!    the end-state bytes are IDENTICAL, but the counter delta drops to 0
//!    ≠ retained — the counter is what makes the full mutant red (the
//!    partial mutant M-h is caught by the `debug_assert` in
//!    `finish_magazine_refill` instead).
//!
//! Counterfactual mutants (run manually):
//! - M-h (partial): revert `refill_class_bump_state_internal`'s MAG to LIVE
//!   (old behaviour). Then the `debug_assert_eq!(.., MAG, ..)` in
//!   `finish_magazine_refill` fires → test goes red in the debug build.
//! - FULL mutant (drain LIVE + the old `finish_magazine_refill` MAG loop,
//!   as in `git show HEAD:src/registry/heap_core/alloc/hot.rs`): final state
//!   bytes are identical, but `S1_MAG_BYTE_WRITES` delta is 0 ≠ retained →
//!   check 5 goes red.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "alloc-xthread",
    feature = "bench-internals",
    feature = "alloc-decommit",
    feature = "alloc-core"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

const LIVE: u8 = 0; // StateByte::LIVE (issued-to-user state, B4 encoding)

fn l16() -> Layout {
    Layout::from_size_align(16, 8).unwrap()
}

/// B4 depot (DP-3): fill class 0's depot from fresh LIVE blocks and leave the
/// magazine empty, so later full-magazine frees take the legacy overflow arm.
/// Returns the still-live helper blocks.
/// G1-K7: every heap here has the depot overflow off (budget 0), so a full
/// floor routes frees to the bins exactly as these checks expect. One config
/// per binary.
fn claim_floor_only() -> Option<sefer_alloc::registry::HeapLease> {
    HeapRegistry::dbg_claim_lease_with_config(
        sefer_alloc::alloc_core::LargeCacheConfig::new().depot_budget_bytes(0),
    )
}

fn fill_depot_class0(heap: &mut HeapCore) -> Vec<*mut u8> {
    const DEPOT_K0: usize = 8;
    let mut warm = Vec::new();
    for _ in 0..16 * (DEPOT_K0 + 1) {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        warm.push(q);
    }
    let mut freed = 0;
    while heap.dbg_depot_len(0) < DEPOT_K0 {
        assert!(freed < warm.len(), "depot never filled");
        unsafe { heap.dealloc(warm[freed], l16()) };
        freed += 1;
    }
    let mut live = warm.split_off(freed);
    while heap.dbg_tcache_count(0) > 0 {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        live.push(q);
    }
    live
}

/// Allocate `2 * cap` blocks and free them all: the first `cap` frees park
/// in the magazine, the rest route straight to the substrate bins (R1-01
/// park cap). Then drain the magazine (those blocks are held LIVE by the
/// caller) — the tcache is empty and the bins hold EXACTLY the second half.
/// Returns both address sets; the miss-refill below drains precisely the
/// binned half, so `issued` is guaranteed to be one of them.
///
/// B4 depot (DP-3): the depot is filled before the frees (so they route as
/// without a depot) and emptied after the magazine drain (so the next miss is
/// a genuine substrate refill); its blocks are returned held-live (4th item).
fn park_half_bin_half(heap: &mut HeapCore) -> (Vec<*mut u8>, Vec<*mut u8>, usize, Vec<*mut u8>) {
    let cap = heap.dbg_free_park_cap(0).expect("small class 0");
    let n = 2 * cap;
    let mut ps = Vec::new();
    for _ in 0..n {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        ps.push(p);
    }
    let mut depot_held = fill_depot_class0(heap);
    for &p in &ps {
        unsafe { heap.dealloc(p, l16()) };
    }
    // Drain the magazine: its `cap` parked blocks come back as held-live.
    let mut live = Vec::new();
    while heap.dbg_tcache_count(0) > 0 {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        live.push(p);
    }
    assert_eq!(heap.dbg_tcache_count(0), 0, "magazine must be empty now");
    while heap.dbg_tcache_count(0) > 0 || heap.dbg_depot_len(0) > 0 {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        depot_held.push(p);
    }
    (ps, live, cap, depot_held)
}

/// S1: refill-issued block is LIVE; hits from the retained set are LIVE.
#[test]
fn s1_refill_issued_live_retained_parked() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let (ps, live, _cap, depot_held) = park_half_bin_half(heap);

    // Counter window (P1-2): only the miss-refill below may write MAG bytes.
    let mag_writes_before = HeapCore::dbg_s1_mag_byte_writes();

    // Miss → refill. The one issued block must read LIVE and must be one of
    // the binned addresses (the exact set the refill drains from the bins).
    let issued = heap.alloc(l16());
    assert!(!issued.is_null());
    assert_eq!(
        heap.dbg_state_byte_state_for(issued),
        Some(LIVE),
        "refill-issued block must be flipped MAG → LIVE exactly once"
    );

    // Retained-set invariant over the freed set `ps` (the only blocks ever
    // freed on this heap, so the refill's bin drain draws from them): after
    // the refill, among `ps \ live` (blocks NOT held live by us) EXACTLY ONE
    // reads LIVE — the issued block — every parked one reads MAG (count ==
    // the tcache depth the refill left), and the rest read FREE (still in
    // the bins). The held `live` half stays LIVE by construction.
    assert!(
        ps.contains(&issued),
        "the refill-issued block must be one of the freed bin blocks"
    );
    let mut live_reads = 0usize;
    let mut mag_reads = 0usize;
    for &p in &ps {
        if live.contains(&p) {
            continue;
        }
        match heap.dbg_state_byte_state_for(p) {
            Some(LIVE) => live_reads += 1,
            Some(1) => mag_reads += 1, // StateByte::MAG — parked by the refill
            Some(2) => {}              // FREE — still in the bins
            other => panic!("unexpected state byte {other:?} for freed block {p:?}"),
        }
    }
    assert_eq!(
        live_reads, 1,
        "exactly one drained block (the issued one) may read LIVE"
    );
    assert_eq!(
        mag_reads,
        heap.dbg_tcache_count(0) as usize,
        "every magazine-resident drained block must read MAG"
    );
    for &p in &live {
        assert_eq!(
            heap.dbg_state_byte_state_for(p),
            Some(LIVE),
            "held blocks must stay LIVE"
        );
    }

    // Counter oracle: the substrate committed MAG itself, once per refilled
    // block (`dbg_refill_n_for_class` is the pure formula the miss path uses;
    // the park budget is `FREE_PARK_CAP`, equal by PF-7). All n blocks get a
    // MAG write at the substrate (the issued one is then flipped to LIVE by
    // the tail's single `set_state`, which is not counted here); retained =
    // n−1. The FULL mutant (drain LIVE + the old tail MAG loop) leaves the
    // end-state bytes identical but drives this delta to 0 → red.
    let refill_n = heap.dbg_refill_n_for_class(0);
    let cap = heap.dbg_free_park_cap(0).expect("small class 0");
    assert_eq!(refill_n, cap, "PF-7: cap == refill_n for the class");
    let retained = refill_n - 1;
    assert!(
        retained >= 1,
        "cap >= 2 expected (FREE_PARK_CAP for the class)"
    );
    assert_eq!(
        HeapCore::dbg_s1_mag_byte_writes() - mag_writes_before,
        refill_n as u64,
        "substrate refill must commit MAG itself, once per refilled block"
    );

    // Subsequent magazine HITs (from the retained MAG set) also end LIVE.
    let refill_n = heap.dbg_refill_n_for_class(0);
    let hits = refill_n.saturating_sub(1).clamp(1, 4);
    let mut extra = Vec::new();
    for _ in 0..hits {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        assert_eq!(
            heap.dbg_state_byte_state_for(p),
            Some(LIVE),
            "magazine HIT issue must commit MAG → LIVE"
        );
        extra.push(p);
    }

    for &p in &extra {
        unsafe { heap.dealloc(p, l16()) };
    }
    for &p in &live {
        unsafe { heap.dealloc(p, l16()) };
    }
    for &p in &depot_held {
        unsafe { heap.dealloc(p, l16()) };
    }
}

/// S1 does not touch the batch-API paths: a direct `carve_batch` and a direct
/// freelist drain still leave LIVE bytes on every handed-out block.
#[test]
fn s1_batch_api_paths_stay_live() {
    let mut core = sefer_alloc::AllocCore::new().unwrap();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let c = core
        .dbg_layout_class_for(layout)
        .expect("expected a small class");

    // carve_batch (fresh bump run) → LIVE bytes.
    let mut buf = [core::ptr::null_mut::<u8>(); 8];
    let n = core.dbg_carve_batch(c, &mut buf);
    assert!(n >= 1);
    for &p in &buf[..n] {
        assert_eq!(
            core.dbg_state_byte_state_for(p),
            Some(LIVE),
            "batch-API carve must leave LIVE bytes"
        );
    }

    // Free them all (→ bins), then a direct drain_freelist_batch → LIVE.
    for &p in &buf[..n] {
        unsafe { core.dealloc(p, layout) };
    }
    let mut out = [core::ptr::null_mut::<u8>(); 8];
    let got = unsafe { core.dbg_drain_freelist_batch(buf[0], c, &mut out) };
    assert!(got >= 1, "drain must find the freed blocks");
    for &p in &out[..got] {
        assert_eq!(
            core.dbg_state_byte_state_for(p),
            Some(LIVE),
            "batch-API drain must leave LIVE bytes"
        );
    }
}
