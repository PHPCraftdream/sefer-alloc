//! R6-T33 + R6-T05 (`docs/agent_reviews_round6/SUMMARY.md`): own-thread small
//! free validation on the magazine path.
//!
//! R6-T33: a segment header whose `kind` byte is corrupted decodes to
//! `BlockKind::Unknown`; `small_free_guard` must reject the free as a no-op —
//! the same policy the substrate free in `mem_impl.rs` already applies — and
//! the block must NOT reach the magazine.
//!
//! R6-T05: default own-free validation rejects offsets not divisible by the
//! caller's class stride. Hardened additionally compares the issued class;
//! aligned wrong-class starts can pass the default arithmetic check.
//!
//! Counterfactuals: without the Unknown-kind check its block reaches the
//! magazine and is reissued; without the arithmetic check the misaligned
//! interior pointer reaches its supplied class's magazine. Without the
//! hardened identity check an aligned wrong-class start reaches the wrong
//! magazine despite passing both stride divisibility checks.
//!
//! The tests deliberately violate the `unsafe fn dealloc` caller contract to
//! probe defense-in-depth no-ops. `HeapLease::drop` aborts while panicking,
//! so the `ForeignLease` wrapper leaks the lease on a failed assertion.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "internals"
))]

use core::alloc::Layout;
use core::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::segment_route::RouteRegistration;
use sefer_alloc::registry::{bootstrap, HeapLease, HeapRegistry};
use sefer_alloc::SegmentLayout;

// Serialise: the registry (and its per-thread heap) is process-global.
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

/// A lease that is leaked, not dropped, while a failed assertion unwinds:
/// `HeapLease::drop` aborts while panicking, which would turn every
/// assertion failure below into a message-less SIGABRT.
struct ForeignLease(Option<HeapLease>);
impl ForeignLease {
    fn claim() -> Self {
        Self(Some(
            HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None"),
        ))
    }
    fn core(&mut self) -> &mut sefer_alloc::registry::HeapCore {
        self.0.as_mut().expect("lease present").core()
    }
}
impl Drop for ForeignLease {
    fn drop(&mut self) {
        if std::thread::panicking() {
            core::mem::forget(self.0.take());
        }
    }
}

#[test]
fn corrupted_kind_byte_own_free_is_a_noop_not_a_magazine_push() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let mut lease = ForeignLease::claim();
    let heap = lease.core();
    let layout = Layout::from_size_align(64, 16).unwrap();
    let class = SegmentLayout::class_for(64, 16).expect("64 B / align 16 is a small class");

    // Positive control: with a legitimate kind, a free parks the block in the
    // class's magazine and the next alloc reissues it (LIFO).
    let p0 = heap.alloc(layout);
    assert!(!p0.is_null(), "setup: 64-byte allocation failed");
    let resident = heap.dbg_tcache_count(class);
    // SAFETY: `p0` is a live own-thread allocation, freed exactly once with
    // its true layout.
    unsafe { heap.dealloc(p0, layout) };
    assert_eq!(
        u32::from(heap.dbg_tcache_count(class)),
        u32::from(resident) + 1,
        "positive control: a legitimate own free must reach the magazine"
    );
    let reissued = heap.alloc(layout);
    assert_eq!(reissued, p0, "positive control: magazine LIFO");

    // R6-T33: corrupt the segment's kind byte; the free must become a no-op.
    let true_byte = heap.dbg_kind_byte_of(p0);
    // SAFETY: p0's segment is a live, exclusively owned segment of this
    // lease's heap; the true byte captured above is restored below (and the
    // corrupt value is only ever observed by the strict-decode accessor).
    unsafe { heap.dbg_stamp_kind_byte(p0, 0x99) };
    assert_eq!(heap.dbg_kind_at_tag(p0), 3, "stamp must decode to Unknown");

    let resident = heap.dbg_tcache_count(class);
    // SAFETY: the deliberately corrupted-kind free is the R6-T33 scenario;
    // the pointer is this heap's live allocation of the matching layout.
    unsafe { heap.dealloc(p0, layout) };
    assert_eq!(
        heap.dbg_tcache_count(class),
        resident,
        "an Unknown-kind segment's block must NOT enter the magazine (R6-T33)"
    );
    let q = heap.alloc(layout);
    assert_ne!(
        q, p0,
        "pre-fix: the Unknown-kind free parked p0 in the magazine and the \
         next alloc reissued it"
    );

    // Restore the true kind; the same free must reach the magazine again.
    // SAFETY: restores the byte captured above.
    unsafe { heap.dbg_stamp_kind_byte(p0, true_byte) };
    assert_eq!(
        heap.dbg_kind_byte_of(p0),
        true_byte,
        "kind byte restore failed"
    );
    let resident = heap.dbg_tcache_count(class);
    // SAFETY: p0 stayed live across the no-op free above (the bitmap was
    // never touched); freed exactly once here with its true layout.
    unsafe { heap.dealloc(p0, layout) };
    assert_eq!(
        u32::from(heap.dbg_tcache_count(class)),
        u32::from(resident) + 1,
        "with the kind restored, the same free must reach the magazine again"
    );
}

#[test]
fn misaligned_interior_free_is_rejected_in_all_configurations() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let mut lease = ForeignLease::claim();
    let heap = lease.core();
    let big = Layout::from_size_align(64, 16).unwrap();
    let small = Layout::from_size_align(48, 16).unwrap();
    let class_big = SegmentLayout::class_for(64, 16).expect("64 B is a small class");
    let class_small = SegmentLayout::class_for(48, 16).expect("48 B is a small class");
    // Positive control: a legitimate same-class free reaches its magazine,
    // so the no-op assertion below is not vacuous.
    let p0 = heap.alloc(small);
    assert!(!p0.is_null(), "setup: 48-byte allocation failed");
    let before = heap.dbg_tcache_count(class_small);
    // SAFETY: live own-thread allocation, true layout, freed exactly once.
    unsafe { heap.dealloc(p0, small) };
    assert_eq!(
        u32::from(heap.dbg_tcache_count(class_small)),
        u32::from(before) + 1,
        "positive control: legitimate free reached the class magazine"
    );
    let p = heap.alloc(big);
    assert!(!p.is_null(), "setup: 64-byte allocation failed");
    let root = SegmentLayout::segment_base_of(p.addr()) as *mut u8;
    let interior = [p.wrapping_add(16), p.wrapping_add(32), p.wrapping_add(48)]
        .into_iter()
        .find(|candidate| {
            let off = candidate.addr() - root.addr();
            off % 48 != 0 && off % 64 != 0
        })
        .expect("at least one candidate is not divisible by either class stride");
    let off = interior.addr() - root.addr();
    assert_ne!(off % 48, 0);
    assert_ne!(off % 64, 0);
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(p.addr()),
        Some(class_big as u8),
        "start granule encodes the issuing class"
    );
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(interior.addr()),
        None,
        "interior granule encodes no class"
    );
    let resident_small = heap.dbg_tcache_count(class_small);
    // SAFETY: deliberate interior-pointer and wrong-layout misuse; the guard
    // must reject before altering the live allocation's free state.
    unsafe { heap.dealloc(interior, small) };
    assert_eq!(
        heap.dbg_tcache_count(class_small),
        resident_small,
        "misaligned interior free must not reach the magazine"
    );
    let q = heap.alloc(small);
    assert_ne!(q, interior, "interior pointer was not reissued");
}

#[cfg(feature = "hardened")]
#[test]
fn aligned_wrong_class_block_start_is_rejected_by_hardened_identity_check() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let mut lease = ForeignLease::claim();
    let heap = lease.core();
    let big = Layout::from_size_align(64, 16).unwrap();
    let small = Layout::from_size_align(48, 16).unwrap();
    let class_big = SegmentLayout::class_for(64, 16).expect("64 B is a small class");
    let class_small = SegmentLayout::class_for(48, 16).expect("48 B is a small class");
    let mut p = core::ptr::null_mut();
    for _ in 0..64 {
        let candidate = heap.alloc(big);
        assert!(!candidate.is_null(), "setup: 64-byte allocation failed");
        let off = candidate.addr() - SegmentLayout::segment_base_of(candidate.addr());
        if off % 192 == 0 {
            p = candidate;
            break;
        }
    }
    assert!(
        !p.is_null(),
        "setup: no 64-byte block aligned to both class strides"
    );
    let off = p.addr() - SegmentLayout::segment_base_of(p.addr());
    assert_eq!(off % 192, 0, "proves divisibility by 64 and 48");
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(p.addr()),
        Some(class_big as u8),
        "start granule encodes the true class"
    );
    let resident_small = heap.dbg_tcache_count(class_small);
    // SAFETY: deliberate wrong-layout misuse at a true block start; both
    // stride checks pass, so only hardened owner identity can reject it.
    unsafe { heap.dealloc(p, small) };
    assert_eq!(
        heap.dbg_tcache_count(class_small),
        resident_small,
        "wrong-class block must not reach the magazine"
    );
    let q = heap.alloc(small);
    assert_ne!(q, p, "wrong-class block start was not reissued");
}
