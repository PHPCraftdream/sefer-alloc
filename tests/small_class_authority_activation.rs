//! B4 У1 activation oracle (ADR §9(д), extended for chunk U1b): routed own
//! Small frees must run the class-authority check on ≥95 % of frees, the
//! check must go through the CACHED `ClassLeaves` handle (ADR §4.1 (в)
//! transport) on ≥95 % of routed frees, and both fallback numerators —
//! case-в fallback (missing/stale cached handle while the route is alive)
//! and case-а unrouted — must be 0 in the standard scenario. Prints the
//! counters.
#![cfg(all(
    feature = "production",
    feature = "small-class-authority",
    feature = "small-class-authority-activation"
))]

use std::alloc::Layout;
use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::AllocCore;

#[test]
fn small_class_authority_activation_at_least_95_percent() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    const N: usize = 1000;
    let l = Layout::from_size_align(16, 8).unwrap();
    let (checks0, fallback0, unrouted0) = AllocCore::dbg_class_authority_activation_counts();
    let mut ptrs = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: valid layout on the calling thread's own heap slot.
        let p = heap.alloc(l);
        assert!(!p.is_null());
        ptrs.push(p);
    }
    for &p in &ptrs {
        // SAFETY: valid routed own Small free.
        unsafe { heap.dealloc(p, l) };
    }
    let (checks1, fallback1, unrouted1) = AllocCore::dbg_class_authority_activation_counts();

    let checks = checks1 - checks0;
    let fallback = fallback1 - fallback0;
    let unrouted = unrouted1 - unrouted0;
    let total = checks + fallback + unrouted;
    let pct = if total == 0 { 0 } else { checks * 100 / total };
    println!("AUTHORITY_CACHED_CHECKS={checks}");
    println!("AUTHORITY_FALLBACK_CHECKS={fallback}");
    println!("AUTHORITY_UNROUTED_FREES={unrouted}");
    println!("AUTHORITY_ROUTED_PERCENT={pct}%");
    assert!(total >= N as u64, "no frees observed by the oracle (total={total})");
    assert!(pct >= 95, "authority check ran on only {pct}% of Small own frees");
    // U1b: the authority must come from the CACHED handle, not the
    // authoritative directory lookup (ADR §4.1 (в) transport).
    assert!(
        checks >= (95 * total) / 100,
        "cached-handle transport on only {checks}/{total} authority checks"
    );
    // Case (в): missing/stale cached handle while the route is alive — must
    // not occur in the standard single-heap scenario.
    assert_eq!(fallback, 0, "case-в cached-handle fallback on a routed heap free");
    // Case (а): unrouted legacy path — must not occur on a routed heap free.
    assert_eq!(unrouted, 0, "case-а fallback on a routed heap free");
    drop(lease);
}
