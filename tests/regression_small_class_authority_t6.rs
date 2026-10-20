//! B4 У1 (`small-class-authority`, ADR §8 T6) — the THREE diagnostic
//! categories of ADR §4.5, observed through the counters:
//!
//! 1. "counted today" — `LOCAL_REJECT_FREES`: RejectNoOp (double-free
//!    oracles) and, under `hardened`, the H1 interior guard. Unchanged by У1.
//! 2. "silent today" — production `divides`/SEC-1 rejects STAY silent (no
//!    counter moves); pinned here by the counterfactual that the shapes which
//!    USED to be silently divides-rejected (unaligned offsets) now surface as
//!    strengthenings, while the genuinely-silent legacy-silent set does not
//!    grow.
//! 3. "strengthenings" — `LOCAL_REJECT_STRONGER_FREES`: aligned wrong-class,
//!    unaligned, interior-into-zero-leaf. ONLY these bump the new counter.
//!
//! MUTANTS:
//! - "strengthening counted in LOCAL_REJECT_FREES" (swap the counter in the
//!   scalar reject arm): RED on every `strengthening_*` assertion.
//! - "old-cause priority flipped (H1 after the class byte)" (hardened only):
//!   RED on `h1_interior_stays_old_category` — an unaligned interior free
//!   must be H1's RejectNoOp (old counter), not the class/alignment arm's
//!   RejectStronger.

#![cfg(all(
    feature = "production",
    feature = "small-class-authority",
    feature = "internals"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::AllocCore;

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

fn stronger() -> u64 {
    AllocCore::dbg_local_reject_stronger_frees()
}
fn rejects() -> u64 {
    AllocCore::dbg_local_reject_frees()
}

/// Category 3 vs category 1 separation on the three strengthening shapes.
#[test]
fn strengthening_goes_only_to_the_stronger_counter() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(64, 16).unwrap();
    let wrong = Layout::from_size_align(128, 16).unwrap();
    // SAFETY: valid layout on the calling thread's own heap slot.
    let p = heap.alloc(l);
    assert!(!p.is_null());

    let (s0, r0) = (stronger(), rejects());

    // SAFETY: the next three are deliberate contract violations, each must be
    // a counted no-op (they stay within p's segment span).
    unsafe { heap.dealloc(p, wrong) }; // aligned wrong-class
    unsafe { heap.dealloc(p.byte_add(8), l) }; // unaligned start+8
    unsafe { heap.dealloc(p.byte_add(16), l) }; // interior / zero leaf

    // Under plain production all three are У1 strengthenings (category 3).
    // Under `hardened`, the PRE-EXISTING chain (H1 / the substrate Small free
    // for blocks the magazine path never sees) claims these frees FIRST
    // (ADR §4.5: "пересечения — по первому сработавшему старому предикату"),
    // so they land in `LOCAL_REJECT_FREES` and the strengthening counter
    // stays quiet — every free is still REJECTED (no weakening, only the
    // diagnostic category differs).
    #[cfg(feature = "hardened")]
    let (want_stronger, want_rejects) = (0, 3);
    #[cfg(not(feature = "hardened"))]
    let (want_stronger, want_rejects) = (3, 0);

    assert_eq!(stronger(), s0 + want_stronger, "mutant: strengthening in the wrong counter");
    assert_eq!(rejects(), r0 + want_rejects, "category separation broken");

    // Acceptance after the rejects: block re-issued exactly once.
    unsafe { heap.dealloc(p, l) };
    // SAFETY: valid layout.
    let a = heap.alloc(l);
    assert_eq!(a, p);
    unsafe { heap.dealloc(a, l) };
    drop(lease);
}

/// Category 1: the M2 oracle rejects stay in `LOCAL_REJECT_FREES`, untouched
/// by У1, and the class authority does not double-count them.
#[test]
fn oracle_rejects_stay_in_the_old_counter() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(16, 16).unwrap();
    // SAFETY: valid layout.
    let p = heap.alloc(l);
    unsafe { heap.dealloc(p, l) }; // → magazine
    let (s0, r0) = (stronger(), rejects());
    // SAFETY: deliberate double free (magazine oracle 1).
    unsafe { heap.dealloc(p, l) };
    assert_eq!(rejects(), r0 + 1, "magazine double-free not counted (old cat.)");
    assert_eq!(stronger(), s0, "magazine double-free leaked into stronger");

    // Flush beyond the cap, then the flushed double-free (oracle 2).
    let mut blocks: Vec<*mut u8> = Vec::new();
    for _ in 0..80 {
        // SAFETY: valid layout.
        let q = heap.alloc(l);
        assert!(!q.is_null());
        blocks.push(q);
    }
    for q in blocks.iter() {
        // SAFETY: live 16 B allocations, freed once here.
        unsafe { heap.dealloc(*q, l) };
    }
    let (s1, r1) = (stronger(), rejects());
    // SAFETY: deliberate double free of a flushed block (oracle 2).
    unsafe { heap.dealloc(blocks[70], l) };
    assert_eq!(rejects(), r1 + 1, "flushed double-free not counted");
    assert_eq!(stronger(), s1, "flushed double-free leaked into stronger");

    // Cleanup: re-issue and free the full set exactly once.
    let mut issued: Vec<*mut u8> = Vec::new();
    for _ in 0..81 {
        // SAFETY: valid layout.
        let q = heap.alloc(l);
        assert!(!q.is_null());
        assert!(issued.iter().all(|&r| r != q));
        issued.push(q);
    }
    for q in issued {
        // SAFETY: live allocation freed exactly once.
        unsafe { heap.dealloc(q, l) };
    }
    drop(lease);
}

/// Category 2 stays silent AND hardened priority: with `hardened`, H1 runs
/// BEFORE the У1 block (§4.5: existing Large/kind/H1 branches above are
/// unchanged), so an UNALIGNED INTERIOR free (`off % block_size != 0`) is
/// H1's RejectNoOp (old counter), NOT the alignment/class arm's
/// RejectStronger — "пересечения — по первому сработавшему старому
/// предикату". RED if the У1 block is moved above H1 (priority mutant).
#[cfg(feature = "hardened")]
#[test]
fn h1_interior_stays_old_category() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    // 32-byte class: block_size 32; p+8 is unaligned AND interior.
    let l = Layout::from_size_align(32, 16).unwrap();
    // SAFETY: valid layout.
    let p = heap.alloc(l);
    assert!(!p.is_null());
    let (s0, r0) = (stronger(), rejects());
    // SAFETY: deliberate contract violation within p's segment span.
    unsafe { heap.dealloc(p.byte_add(8), l) };
    assert_eq!(rejects(), r0 + 1, "H1 must fire FIRST (old category)");
    assert_eq!(stronger(), s0, "H1 reject leaked into the stronger counter");

    // An ALIGNED interior granule of the same live block (p+32 is either the
    // next block or never-issued; H1 passes, the class authority rejects —
    // this one IS a strengthening when H1 does not claim it first).
    let (s1, r1) = (stronger(), rejects());
    // SAFETY: deliberate contract violation within p's segment span.
    unsafe { heap.dealloc(p.byte_add(32), l) };
    assert_eq!(
        stronger(),
        s1 + 1,
        "aligned interior must be the class authority's strengthening"
    );
    assert_eq!(rejects(), r1);

    unsafe { heap.dealloc(p, l) };
    // SAFETY: valid layout.
    let a = heap.alloc(l);
    assert_eq!(a, p);
    unsafe { heap.dealloc(a, l) };
    drop(lease);
}

/// Activation oracle (cfg `small-class-authority-activation`, ADR §9(д);
/// NEVER in a measured build): on a routed heap every free either runs the
/// authority check or takes the case-а fallback; on the HeapCore free path
/// the fallback is expected ~0 and the authority check > 0.
#[cfg(feature = "small-class-authority-activation")]
#[test]
fn activation_oracle_counts() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(16, 16).unwrap();
    let (checks0, unrouted0) = AllocCore::dbg_class_authority_activation_counts();
    // SAFETY: valid layout.
    let p = heap.alloc(l);
    unsafe { heap.dealloc(p, l) };
    let (checks1, unrouted1) = AllocCore::dbg_class_authority_activation_counts();
    assert!(
        checks1 > checks0,
        "authority check did not run on a routed free"
    );
    assert_eq!(unrouted1, unrouted0, "case-а fallback on a routed heap free");
    // SAFETY: valid layout.
    let a = heap.alloc(l);
    assert_eq!(a, p);
    unsafe { heap.dealloc(a, l) };
    drop(lease);
}
