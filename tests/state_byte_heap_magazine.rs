//! B4 variant (в) — STATE-BYTE TRANSITION TABLE (TS1), HeapCore/tcache edition.
//!
//! The MAG transitions commit in `src/registry/heap_core/` (own free push,
//! batch push, alloc-hit issue, refill-retained, deferred-batch issue). The
//! byte dump hook (`dbg_state_bytes_for`) exists only at the `AllocCore`
//! level, so this file pins those transitions through the HeapCore-level
//! state probes — `dbg_tcache_contains` (magazine residency ⇔ MAG per the
//! API semantics) and `dbg_is_free_for` (FREE-state decode of the byte) —
//! plus issue-exclusivity behavior. Byte-level encoding for the LIVE/FREE
//! commit sites is pinned in `state_byte_transitions.rs`; the MAG byte
//! encoding itself is pinned at `state_byte.rs`'s own contract and via the
//! class-cursor consumers.
//!
//!
//! Synthetic MAG-reclaim regressions use the existing unsafe test seam only
//! inside pre-existing unsafe test blocks; no additional unsafe syntax is
//! introduced by these probes.

#![cfg(all(
    all(feature = "alloc-global", feature = "fastbin"),
    feature = "internals",
    feature = "alloc-xthread",
    feature = "bench-internals",
    feature = "alloc-decommit"
))]

use core::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

/// G1-K7: heaps in this binary run with the depot overflow off (budget 0),
/// so a full magazine past the depot floor flushes to the substrate as these
/// checks expect; the overflow has its own binaries
/// (`tests/g1_k7_depot_overflow*.rs`). One config per binary.
fn claim_floor_only() -> Option<sefer_alloc::registry::HeapLease> {
    #[cfg(feature = "alloc-decommit")]
    {
        HeapRegistry::dbg_claim_lease_with_config(
            sefer_alloc::alloc_core::LargeCacheConfig::new().depot_budget_bytes(0),
        )
    }
    #[cfg(not(feature = "alloc-decommit"))]
    {
        HeapRegistry::dbg_claim_lease()
    }
}

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

const TCACHE_CAP: usize = 16;

fn l16() -> Layout {
    Layout::from_size_align(16, 8).unwrap()
}

/// B4 depot (DP-3): fill class 0's depot from fresh LIVE blocks and leave the
/// magazine empty, so the test's next full-magazine free takes the legacy
/// overflow arm exactly as before the depot. Call it only AFTER the test has
/// allocated its own blocks: any later magazine miss reloads from the depot.
/// Returns the still-live helper blocks; free them at the end of the test.
fn fill_depot_class0(heap: &mut HeapCore) -> Vec<*mut u8> {
    const DEPOT_K0: usize = 8;
    let mut warm = Vec::new();
    for _ in 0..TCACHE_CAP * (DEPOT_K0 + 1) {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        warm.push(q);
    }
    let mut freed = 0;
    while heap.dbg_depot_len(0) < DEPOT_K0 {
        assert!(freed < warm.len(), "depot never filled");
        // SAFETY: each warm block is live and freed exactly once.
        unsafe { heap.dealloc(warm[freed], l16()) };
        freed += 1;
    }
    let mut live = warm.split_off(freed);
    // The park that filled the depot left the current free alone in the
    // magazine; pop it (a magazine hit, no reload) to empty the magazine.
    while heap.dbg_tcache_count(0) > 0 {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        live.push(q);
    }
    assert_eq!(heap.dbg_depot_len(0), DEPOT_K0, "precondition: depot full");
    live
}

/// Frees the helper blocks returned by [`fill_depot_class0`].
fn release(heap: &mut HeapCore, blocks: &[*mut u8]) {
    for &q in blocks {
        // SAFETY: live helper block, freed exactly once.
        unsafe { heap.dealloc(q, l16()) };
    }
}

// ── MAG transitions ───────────────────────────────────────────────────────

/// own free push → MAG: a scalar own-thread free of a LIVE block parks it in
/// the magazine (MAG), NOT on a bin (not FREE).
/// Mutant "remove the MAG store in `dealloc_own_base`'s push": the SCALAR
/// push site is caught by `ts1_reject_in_magazine_double_free` HERE and by
/// `tests/regression_magazine_oracles.rs::{in_magazine_double_free_is_noop,
/// flushed_double_free_is_noop}` — `small_free_guard` requires the byte to
/// read exactly LIVE, so with the store gone the immediate double-free is
/// ACCEPTED (not a state reject) and the block ends up parked twice. The
/// dedicated store-site mutants have their own counterfactuals below:
/// `ts1_reject_overflow_push_double_free` (overflow-branch push, M9b),
/// `ts1_batch_push_duplicate_exactly_once` (batch push, M10) and
/// `ts1_refill_retained_stale_double_free` (refill-retained, M11).
#[test]
fn ts1_own_free_push_mag_then_hit_live() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l16());
    assert!(!p.is_null());
    let live_before = heap.dbg_live_count_for(p);
    let mag_before = heap.dbg_tcache_count(0);
    let class = heap.dbg_class_for(l16()).expect("issued class") as u8;
    // SAFETY: p is live and mapped; first free places it in MAG for direct reclaim probe.
    unsafe {
        heap.dealloc(p, l16());
        let state = heap.dbg_state_byte_state_for(p);
        let live = heap.dbg_live_count_for(p);
        let count = heap.dbg_tcache_count(0);
        let resident = heap.dbg_tcache_contains(0, p);
        assert!(!heap.dbg_reclaim_sidecar_record_for_test(p, class));
        assert_eq!(heap.dbg_state_byte_state_for(p), state);
        assert_eq!(heap.dbg_live_count_for(p), live);
        assert_eq!(heap.dbg_tcache_count(0), count);
        assert_eq!(heap.dbg_tcache_contains(0, p), resident);
    }
    assert_eq!(heap.dbg_state_byte_state_for(p), Some(1));
    assert_eq!(heap.dbg_live_count_for(p), live_before);
    assert_eq!(heap.dbg_tcache_count(0), mag_before + 1);
    assert!(heap.dbg_tcache_contains(0, p), "own free parks MAG");
    assert!(!heap.dbg_is_free_for(p));

    // alloc-hit → LIVE: the magazine hit reissues the same block and clears
    // its magazine residency.
    let q = heap.alloc(l16());
    assert_eq!(q, p, "magazine hit must reissue the parked block");
    assert!(
        !heap.dbg_tcache_contains(0, q),
        "issued block left the magazine"
    );
    assert!(!heap.dbg_is_free_for(q), "issued block must not read FREE");
    // Mutant canary for `clear_magazine_on_issue`: with the MAG→LIVE flip
    // removed the byte stays MAG and the NEXT free of the block is wrongly
    // swallowed by the LIVE-exactness oracle — the block would never come
    // back. So: free + reissue must work repeatedly.
    for _ in 0..3 {
        // SAFETY: live, single free each round.
        unsafe { heap.dealloc(q, l16()) };
        assert!(heap.dbg_tcache_contains(0, q));
        let r = heap.alloc(l16());
        assert_eq!(r, q, "hit must reissue the same block every round");
    }
    // SAFETY: live at loop exit.
    unsafe { heap.dealloc(q, l16()) };
    drop(lease);
}

/// zeroed-hit → LIVE: `alloc_zeroed` served from the magazine commits the
/// same MAG → LIVE issue as the plain hit.
#[test]
fn ts1_zeroed_hit_writes_live() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l16());
    assert!(!p.is_null());
    // SAFETY: live, single free.
    unsafe { heap.dealloc(p, l16()) };
    assert!(heap.dbg_tcache_contains(0, p));

    let q = heap.alloc_zeroed(l16());
    assert_eq!(q, p, "zeroed hit must reissue the parked block");
    assert!(!heap.dbg_tcache_contains(0, q));
    assert!(!heap.dbg_is_free_for(q), "zeroed hit must commit LIVE");
    // SAFETY: live.
    unsafe { heap.dealloc(q, l16()) };
    drop(lease);
}

/// overflow flush → FREE: parking more than TCACHE_CAP blocks flushes the
/// overflow to the substrate free list; flushed blocks read FREE, retained
/// ones stay MAG.
#[test]
fn ts1_overflow_flush_writes_free() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let n = TCACHE_CAP + 8;
    let mut ps = Vec::new();
    for _ in 0..n {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        ps.push(p);
    }
    let warm = fill_depot_class0(heap);
    for &p in &ps {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    let count = heap.dbg_tcache_count(0);
    assert!(
        count as usize <= TCACHE_CAP,
        "magazine over capacity: {count}"
    );
    let mut free_n = 0;
    let mut mag_n = 0;
    for &p in &ps {
        if heap.dbg_is_free_for(p) {
            free_n += 1;
        } else if heap.dbg_tcache_contains(0, p) {
            mag_n += 1;
        }
    }
    assert_eq!(free_n + mag_n, n, "every block is either FREE or MAG");
    assert_eq!(mag_n, count as usize, "MAG set == tcache residency");
    assert!(
        free_n >= 1,
        "the overflow must have flushed to the bin (FREE)"
    );

    // Flushed blocks are re-issued exactly once (free-list pop → LIVE).
    let mut re = Vec::new();
    for _ in 0..free_n + 1 {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        re.push(p);
    }
    let uniq = {
        let mut v = re.clone();
        v.sort_unstable();
        v.dedup();
        v.len()
    };
    assert_eq!(uniq, re.len(), "a flushed block was issued twice");
    for &p in &re {
        assert!(!heap.dbg_is_free_for(p), "re-issued block must be LIVE");
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    release(heap, &warm);
    drop(lease);
}

/// refill retained blocks → MAG: a magazine-miss refill retains
/// `refill_n - 1` blocks in the magazine (MAG) and issues exactly one (LIVE).
#[test]
fn ts1_refill_retained_writes_mag() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    // Drain the class-0 magazine completely (free N blocks, then re-issue
    // them all) so the next alloc is a genuine magazine miss → refill.
    let n = 64;
    let mut ps = Vec::new();
    for _ in 0..n {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        ps.push(p);
    }
    let drained_before = heap.dbg_tcache_count(0);
    for &p in &ps {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    // Re-issue everything (drains the magazine; overflow refills the bin).
    for _ in 0..n {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        // SAFETY: live, single free.
        unsafe { heap.dealloc(p, l16()) };
    }
    // Now drain from the bin side: issue until the tcache is empty, keeping
    // everything live.
    let mut live = Vec::new();
    let deadline = n * 4;
    while heap.dbg_tcache_count(0) > 0 && live.len() < deadline {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        live.push(p);
    }
    assert!(heap.dbg_tcache_count(0) == 0, "magazine must be empty now");
    let _ = drained_before;

    // Miss → refill: exactly one block issued to the caller, the retained
    // rest parked MAG in the magazine.
    let issued = heap.alloc(l16());
    assert!(!issued.is_null());
    let refill_n = heap.dbg_refill_n_for_class(0);
    if refill_n > 1 {
        assert_eq!(
            heap.dbg_tcache_count(0) as usize,
            refill_n - 1,
            "refill must retain refill_n - 1 blocks as MAG"
        );
    }
    assert!(!heap.dbg_is_free_for(issued), "issued block must be LIVE");
    // Every subsequent alloc is a magazine HIT from the retained MAG set.
    for _ in 0..4 {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        assert!(!heap.dbg_is_free_for(p));
        live.push(p);
    }
    for &p in &live {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    // SAFETY: issued still live.
    unsafe { heap.dealloc(issued, l16()) };
    drop(lease);
}

// ── MAG transitions: batch path (batch-api) ──────────────────────────────

/// batch push → MAG, then deferred batch clear → LIVE: `dealloc_batch`
/// parks its blocks magazine-resident (MAG, not FREE) in one commit; the
/// next `alloc_batch` issue clears them back to LIVE.
#[test]
#[cfg(feature = "batch-api")]
fn ts1_batch_push_mag_then_deferred_clear_live() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let n = 8;
    let mut buf: Vec<*mut u8> = vec![std::ptr::null_mut(); n];
    let filled = heap.alloc_batch(l16(), &mut buf);
    assert_eq!(filled, n);
    for &p in &buf {
        assert!(!p.is_null());
        // LIVE exactness: a block issued out of the batch carve transaction
        // must read LIVE (state bits 0) -  the FREE-state probe must be false.
        // COUNTERFACTUAL: corrupt the carve-batch LIVE commit
        // (alloc_core_small_impl.rs try_carve_batch) to FREE -> RED here (the
        // issued blocks read FREE while the caller holds them).
        assert!(!heap.dbg_is_free_for(p), "fresh batch blocks are LIVE");
    }

    // SAFETY: live blocks, single batch free.
    unsafe { heap.dealloc_batch(l16(), &buf) };
    for &p in &buf {
        assert!(
            !heap.dbg_is_free_for(p),
            "batch-pushed block must be MAG (deferred), not FREE"
        );
        assert!(
            heap.dbg_tcache_contains(0, p),
            "batch-pushed block must be magazine-resident"
        );
    }
    assert_eq!(heap.dbg_tcache_count(0) as usize, n);

    // Deferred batch clear: re-issuing via the batch commits MAG → LIVE.
    let mut buf2: Vec<*mut u8> = vec![std::ptr::null_mut(); n];
    let filled2 = heap.alloc_batch(l16(), &mut buf2);
    assert_eq!(filled2, n);
    for &p in &buf2 {
        assert!(!p.is_null());
        assert!(!heap.dbg_is_free_for(p), "re-issued batch blocks are LIVE");
        assert!(
            !heap.dbg_tcache_contains(0, p),
            "issued block left magazine"
        );
        assert!(!heap.dbg_is_free_for(p), "issued block must be LIVE");
    }
    // SAFETY: live blocks, single batch free.
    unsafe { heap.dealloc_batch(l16(), &buf2) };
    drop(lease);
}

// ── rejects: no-op + unchanged observed state ────────────────────────────

/// in-magazine double free: second free is a no-op — the block stays parked
/// exactly once (MAG set unchanged) and is issued at most once afterwards.
#[test]
fn ts1_reject_in_magazine_double_free() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l16());
    assert!(!p.is_null());
    // SAFETY: deliberate double-free probe; synthetic records may be MAG and
    // are rejected without changing the owner magazine or live accounting.
    let class = heap.dbg_class_for(l16()).expect("issued block class") as u8;
    let live_before = heap.dbg_live_count_for(p);
    unsafe {
        heap.dealloc(p, l16());
    }
    let mag_before = heap.dbg_tcache_count(0);
    unsafe {
        assert!(!heap.dbg_reclaim_sidecar_record_for_test(p, class));
    }
    assert_eq!(heap.dbg_state_byte_state_for(p), Some(1));
    assert_eq!(heap.dbg_live_count_for(p), live_before);
    assert_eq!(heap.dbg_tcache_count(0), mag_before);
    assert!(heap.dbg_tcache_contains(0, p));
    assert_eq!(
        heap.alloc(l16()),
        p,
        "cleanup/reissue keeps canonical block"
    );
    // SAFETY: reissued live allocation.
    unsafe { heap.dealloc(p, l16()) };
    let cnt = heap.dbg_tcache_count(0);
    assert!(heap.dbg_tcache_contains(0, p));
    // Double free while MAG — must be swallowed.
    // SAFETY: deliberate double-free probe.
    unsafe { heap.dealloc(p, l16()) };
    assert_eq!(
        heap.dbg_tcache_count(0),
        cnt,
        "double free changed the magazine residency set"
    );
    let p1 = heap.alloc(l16());
    let p2 = heap.alloc(l16());
    assert_ne!(p1, p2, "in-magazine double free issued the block twice");
    // SAFETY: live, single free each.
    unsafe {
        heap.dealloc(p1, l16());
        heap.dealloc(p2, l16());
    }
    drop(lease);
}

/// double free AFTER flush: the flushed (FREE) block's second free is a
/// no-op — it stays on the bin exactly once and is issued at most once.
#[test]
fn ts1_reject_double_free_after_flush() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let n = TCACHE_CAP + 8;
    let mut ps = Vec::new();
    for _ in 0..n {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        ps.push(p);
    }
    let warm = fill_depot_class0(heap);
    for &p in &ps {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    let flushed: Vec<*mut u8> = ps
        .iter()
        .copied()
        .filter(|&p| heap.dbg_is_free_for(p))
        .collect();
    assert!(!flushed.is_empty());
    let f = flushed[0];
    // Double free after flush — must be swallowed by the FREE-state oracle.
    // SAFETY: deliberate double-free probe on a flushed block.
    unsafe { heap.dealloc(f, l16()) };
    assert!(
        heap.dbg_is_free_for(f),
        "flushed block must still read FREE"
    );
    // B4 depot: take the magazine and the depot out first (blocks stay LIVE),
    // so the storm below refills from the bin holding `f`, as before the depot.
    let mut drained = Vec::new();
    while heap.dbg_tcache_count(0) > 0 || heap.dbg_depot_len(0) > 0 {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        drained.push(q);
    }
    // The block is issued at most once in the following reissue storm.
    let mut re = Vec::new();
    for _ in 0..flushed.len() + 4 {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        re.push(p);
    }
    let mut sorted = re.clone();
    sorted.sort_unstable();
    sorted.dedup();
    let uniq = sorted.len();
    assert_eq!(
        uniq,
        re.len(),
        "double-free-after-flush issued a block twice"
    );
    for &p in &re {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    release(heap, &drained);
    release(heap, &warm);
    drop(lease);
}

/// wrong class (aligned start of ANOTHER class): the own-free oracle
/// requires the byte to carry THIS class's class field and LIVE state —
/// a 64B-class free of a 16B block is rejected, the block stays issued
/// (LIVE) and a free with the CORRECT class afterwards still works.
#[test]
// F12 (chunk 3): production too — the wrong-class reject is the state byte's
// CLASS-FIELD compare in `small_free_guard`, UNCONDITIONAL since F1/F4 (not
// a `hardened`-only H1 semantics), so the same counterfactual runs in every
// configuration.
fn ts1_reject_wrong_class_stays_live() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let l64 = Layout::from_size_align(64, 8).unwrap();
    let p = heap.alloc(l16());
    assert!(!p.is_null());
    // Wrong-class free: 64B layout over a 16B block start.
    // SAFETY: deliberate reject probe.
    unsafe { heap.dealloc(p, l64) };
    assert!(
        !heap.dbg_is_free_for(p),
        "wrong-class free must not commit FREE"
    );
    assert!(
        !heap.dbg_tcache_contains(0, p),
        "wrong-class free must not park the block in the class-0 magazine"
    );
    // The block is still live and its correct-class free works.
    // SAFETY: live, single free.
    unsafe { heap.dealloc(p, l16()) };
    assert!(heap.dbg_tcache_contains(0, p), "correct free parks MAG");
    // Reclaim the parked block for cleanup.
    let cleanup = heap.alloc(l16());
    // SAFETY: live or freshly parked; single free.
    unsafe { heap.dealloc(cleanup, l16()) };
    drop(lease);
}

/// unaligned (interior) pointer through the heap free face: hardened guard
/// rejects as a no-op, the real block stays issued and reusable.
#[test]
#[cfg(feature = "hardened")]
fn ts1_reject_unaligned_interior() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l16());
    assert!(!p.is_null());
    let interior = unsafe { p.add(8) };
    // SAFETY: deliberate reject probe.
    unsafe { heap.dealloc(interior, l16()) };
    assert!(!heap.dbg_is_free_for(interior));
    assert!(!heap.dbg_is_free_for(p), "real block must stay LIVE");
    assert!(!heap.dbg_tcache_contains(0, p), "nothing was parked");
    // Reissue exclusivity: the next two allocs are distinct.
    let a = heap.alloc(l16());
    let b = heap.alloc(l16());
    assert_ne!(a, b);
    // SAFETY: live, single free each.
    unsafe {
        heap.dealloc(a, l16());
        heap.dealloc(b, l16());
        heap.dealloc(p, l16());
    }
    drop(lease);
}

/// foreign free through the heap face: a pointer of another heap is either
/// routed back to its owner (cross-thread protocol) or rejected — in BOTH
/// cases it must be accounted EXACTLY ONCE in the owner: the block is never
/// issued twice afterwards, and heap B never reports it as its own free.
#[test]
fn ts1_reject_foreign_single_accounting() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease_a = HeapRegistry::dbg_claim_lease().expect("claim A");
    let mut lease_b = HeapRegistry::dbg_claim_lease().expect("claim B");
    let ha = lease_a.core();
    let hb = lease_b.core();

    let p = ha.alloc(l16());
    assert!(!p.is_null());
    // Foreign free into heap B.
    // SAFETY: deliberate foreign-free probe — `p` belongs to heap A.
    unsafe { hb.dealloc(p, l16()) };
    assert!(
        !hb.dbg_is_free_for(p),
        "heap B must not own/report p as FREE"
    );

    // The owner's accounting is exact: in the following reissue storm `p`
    // appears AT MOST ONCE (zero times if the routed free was reclaimed,
    // once if the foreign free was rejected — never twice).
    let mut re = Vec::new();
    for _ in 0..24 {
        let q = ha.alloc(l16());
        assert!(!q.is_null());
        re.push(q);
    }
    let count = re.iter().filter(|&&q| q == p).count();
    assert!(
        count <= 1,
        "foreign free double-accounted: p issued {count}x"
    );
    for &q in &re {
        // SAFETY: live, single free each.
        unsafe { ha.dealloc(q, l16()) };
    }
    drop(lease_b);
    drop(lease_a);
}

// ── MAG store-site mutants (M9b / M10 / M11) ──────────────────────────────

/// M9b (review): the OVERFLOW-branch push (magazine at cap → half-flush +
/// compact + push) must commit the MAG store like every other push arm.
/// COUNTERFACTUAL: drop the `meta.state_byte().write(off, c, MAG)` in
/// `dealloc_own_thread_with_base`'s overflow arm → the pushed block's byte
/// stays LIVE → the immediate double-free is ACCEPTED by the LIVE-exact
/// guard → the magazine gains a second residency for `p` (count moves) and
/// the storm below reissues `p` twice. RED under the mutant.
#[test]
fn ts1_reject_overflow_push_double_free() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    // Drain whatever earlier tests in this binary left in the magazine
    // (blocks stay LIVE so the count really reaches 0).
    let mut held = Vec::new();
    while heap.dbg_tcache_count(0) > 0 {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        held.push(q);
    }

    // Fill the magazine to EXACTLY its free-side cap: 24 fresh blocks, the
    // first 16 of them freed one by one (the 16th free pushes at cnt==15 —
    // NOT the overflow branch yet).
    let mut ps = Vec::new();
    for _ in 0..24 {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        ps.push(q);
    }
    let warm = fill_depot_class0(heap);
    for q in &ps[..TCACHE_CAP] {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(*q, l16()) };
    }
    assert_eq!(heap.dbg_tcache_count(0) as usize, TCACHE_CAP);

    // Free `ps[TCACHE_CAP]` at full capacity → the OVERFLOW branch
    // (half-flush + compact + push).
    let p = ps[TCACHE_CAP];
    let class = heap.dbg_class_for(l16()).expect("issued class") as u8;
    let mag_state_after_free;
    let live_after_first;
    let mag_count_after_first;
    // SAFETY: live p is freed into MAG, then the synthetic record probes it directly.
    unsafe {
        heap.dealloc(p, l16());
        mag_state_after_free = heap.dbg_state_byte_state_for(p);
        live_after_first = heap.dbg_live_count_for(p);
        mag_count_after_first = heap.dbg_tcache_count(0);
        let mag_state = heap.dbg_state_byte_state_for(p);
        let live = heap.dbg_live_count_for(p);
        let count = heap.dbg_tcache_count(0);
        let resident = heap.dbg_tcache_contains(0, p);
        assert!(!heap.dbg_reclaim_sidecar_record_for_test(p, class));
        assert_eq!(heap.dbg_state_byte_state_for(p), mag_state);
        assert_eq!(heap.dbg_live_count_for(p), live);
        assert_eq!(heap.dbg_tcache_count(0), count);
        assert_eq!(heap.dbg_tcache_contains(0, p), resident);
    }
    assert_eq!(heap.dbg_live_count_for(p), live_after_first);
    assert_eq!(heap.dbg_state_byte_state_for(p), mag_state_after_free);
    assert!(heap.dbg_tcache_contains(0, p));
    assert_eq!(heap.dbg_tcache_count(0), mag_count_after_first);
    // Duplicate scalar free while MAG must remain a no-op.
    // SAFETY: deliberate duplicate free of owner MAG block.
    unsafe { heap.dealloc(p, l16()) };
    assert_eq!(heap.dbg_state_byte_state_for(p), Some(1));
    assert_eq!(heap.dbg_live_count_for(p), live_after_first);
    assert_eq!(heap.dbg_tcache_count(0), mag_count_after_first);

    // `p` is issued AT MOST once afterwards.
    let mut seen = 0usize;
    let mut live = Vec::new();
    for _ in 0..32 {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        if q == p {
            seen += 1;
        }
        live.push(q);
    }
    assert!(seen <= 1, "overflow-push double free reissued `p` {seen}x");
    for &q in &live {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(q, l16()) };
    }
    for &q in &held {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(q, l16()) };
    }
    release(heap, &warm);
    drop(lease);
}

#[test]
#[cfg(feature = "batch-api")]
fn ts1_batch_push_duplicate_exactly_once() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    // (a) single-block batch free, then probe reclaim while p is MAG.
    // The registry heap is process-global: drain whatever earlier tests in
    // this binary left in the magazine so the count precondition holds.
    let mut drained = Vec::new();
    while heap.dbg_tcache_count(0) > 0 {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        drained.push(q);
    }
    let p = heap.alloc(l16());
    assert!(!p.is_null());
    // NOTE: `p`'s alloc may itself be a magazine MISS (refill parks up to
    // cap-1 blocks), so the baseline is measured AFTER the alloc.
    let c0 = heap.dbg_tcache_count(0);
    let live_before = heap.dbg_live_count_for(p);
    assert!(
        (c0 as usize) < TCACHE_CAP,
        "precondition: magazine not full"
    );
    let mag_before = heap.dbg_tcache_count(0);
    let class = heap.dbg_class_for(l16()).expect("l16 class") as u8;
    // SAFETY: live mapped block; batch free creates MAG before synthetic reclaim.
    unsafe {
        heap.dealloc_batch(l16(), &[p]);
        let mag_state = heap.dbg_state_byte_state_for(p);
        let mag_count = heap.dbg_tcache_count(0);
        let resident = heap.dbg_tcache_contains(0, p);
        let live = heap.dbg_live_count_for(p);
        assert!(!heap.dbg_reclaim_sidecar_record_for_test(p, class));
        assert_eq!(heap.dbg_state_byte_state_for(p), mag_state);
        assert_eq!(heap.dbg_tcache_count(0), mag_count);
        assert_eq!(heap.dbg_tcache_contains(0, p), resident);
        assert_eq!(heap.dbg_live_count_for(p), live);
    }
    assert_eq!(heap.dbg_state_byte_state_for(p), Some(1));
    assert_eq!(heap.dbg_live_count_for(p), live_before);
    assert_eq!(heap.dbg_tcache_count(0), mag_before + 1);
    assert!(heap.dbg_tcache_contains(0, p));
    // SAFETY: duplicate batch free of an owner-MAG block is the tested no-op.
    unsafe {
        heap.dealloc_batch(l16(), &[p, p]);
    }
    assert_eq!(heap.dbg_tcache_count(0), mag_before + 1);

    // (b) duplicate batch entry `[p, p]` → exactly ONE residency. Drain the
    // magazine again (kept live) so the count precondition holds.
    for &q in &drained {
        let r = heap.alloc(l16());
        assert_eq!(r, q, "drained blocks must reissue");
    }
    while heap.dbg_tcache_count(0) > 0 {
        let q = heap.alloc(l16());
        assert!(!q.is_null());
        drained.push(q);
    }
    let p2 = heap.alloc(l16());
    assert!(!p2.is_null());
    let c1 = heap.dbg_tcache_count(0);
    assert!(
        (c1 as usize) < TCACHE_CAP,
        "precondition: magazine not full"
    );
    // SAFETY: live block; the duplicate entry is the deliberate contract-
    // violation probe under test (must degrade to exactly one push).
    unsafe { heap.dealloc_batch(l16(), &[p2, p2]) };
    assert_eq!(
        heap.dbg_tcache_count(0),
        c1 + 1,
        "duplicate batch entry must add exactly one residency"
    );
    // Cleanup: free everything held live.
    // SAFETY: live blocks, single batch free.
    unsafe { heap.dealloc_batch(l16(), &drained) };
    drop(lease);
}

/// M11 (review): the REFILL-retained tail (`finish_magazine_refill`) must
/// commit MAG for the `n - 1` blocks retained in the magazine. COUNTERFACTUAL:
/// drop the MAG loop in `finish_magazine_refill` → a retained block's byte
/// still reads LIVE (the drain committed LIVE at pop) → the stale free of a
/// magazine-resident block is ACCEPTED by the LIVE-exact guard → count moves
/// and the block is parked twice (storm reissues it twice).
#[test]
fn ts1_refill_retained_stale_double_free() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();

    // Build the miss state: free 64 blocks (magazine caps at 16, the rest
    // flush FREE to the bins), then issue all 16 back (magazine empty,
    // they stay LIVE as the anchor set).
    let n = 64;
    let mut ps = Vec::new();
    for _ in 0..n {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        ps.push(p);
    }
    for &p in &ps {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    let mut anchor = Vec::new();
    for _ in 0..TCACHE_CAP {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        anchor.push(p);
    }
    assert_eq!(heap.dbg_tcache_count(0), 0, "magazine must be empty");

    // Miss → refill: freed blocks return into slots[0..n-1] as MAG.
    let issued = heap.alloc(l16());
    assert!(!issued.is_null());
    let resident: Vec<*mut u8> = ps
        .iter()
        .copied()
        .filter(|&p| heap.dbg_tcache_contains(0, p))
        .collect();
    assert!(!resident.is_empty(), "refill must retain freed blocks");
    let cnt = heap.dbg_tcache_count(0);

    // Stale free of a retained (MAG) block → no-op; count unchanged.
    let q = resident[0];
    let class = heap.dbg_class_for(l16()).expect("l16 class") as u8;
    let live_before = heap.dbg_live_count_for(q);
    let mag_before = heap.dbg_tcache_count(0);
    // SAFETY: q is an owner-MAG block in a mapped segment; probe real reclaim.
    unsafe {
        let state = heap.dbg_state_byte_state_for(q);
        let live = heap.dbg_live_count_for(q);
        let count = heap.dbg_tcache_count(0);
        let is_resident = heap.dbg_tcache_contains(0, q);
        assert!(!heap.dbg_reclaim_sidecar_record_for_test(q, class));
        assert_eq!(heap.dbg_state_byte_state_for(q), state);
        assert_eq!(heap.dbg_live_count_for(q), live);
        assert_eq!(heap.dbg_tcache_count(0), count);
        assert_eq!(heap.dbg_tcache_contains(0, q), is_resident);
    }
    assert_eq!(heap.dbg_state_byte_state_for(q), Some(1));
    assert_eq!(heap.dbg_live_count_for(q), live_before);
    assert_eq!(heap.dbg_tcache_count(0), mag_before);
    assert!(heap.dbg_tcache_contains(0, q));
    assert_eq!(heap.dbg_tcache_count(0), cnt);

    // `q` is issued AT MOST once afterwards.
    let mut seen = 0usize;
    let mut live = Vec::new();
    for _ in 0..32 {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        if p == q {
            seen += 1;
        }
        live.push(p);
    }
    assert!(
        seen <= 1,
        "refill-retained double free reissued `q` {seen}x"
    );
    for &p in &live {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    for &p in &anchor {
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
    }
    // SAFETY: live, single free.
    unsafe { heap.dealloc(issued, l16()) };
    drop(lease);
}
