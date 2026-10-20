//! B4 У1 (`small-class-authority`, ADR §8 T5) — G4 magazine-oracle pin under
//! the class authority: free X → (refill path) → X resident in the magazine →
//! repeated free X → NO double issue. The M2 oracle pair is UNCHANGED by У1
//! (§4.5 order: alignment → class byte → magazine → bump → is_free), and the
//! in-magazine reject stays in the OLD counter category
//! (`LOCAL_REJECT_FREES`), never `LOCAL_REJECT_STRONGER_FREES`.
//!
//! COUNTERFACTUAL: removing `is_in_magazine` REDs this on the existing path
//! already (the block is pushed twice and issued twice); feeding a WRONG
//! class map into the check (м4) does NOT green the double-free — the
//! magazine oracle is class-map-independent — but the wrong-class free of the
//! same block below turns from reject to push, double-issuing across classes.

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

#[test]
fn in_magazine_double_free_stays_noop_under_class_authority() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(16, 16).unwrap();
    // SAFETY: valid layout on the calling thread's own heap slot.
    let p = heap.alloc(l);
    assert!(!p.is_null());

    // Free → p is magazine-resident. A wrong-class free of the SAME
    // resident block must be caught by the class authority BEFORE the
    // magazine oracle (§4.5 order) and counted as a strengthening.
    unsafe { heap.dealloc(p, l) };
    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    let before_rejects = AllocCore::dbg_local_reject_frees();
    let wrong = Layout::from_size_align(32, 16).unwrap();
    // SAFETY: deliberate contract violation; must be a no-op.
    unsafe { heap.dealloc(p, wrong) };
    assert_eq!(
        AllocCore::dbg_local_reject_stronger_frees(),
        before_stronger + 1
    );

    // Repeated free of the magazine-resident block: the G4 oracle (oracle 1),
    // OLD category.
    // SAFETY: deliberate double free; must be a no-op.
    unsafe { heap.dealloc(p, l) };
    assert_eq!(AllocCore::dbg_local_reject_frees(), before_rejects + 1);
    assert_eq!(
        AllocCore::dbg_local_reject_stronger_frees(),
        before_stronger + 1,
        "in-magazine double free must stay in the OLD counter category"
    );

    // No double issue: subsequent allocs of the class are pairwise distinct.
    let mut issued: Vec<*mut u8> = Vec::new();
    for _ in 0..64 {
        // SAFETY: valid layout.
        let q = heap.alloc(l);
        assert!(!q.is_null());
        assert!(issued.iter().all(|&r| r != q), "double issue after G4");
        issued.push(q);
    }
    for q in issued {
        // SAFETY: live 16 B allocation, freed exactly once.
        unsafe { heap.dealloc(q, l) };
    }
    drop(lease);
}
