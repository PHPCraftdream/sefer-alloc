//! F1 + F4 (review, B4 state-byte) — MISALIGNED-INTERIOR-FREE and
//! LOCAL_REJECT_FREES counter semantics of the HeapCore/tcache free path.
//!
//! F1: `StateByte::read(off)` indexes `off >> 4`, so a scalar free of an
//! interior address `p+1..p+15` of a LIVE 16 B block (WITHOUT `hardened`'s H1
//! stride guard) reads the START granule's byte `encode(c, LIVE)` and used to
//! be ACCEPTED — the interior address was parked in the magazine. The guard
//! now rejects `(off & (MIN_BLOCK - 1)) != 0` unconditionally, shared with
//! the batch path. Mutant: drop the alignment term from the guard's reject
//! branch → `f1_misaligned_interior_frees_are_noops` goes RED.
//!
//! F4: the base's wrong-class Accept-branch reject returned WITHOUT bumping
//! `LOCAL_REJECT_FREES`; the state-byte rewrite funneled every reject into
//! the cold reject path only: state (MAG/FREE/zero) rejects bump (as the
//! base's `RejectNoOp` arm did); MISALIGNED rejects and an aligned LIVE-byte
//! class-field mismatch return `RejectNoOpSilent` (no bump) — the base
//! rejected both via `divides`/class identity in the Accept branch without
//! incrementing. Mutant: fold
//! `RejectNoOpSilent` back into `RejectNoOp` →
//! `f4_wrong_class_free_counter_semantics` goes RED.
//!
//! This file deliberately does NOT enable `hardened`: F1 is a production
//! (`plain production`, no H1) defect.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "batch-api",
    not(feature = "hardened")
))]

use core::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::{AllocCore, SegmentLayout};

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

fn l16() -> Layout {
    Layout::from_size_align(16, 8).unwrap()
}

fn l32() -> Layout {
    Layout::from_size_align(32, 8).unwrap()
}

fn l64() -> Layout {
    Layout::from_size_align(64, 8).unwrap()
}

/// F1: scalar AND batch frees of `p+1` / `p+8` / `p+15` must be no-ops —
/// the magazine never gains residency for them, the block never reads FREE,
/// and none of these addresses is ever reissued by a subsequent alloc storm;
/// the genuine free of `p` still works and `p` is reissued exactly once.
#[test]
fn f1_misaligned_interior_frees_are_noops() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l16());
    assert!(!p.is_null(), "alloc 16 B failed");
    // Live anchor so the segment/bin state stays non-trivial.
    let anchor = heap.alloc(l16());
    assert!(!anchor.is_null());

    let bads: Vec<*mut u8> = vec![
        // SAFETY (probe construction): pointer arithmetic within `p`'s own
        // live 16 B allocation, one byte past its end inclusive — addresses
        // only, never dereferenced.
        unsafe { p.add(1) },
        unsafe { p.add(8) },
        unsafe { p.add(15) },
    ];

    // Scalar interior frees: each must be rejected outright — the magazine
    // residency (slot count for class 0) must not move.
    let count0 = heap.dbg_tcache_count(0);
    for &bad in &bads {
        // SAFETY (guard probe): exercising the documented no-op reject path —
        // `bad` is interior to the live block `p`; the callee must reject it
        // without freeing anything.
        unsafe { heap.dealloc(bad, l16()) };
        assert_eq!(
            heap.dbg_tcache_count(0),
            count0,
            "scalar free of {bad:p} parked an interior address in the magazine"
        );
    }

    // Batch interior frees: the shared guard must reject them too.
    // SAFETY (guard probe): same interior-address no-op probe, batch form.
    unsafe { heap.dealloc_batch(l16(), &bads) };
    assert_eq!(
        heap.dbg_tcache_count(0),
        count0,
        "batch interior frees parked addresses in the magazine"
    );

    // Alloc storm: every issued address is unique, and none of the interior
    // (nor the still-live start) addresses ever comes back.
    let mut seen: HashSet<usize> = HashSet::new();
    seen.insert(p as usize);
    for bad in &bads {
        seen.insert(*bad as usize);
    }
    seen.insert(anchor as usize);
    let mut storm = Vec::new();
    for _ in 0..64 {
        let q = heap.alloc(l16());
        assert!(!q.is_null(), "storm alloc returned null");
        assert!(
            seen.insert(q as usize),
            "alloc reissued an address ({:p}) after interior-free rejects",
            q
        );
        storm.push(q);
    }

    // The genuine free of the START block works and `p` is reissued exactly
    // once.
    // SAFETY: live, matching layout, freed exactly once here.
    unsafe { heap.dealloc(p, l16()) };
    assert_eq!(
        heap.dbg_tcache_count(0),
        count0 + 1,
        "legitimate free of the start block must park it in the magazine"
    );
    let reissue = heap.alloc(l16());
    assert_eq!(
        reissue, p,
        "the legitimately freed start block must be reissued exactly once (LIFO)"
    );

    // Cleanup: everything the storm issued, plus the reissue.
    // SAFETY: each pointer is live, was issued by a matching alloc above,
    // and is freed exactly once here.
    unsafe { heap.dealloc(reissue, l16()) };
    for &q in &storm {
        unsafe { heap.dealloc(q, l16()) };
    }
    // SAFETY: live anchor, matching layout, freed exactly once here.
    unsafe { heap.dealloc(anchor, l16()) };
}

/// F4: `LOCAL_REJECT_FREES` must keep the base's per-reason semantics — a
/// misaligned free and an aligned wrong-class free (LIVE byte, foreign class
/// field) do NOT bump the counter; a state reject (double-free against a MAG
/// byte) bumps by 1.
#[test]
fn f4_wrong_class_free_counter_semantics() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l16());
    assert!(!p.is_null());
    let anchor = heap.alloc(l16());
    assert!(!anchor.is_null());
    let wide = heap.alloc(l64());
    assert!(!wide.is_null());

    let before = AllocCore::dbg_local_reject_frees();

    // F4-fix: MISALIGNED free — the base rejected these via `divides` in the
    // Accept branch and returned WITHOUT incrementing. The guard must keep
    // that (RejectNoOpSilent).
    // SAFETY (guard probe): `p.add(8)` is interior to the live block `p`; the
    // misaligned layout case is the deliberately rejected probe under test.
    // SAFETY (guard probe): `p.add(8)` is interior to the live block `p` —
    // the deliberately rejected probe under test.
    unsafe {
        heap.dealloc(p.wrapping_add(8), l16());
        assert_eq!(
            AllocCore::dbg_local_reject_frees(),
            before,
            "misaligned free must not bump LOCAL_REJECT_FREES (base semantics)"
        );

        // A byte-zero interior granule within the bump range is likewise a silent
        // no-op, without changing the live block or magazine.
        let zero_before = AllocCore::dbg_local_reject_frees();
        let mag_before = heap.dbg_tcache_count(0);
        heap.dealloc(wide.wrapping_add(16), l64());
        assert_eq!(AllocCore::dbg_local_reject_frees(), zero_before);
        assert_eq!(heap.dbg_tcache_count(0), mag_before);
        assert_eq!(heap.dbg_state_byte_state_for(wide), Some(0));

        // An aligned address beyond the initialized bump range, but still inside
        // the same mapped segment, is a counted reject. Derive it from the live
        // allocation's segment base and never pass an out-of-segment address.
        let segment_end =
            SegmentLayout::segment_base_of(wide as usize).wrapping_add(SegmentLayout::SEGMENT);
        let past_bump = (segment_end - 16) as *mut u8;
        let past_before = AllocCore::dbg_local_reject_frees();
        heap.dealloc(past_bump, l64());
        assert_eq!(
            AllocCore::dbg_local_reject_frees(),
            past_before + 1,
            "past-bump reject must bump LOCAL_REJECT_FREES by exactly 1"
        );
        assert_eq!(heap.dbg_tcache_count(0), mag_before);
    }

    // Aligned wrong-class free: class 1 (32 B) requested for a class 0 block
    // — byte reads encode(0, LIVE) != encode(1, LIVE) → the base's
    // Accept-branch reject, which did NOT increment.
    let wrong_class_before = AllocCore::dbg_local_reject_frees();
    // SAFETY (guard probe): `p` is live and interior-safe; the wrong-class
    // layout is the deliberately rejected contract case under test.
    unsafe { heap.dealloc(p, l32()) };
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        wrong_class_before,
        "wrong-class free must not bump LOCAL_REJECT_FREES (base semantics)"
    );

    // Legitimate free → MAG; the immediate double-free is a STATE reject
    // (MAG ≠ LIVE) → exactly one increment, as in the base.
    // SAFETY: live, matching layout, freed exactly once here.
    unsafe { heap.dealloc(p, l16()) };
    let before_double_free = AllocCore::dbg_local_reject_frees();
    // SAFETY (double-free probe): second free of the just-freed block — the
    // M2 no-op path under test.
    unsafe { heap.dealloc(p, l16()) };
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        before_double_free + 1,
        "double-free (state reject) must bump LOCAL_REJECT_FREES by exactly 1"
    );

    // Cleanup.
    // SAFETY: live wide block, matching layout, freed exactly once here.
    unsafe {
        heap.dealloc(wide, l64());
        heap.dealloc(anchor, l16());
    }
}

/// F12 (chunk 3): production misaligned-free inventory, extended: for
/// k ∈ {1, 8, 15} the free of `p + k` must (a) not move the magazine count,
/// (b) NEVER let `p + k` be issued (storm of 64 allocs sees it at most...
/// never — the address was never a block start), and (c) leave `p`'s state
/// byte exactly LIVE (nothing was committed for the interior address).
#[test]
fn f12_misaligned_free_production_inventory() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    for &k in &[1usize, 8, 15] {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        let anchor = heap.alloc(l16());
        assert!(!anchor.is_null());
        assert_eq!(
            heap.dbg_state_byte_state_for(p),
            Some(0),
            "k={k}: precondition, live block must read LIVE"
        );
        let interior = unsafe { p.add(k) };
        let cnt = heap.dbg_tcache_count(0);
        // SAFETY (guard probe): `interior` is inside the live block `p`; the
        // callee must reject it as a no-op.
        unsafe { heap.dealloc(interior, l16()) };
        assert_eq!(
            heap.dbg_tcache_count(0),
            cnt,
            "k={k}: misaligned free moved the magazine count"
        );
        assert_eq!(
            heap.dbg_state_byte_state_for(p),
            Some(0),
            "k={k}: misaligned free changed the block's state byte"
        );
        // Storm: `interior` is never issued (it is not a block start), and
        // `p` itself is issued at most once in the storm (nothing was freed).
        let mut seen_interior = 0;
        let mut seen_p = 0;
        let mut storm = Vec::new();
        for _ in 0..64 {
            let q = heap.alloc(l16());
            assert!(!q.is_null());
            if q == interior {
                seen_interior += 1;
            }
            if q == p {
                seen_p += 1;
            }
            storm.push(q);
        }
        assert_eq!(seen_interior, 0, "k={k}: interior address was issued");
        assert!(seen_p <= 1, "k={k}: live block reissued {seen_p}x");
        for &q in &storm {
            // SAFETY: live, single free each.
            unsafe { heap.dealloc(q, l16()) };
        }
        // Cleanup round-trip.
        // SAFETY: live, single free each.
        unsafe { heap.dealloc(p, l16()) };
        let rp = heap.alloc(l16());
        // SAFETY: live (reissue), single free.
        unsafe { heap.dealloc(rp, l16()) };
        // SAFETY: live anchor, single free.
        unsafe { heap.dealloc(anchor, l16()) };
    }
}
