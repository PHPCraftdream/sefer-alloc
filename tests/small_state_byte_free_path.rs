//! B4 Шаг S-v (`small-state-byte`, `state_byte_design.md` variant (в)) —
//! free-path behaviour tests. The state byte (`(c+1) | s<<6`, LIVE=0 /
//! MAG=1 / FREE=2) is written by the substrate/heap writers and read ONCE
//! by `small_free_guard`, where it replaces the `is_in_magazine` +
//! `alloc_bitmap().is_free` oracle pair AND the U1 class formula (strict
//! form: the byte's class field must equal `class_for(Layout) + 1`).
//!
//! Covered here:
//! - (a) a valid free takes the byte path and the block re-issues;
//! - (b) double-free of a MAGAZINE-resident block → MAG byte → OLD reject
//!   category (`LOCAL_REJECT_FREES`, oracle G4 через байт MAG);
//! - (c) double-free AFTER an overflow flush → FREE byte (and the alloc
//!   bitmap agrees — both states reject) → OLD category;
//! - (d) free whose Layout class differs from the byte's class field →
//!   strengthening reject (`LOCAL_REJECT_STRONGER_FREES`) — м2's target;
//! - (e) alloc after free re-issues the block with a LIVE byte (a further
//!   legitimate free is accepted — м3/м5's target);
//! - (f) payload/bump-era checks remain: unaligned `free(p+8)` and a
//!   non-start granule (`p+16` of a 32 B block) are strengthening rejects.

#![cfg(all(
    feature = "production",
    feature = "small-state-byte",
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

/// (a) + (e): alloc → free → alloc re-issues; the re-issued block carries a
/// LIVE byte, proven by a further legitimate free being accepted (no reject
/// counter moves).
#[test]
fn ssv_free_then_reissue_byte_path_accepts() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(16, 16).unwrap();
    // SAFETY: valid layout on the calling thread's own heap slot.
    let p = heap.alloc(l);
    assert!(!p.is_null());
    unsafe { heap.dealloc(p, l) };

    let before_rejects = AllocCore::dbg_local_reject_frees();
    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    // (e) re-issue (LIFO → the same block), then free it AGAIN legitimately:
    // accepted only if the issue path wrote LIVE into the byte (м3/м5 RED
    // if alloc-hit / refill does not maintain the byte).
    // SAFETY: valid layout.
    let q = heap.alloc(l);
    assert_eq!(q, p, "magazine LIFO must re-issue the just-freed block");
    unsafe { heap.dealloc(q, l) };
    assert_eq!(AllocCore::dbg_local_reject_frees(), before_rejects);
    assert_eq!(
        AllocCore::dbg_local_reject_stronger_frees(),
        before_stronger,
        "a legitimate free of a re-issued block must NOT be rejected"
    );

    // (e2) force a magazine REFILL (bin→user issue through `pop_free`): the
    // refill-issued block must carry a LIVE byte, proven by its legitimate
    // free being accepted (м5: without `pop_free`'s store the block keeps
    // its bin FREE byte and this free is wrongly rejected).
    let mut last = ::core::ptr::null_mut::<u8>();
    for _ in 0..128 {
        // SAFETY: valid layout.
        let b = heap.alloc(l);
        assert!(!b.is_null());
        last = b;
    }
    let before_rejects = AllocCore::dbg_local_reject_frees();
    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    // SAFETY: `last` is a live refill-issued block of exactly `l`.
    unsafe { heap.dealloc(last, l) };
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        before_rejects,
        "a refill-issued (bin→user, `pop_free`) block must carry a LIVE \
         state byte so its legitimate free is accepted"
    );
    assert_eq!(AllocCore::dbg_local_reject_stronger_frees(), before_stronger);
}

/// (b): free X → X is magazine-resident (byte = `c+1 | MAG`) → repeated
/// free X → rejected by the MAG byte in the OLD counter category.
#[test]
fn ssv_in_magazine_double_free_rejected_via_mag_byte() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(16, 16).unwrap();
    // SAFETY: valid layout.
    let p = heap.alloc(l);
    assert!(!p.is_null());
    unsafe { heap.dealloc(p, l) };

    let before_rejects = AllocCore::dbg_local_reject_frees();
    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    // SAFETY: deliberate double free; must be a no-op.
    unsafe { heap.dealloc(p, l) };
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        before_rejects + 1,
        "MAG-byte double free must land in the OLD reject category (G4)"
    );
    assert_eq!(AllocCore::dbg_local_reject_stronger_frees(), before_stronger);
}

/// (c): drive the class's magazine past its free-side budget so the earliest
/// freed blocks are flushed to the substrate (byte = `c+1 | FREE`, alloc
/// bitmap free) → re-freeing such a block is rejected in the OLD category —
/// through the byte AND the bitmap (both stale-free states reject).
#[test]
fn ssv_double_free_after_flush_rejected_via_free_byte() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    const N: usize = 80; // ≥ TCACHE_CAP for the 16 B class → overflow flush
    let l = Layout::from_size_align(16, 16).unwrap();
    let mut blocks = [::core::ptr::null_mut::<u8>(); N];
    for b in blocks.iter_mut() {
        // SAFETY: valid layout.
        *b = heap.alloc(l);
        assert!(!b.is_null());
    }
    // Free in allocation order: the first FREED blocks are the ones the
    // R1-01 overflow policy flushes to the substrate (FREE byte + free bit).
    for &b in blocks.iter() {
        // SAFETY: each block was issued for exactly this layout and is live.
        unsafe { heap.dealloc(b, l) };
    }

    let before_rejects = AllocCore::dbg_local_reject_frees();
    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    // The FIRST-freed block was flushed (not magazine-resident anymore): its
    // byte is `c+1 | FREE` and its alloc-bitmap bit is set — both reject.
    // SAFETY: deliberate double free; must be a no-op.
    unsafe { heap.dealloc(blocks[0], l) };
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        before_rejects + 1,
        "FREE-byte (flushed) double free must land in the OLD reject category"
    );
    assert_eq!(AllocCore::dbg_local_reject_stronger_frees(), before_stronger);
}

/// (d): free with a Layout whose class differs from the byte's class field
/// (aligned wrong-class) → strengthening reject — the strict form of the
/// byte comparison (this is the counterfactual target of mutant м2).
#[test]
fn ssv_wrong_class_free_is_strengthening_reject() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let l = Layout::from_size_align(16, 16).unwrap();
    // SAFETY: valid layout.
    let p = heap.alloc(l);
    assert!(!p.is_null());
    unsafe { heap.dealloc(p, l) };

    let before_rejects = AllocCore::dbg_local_reject_frees();
    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    let wrong = Layout::from_size_align(32, 16).unwrap();
    // SAFETY: deliberate contract violation (wrong-class free of a live
    // magazine block); must be a no-op counted as a strengthening.
    unsafe { heap.dealloc(p, wrong) };
    assert_eq!(
        AllocCore::dbg_local_reject_stronger_frees(),
        before_stronger + 1,
        "the byte's class field (16 B) vs Layout class (32 B) mismatch is a \
         strengthening reject"
    );
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        before_rejects,
        "strengthening rejects must NOT leak into LOCAL_REJECT_FREES"
    );
}

/// (f): the explicit payload-era checks remain on the byte path — an
/// unaligned `free(start+8)` and a free of a non-start granule
/// (`start+16` inside a 32 B block, whose granule was never issued) are
/// both strengthening rejects, exactly as under `small-class-authority`.
#[test]
fn ssv_unaligned_and_interior_frees_remain_rejects() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    // SAFETY: valid layout.
    let p = heap.alloc(Layout::from_size_align(32, 16).unwrap());
    assert!(!p.is_null());

    let before_rejects = AllocCore::dbg_local_reject_frees();
    let before_stronger = AllocCore::dbg_local_reject_stronger_frees();
    let l = Layout::from_size_align(32, 16).unwrap();
    // SAFETY: deliberate contract violations; no-ops.
    unsafe {
        heap.dealloc(p.byte_add(8), l);
    }
    unsafe {
        heap.dealloc(p.byte_add(16), l);
    }
    assert_eq!(
        AllocCore::dbg_local_reject_stronger_frees(),
        before_stronger + 2,
        "unaligned (start+8) and non-start-granule (start+16) frees are \
         strengthening rejects under the state-byte path"
    );
    assert_eq!(AllocCore::dbg_local_reject_frees(), before_rejects);
}
