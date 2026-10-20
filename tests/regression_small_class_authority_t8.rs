//! B4 У1 (`small-class-authority`, ADR §8 T8) — item 247(b) closed for
//! `batch-api`: `dealloc_batch`'s batched fast path goes through the SAME
//! `small_free_guard` as the scalar path, so the alignment + class-byte
//! conjunct applies automatically. Aligned wrong-class frees and interior
//! frees into a zero class byte must be batch NO-OPs (before У1 the batch
//! path — which has NO `divides` of its own — ACCEPTED aligned wrong-class
//! blocks and pushed them into the wrong class's magazine).
//!
//! MUTANT ("У1 only in the scalar branch" — batch ignores the authority):
//! RED on both tests — the wrong-class batch free pushes the blocks into the
//! 128 B magazine and the follow-up allocs double-issue them.

#![cfg(all(
    feature = "production",
    feature = "small-class-authority",
    feature = "batch-api",
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

/// Aligned wrong-class batch free of LIVE blocks ⇒ every entry a no-op; the
/// blocks stay live and freeable with their real layout afterwards.
#[test]
fn batch_aligned_wrong_class_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(64, 16).unwrap();
    let wrong = Layout::from_size_align(128, 16).unwrap();
    const N: usize = 8;
    let mut blocks: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: valid layout on the calling thread's own heap slot.
        let p = heap.alloc(l);
        assert!(!p.is_null());
        blocks.push(p);
    }

    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    // SAFETY: deliberate contract violation (wrong layout for every entry);
    // each must degrade to a counted strengthening no-op.
    unsafe { heap.dealloc_batch(wrong, &blocks) };
    assert_eq!(
        AllocCore::dbg_local_reject_stronger_frees(),
        before_stronger + N as u64,
        "batch wrong-class frees must each be counted strengthenings"
    );

    // The blocks are still live: the REAL batch free works, and nothing was
    // double-pushed into the 128 B magazine.
    // SAFETY: every entry is a live 64 B allocation of `heap`, freed once.
    unsafe { heap.dealloc_batch(l, &blocks) };
    let mut issued: Vec<*mut u8> = Vec::new();
    for _ in 0..N {
        // SAFETY: valid layout.
        let q = heap.alloc(l);
        assert!(!q.is_null());
        assert!(issued.iter().all(|&r| r != q), "double issue (mutant: У1 scalar-only)");
        issued.push(q);
    }
    // The 128 B class must NOT hand back any of the 64 B blocks.
    let l128 = Layout::from_size_align(128, 16).unwrap();
    for _ in 0..16 {
        // SAFETY: valid layout.
        let q = heap.alloc(l128);
        assert!(!q.is_null());
        assert!(
            issued.iter().all(|&r| r != q),
            "wrong-class batch free pushed a 64 B block into the 128 B magazine"
        );
        // SAFETY: live 128 B allocation, freed once.
        unsafe { heap.dealloc(q, l128) };
    }
    for q in issued {
        // SAFETY: live 64 B allocation, freed once.
        unsafe { heap.dealloc(q, l) };
    }
    drop(lease);
}

/// Interior (zero class byte) batch free ⇒ no-op; the live block survives.
#[test]
fn batch_interior_zero_leaf_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(256, 16).unwrap();
    const N: usize = 4;
    let mut blocks: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: valid layout.
        let p = heap.alloc(l);
        assert!(!p.is_null());
        blocks.push(p);
    }

    let mut batch: Vec<*mut u8> = Vec::with_capacity(N);
    batch.push(unsafe { blocks[0].byte_add(16) }); // interior granule, encoded 0
    batch.extend_from_slice(&blocks[1..]);

    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    // SAFETY: the first entry is a deliberate interior contract violation
    // (within the segment span); the rest are live 256 B allocations — the
    // interior entry must be a no-op, the rest freed normally.
    unsafe { heap.dealloc_batch(l, &batch) };
    assert_eq!(
        AllocCore::dbg_local_reject_stronger_frees(),
        before_stronger + 1,
        "interior-into-zero-leaf batch entry must be a counted strengthening"
    );

    // blocks[0] is still live: free it and see it re-issued exactly once.
    // SAFETY: live 256 B allocation freed once.
    unsafe { heap.dealloc(blocks[0], l) };
    // SAFETY: valid layout.
    let a = heap.alloc(l);
    assert_eq!(a, blocks[0], "interior batch entry corrupted blocks[0]");
    // SAFETY: live allocation freed once.
    unsafe { heap.dealloc(a, l) };
    drop(lease);
}
