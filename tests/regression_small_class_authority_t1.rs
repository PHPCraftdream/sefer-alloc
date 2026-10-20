//! B4 У1 (`small-class-authority`, ADR `adr_v2_r4.md` §8 T1) — INCLUSION test:
//! `old_prod_reject ⇒ new_reject` plus preserved acceptance of valid blocks.
//!
//! The reference chain is the FULL hardened chain (H1 + payload_start + SEC-1):
//! under `small-class-authority` every free the legacy path rejected must
//! still be rejected, and every legitimately issued block must still be
//! accepted and re-issued exactly once. The strengthening cases —
//! {aligned wrong-class, unaligned `free(start+8)`, non-start 16-aligned
//! granule (interior of a live block / never-issued granule), interior into a
//! zero class byte} — were ACCEPTED before У1 (aligned wrong-class passed
//! production outright; item 209's `start+8` was a documented no-op-by-luck)
//! and must now be counted no-ops that never push into the magazine.
//!
//! COUNTERFACTUALS (mutants from ADR §8):
//! - м1 (drop the alignment conjunct): RED on `unaligned_start_plus_8_*` —
//!   `start+8` passes the class byte, misses the magazine bit, and gets
//!   pushed; the next alloc of the class hands out the unaligned pointer.
//! - м2 (compare `class` instead of `class + 1`): RED on
//!   `acceptance_legit_free_is_reissued` — every legit free is rejected.
//! - м3 (drop the class-byte check / treat Mismatch as Match): RED on
//!   `aligned_wrong_class_*` and `interior_zero_leaf_*` — the bad free is
//!   pushed and the block is double-issued across classes.
//! - м4a (foreign cached handle injected into the own_cache entry — the
//!   U1b transport reads ANOTHER segment's `ClassLeaves`, so the class
//!   byte is wrong for every free): RED on `acceptance_legit_free_is_reissued`
//!   (legit frees rejected) and/or the strengthening assertions — the
//!   verdicts no longer track the real issuer.
//! - м4b (fallback removed: an invalid/missing cached handle is silently
//!   treated as `Unrouted`/skip instead of the authoritative route
//!   lookup): RED on every strengthening assertion — the legacy path
//!   accepts what У1 rejects (no weaker-than-SEC-1 recovery).

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

// Distinct sizes ⇒ distinct classes (`class_for` is monotone in size); all
// with the same 16 B alignment so the ONLY difference between two layouts is
// the class — an aligned wrong-class free in its purest form.
const SIZES: [usize; 5] = [16, 32, 64, 256, 1024];

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 16).unwrap()
}

fn stronger() -> u64 {
    AllocCore::dbg_local_reject_stronger_frees()
}

fn rejects() -> u64 {
    AllocCore::dbg_local_reject_frees()
}

/// Acceptance preservation, per class: a legitimate free must be accepted AND
/// re-issued as the very next alloc of that class (magazine LIFO top). RED
/// under м2 (legit frees rejected).
#[test]
fn acceptance_legit_free_is_reissued() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    for &s in SIZES.iter() {
        let l = layout(s);
        let before = stronger();
        // SAFETY: valid layout on the calling thread's own heap slot.
        let p = heap.alloc(l);
        assert!(!p.is_null(), "alloc {s} returned null");
        unsafe { heap.dealloc(p, l) };
        // SAFETY: valid layout.
        let a = heap.alloc(l);
        assert_eq!(
            a, p,
            "legit free of a {s}B block was not re-issued as the next alloc \
             (acceptance broken — cf. mutant м2)"
        );
        assert_eq!(stronger(), before, "legit free bumped the stronger counter");
        unsafe { heap.dealloc(a, l) };
    }
    drop(lease);
}

/// The three strengthening shapes, swept over classes × offsets (ADR §8 T1's
/// off × классы sweep; the live-block state). Each bad free must be a no-op
/// that (a) bumps ONLY `LOCAL_REJECT_STRONGER_FREES`, never
/// `LOCAL_REJECT_FREES`, and (b) never pushes anything, proven by the block
/// being re-issued exactly once afterwards with no aliasing shadows.
#[test]
fn strengthening_rejects_are_noops_across_classes() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    for (i, &s) in SIZES.iter().enumerate() {
        let l = layout(s);
        // Same alignment, different class: aligned wrong-class free.
        let other = layout(SIZES[(i + 1) % SIZES.len()]);
        let before_stronger = stronger();
        let before_rejects = rejects();

        // SAFETY: valid layout on the calling thread's own heap slot.
        let p = heap.alloc(l);
        assert!(!p.is_null(), "alloc {s} returned null");

        // (1) aligned wrong-class (item 247(b) scalar shape): accepted before
        // У1, pushed p into the OTHER class's magazine — double-issue.
        unsafe { heap.dealloc(p, other) };
        // (2) unaligned `start + 8` (item 209): passes the class byte (the
        // START granule's encoding), so only У1's alignment conjunct rejects.
        // SAFETY: p.byte_add(8) stays inside p's block/segment span; the free
        // is a deliberate contract violation that must be a no-op.
        unsafe { heap.dealloc(p.byte_add(8), l) };
        // (3) non-start 16-aligned granule INSIDE p's own block (s ≥ 32 ⇒ the
        // block spans ≥ 2 granules): encoded 0 — interior into a zero leaf.
        // (For s == 16 there is no interior granule inside the block; the
        // shape is covered by the s == 32 iteration. A free of the granule
        // AFTER a 16 B block legitimately falls through to the magazine/
        // bitmap oracles — its start granule may carry a stale-but-valid
        // class byte from an issued block — and is NOT a strengthening.)
        let want = if s == 16 { 2 } else { 3 };
        if s != 16 {
            // SAFETY: as above; stays within the segment reservation.
            unsafe { heap.dealloc(p.byte_add(16), l) };
        }

        assert_eq!(
            stronger(),
            before_stronger + want,
            "s={s}: the strengthening frees must each bump \
             LOCAL_REJECT_STRONGER_FREES exactly once"
        );
        assert_eq!(
            rejects(),
            before_rejects,
            "s={s}: strengthening rejects must NOT leak into \
             LOCAL_REJECT_FREES (ADR §4.5 category separation)"
        );

        // The block is untouched: the legit free still works and the block is
        // re-issued exactly once, with no pushed shadow (p+8 / p+16).
        unsafe { heap.dealloc(p, l) };
        // SAFETY: valid layout.
        let a = heap.alloc(l);
        assert_eq!(a, p, "s={s}: legit free after bad frees must re-issue p");
        // SAFETY: valid layout.
        let b = heap.alloc(l);
        assert!(!b.is_null());
        assert_ne!(
            b.addr(),
            // SAFETY: pointer arithmetic within p's segment span.
            unsafe { p.byte_add(8).addr() },
            "s={s}: unaligned shadow was pushed (м1)"
        );
        if s != 16 {
            // p+16 is interior to p's own block for s >= 32: it can never be
            // a block start, so issuing it means the interior free was
            // pushed (м3). For s == 16 p+16 IS the next block start and a
            // legitimate issue.
            assert_ne!(
                b.addr(),
                // SAFETY: pointer arithmetic within p's segment span.
                unsafe { p.byte_add(16).addr() },
                "s={s}: interior shadow was pushed (м3)"
            );
        }
        assert_eq!(
            b.addr() % 16,
            0,
            "s={s}: unaligned pointer issued after rejected start+8"
        );
        unsafe {
            heap.dealloc(a, l);
            heap.dealloc(b, l);
        }
    }
    drop(lease);
}

/// Flushed/free-resting state: a block that has overflowed the magazine into
/// the bin (flushed) must behave identically — wrong-class free rejected as a
/// strengthening, genuine double-free still the OLD category (oracle 2,
/// `LOCAL_REJECT_FREES`), acceptance preserved.
#[test]
fn flushed_state_wrong_class_and_double_free() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = layout(16);
    let wrong = layout(32);
    const N: usize = 80; // > TCACHE_CAP: the tail of these flushes to the bin
    let mut blocks = [core::ptr::null_mut::<u8>(); N];
    for b in blocks.iter_mut() {
        // SAFETY: valid layout.
        *b = heap.alloc(l);
        assert!(!b.is_null());
    }
    for b in blocks.iter() {
        // SAFETY: each is a live 16 B allocation of `heap`, freed once.
        unsafe { heap.dealloc(*b, l) };
    }
    let p0 = blocks[0];

    let before_stronger = stronger();
    let before_rejects = rejects();
    // Aligned wrong-class on a FREE block (magazine- or bin-resident):
    // before У1 this was ACCEPTED and pushed p0 into the 32 B magazine.
    // SAFETY: deliberate contract violation; must be a no-op.
    unsafe { heap.dealloc(p0, wrong) };
    assert_eq!(stronger(), before_stronger + 1, "wrong-class free not counted");
    assert_eq!(rejects(), before_rejects, "old counter moved on strengthening");

    // Genuine double-free: the OLD category (magazine oracle or flushed
    // oracle 2) — `LOCAL_REJECT_FREES`, never the strengthening counter.
    // SAFETY: deliberate double free; must be a no-op.
    unsafe { heap.dealloc(p0, l) };
    assert_eq!(rejects(), before_rejects + 1, "double free not counted");
    assert_eq!(stronger(), before_stronger + 1, "double free hit stronger");

    // Acceptance: the full set is still recoverable, each block exactly once.
    let mut seen: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: valid layout.
        let p = heap.alloc(l);
        assert!(!p.is_null());
        assert!(
            seen.iter().all(|&q| q != p),
            "double issue after flushed-state frees"
        );
        seen.push(p);
    }
    for p in seen {
        // SAFETY: a live 16 B allocation of `heap` just issued, freed exactly
        // once here.
        unsafe { heap.dealloc(p, l) };
    }
    drop(lease);
}
