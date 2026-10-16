//! R6-T33 + R6-T05 (`docs/agent_reviews_round6/SUMMARY.md`): own-thread small
//! free validation on the magazine path.
//!
//! R6-T33: a segment header whose `kind` byte is corrupted decodes to
//! `BlockKind::Unknown`; `small_free_guard` must reject the free as a no-op —
//! the same policy the substrate free in `mem_impl.rs` already applies — and
//! the block must NOT reach the magazine.
//!
//! R6-T05: the class from the caller's `Layout` is checked against the owner
//! route class map's issued start-granule class. An interior pointer freed
//! with a mismatched class (the review's `alloc(64)` → `dealloc(p+16,
//! Layout(48))` scenario) is a no-op: the interior granule encodes no class,
//! so the address is never parked in any magazine and never reissued inside
//! the still-live block.
//!
//! Both tests deliberately violate the `unsafe fn dealloc` caller contract
//! (corrupted allocator metadata; a false layout on an interior pointer) to
//! probe defense-in-depth no-ops — same posture as
//! `tests/r6_t50_foreign_realloc_committed_span.rs`, whose `ForeignLease`
//! wrapper this file reuses verbatim: `HeapLease::drop` aborts while
//! panicking, so a failed assertion must leak the lease instead.
//!
//! Pre-fix counterfactuals: R6-T33 — the Unknown-kind free fell through to
//! the M2 oracles into the magazine push, so the tcache count incremented and
//! the next same-class alloc reissued the corrupted-kind block; R6-T05 — the
//! interior/mismatched-class pointer passed the oracles into class c_small's
//! magazine, so `dbg_tcache_count(c_small)` incremented and the next
//! `alloc(48)` returned `p + 16` inside the live 64-byte block.

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
fn mismatched_class_and_interior_frees_are_not_reissued() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let mut lease = ForeignLease::claim();
    let heap = lease.core();

    let big = Layout::from_size_align(64, 16).unwrap();
    let small = Layout::from_size_align(48, 16).unwrap();
    let class_big = SegmentLayout::class_for(64, 16).expect("64 B is a small class");
    let class_small = SegmentLayout::class_for(48, 16).expect("48 B is a small class");
    assert_ne!(class_big, class_small, "premise: distinct size classes");

    // Positive control: a same-class free increments its magazine count.
    let p1 = heap.alloc(big);
    assert!(!p1.is_null(), "setup: 64-byte allocation failed");
    let resident_big = heap.dbg_tcache_count(class_big);
    // SAFETY: live own-thread allocation, true layout, freed exactly once.
    unsafe { heap.dealloc(p1, big) };
    assert_eq!(
        u32::from(heap.dbg_tcache_count(class_big)),
        u32::from(resident_big) + 1,
        "positive control: legitimate free reached the class magazine"
    );

    // R6-T05 review scenario: alloc(64), then dealloc(p + 16, Layout(48,16)).
    let p = heap.alloc(big);
    assert!(!p.is_null());
    let interior = p.wrapping_add(16);
    // The owner class map carries the issuing class only at a block's start
    // granule — the direct mechanism oracle for the check below.
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(p.addr()),
        Some(class_big as u8),
        "the start granule must encode the issuing class"
    );
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(interior.addr()),
        None,
        "an interior granule must not encode a class"
    );
    let resident_small = heap.dbg_tcache_count(class_small);
    let resident_big = heap.dbg_tcache_count(class_big);
    // SAFETY: the interior pointer + wrong-class layout is the deliberate
    // R6-T05 caller-misuse scenario (a defense-in-depth no-op is expected;
    // the live block p is untouched by it).
    unsafe { heap.dealloc(interior, small) };
    assert_eq!(
        heap.dbg_tcache_count(class_small),
        resident_small,
        "an interior/mismatched-class free must be a no-op (R6-T05)"
    );
    assert_eq!(
        heap.dbg_tcache_count(class_big),
        resident_big,
        "the interior free must not disturb the true class's magazine either"
    );
    let q = heap.alloc(small);
    assert_ne!(
        q.addr(),
        interior.addr(),
        "pre-fix: p+16 passed the oracles into class c_small's magazine and \
         the next alloc(48) reissued it inside the live 64-byte block"
    );

    // Block-start granule with a wrong class: also a no-op.
    let p3 = heap.alloc(big);
    assert!(!p3.is_null());
    let resident_small = heap.dbg_tcache_count(class_small);
    // SAFETY: the block-start pointer with a deliberately wrong-class layout
    // is the mismatched-class half of R6-T05.
    unsafe { heap.dealloc(p3, small) };
    assert_eq!(
        heap.dbg_tcache_count(class_small),
        resident_small,
        "a wrong-class free must not park the block in the wrong magazine"
    );
}
