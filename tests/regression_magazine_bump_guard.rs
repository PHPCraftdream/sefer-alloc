//! R1 (task #153, review agent A — Finding 2, MEDIUM) — the magazine-push
//! `off >= bump` stale-free guard, at parity with `AllocCore::dealloc_small`.
//!
//! ## The hole this guards (pre-existing since fastbin)
//!
//! The Э6 magazine push path in `HeapCore::dealloc_own_thread` consulted only
//! two oracles: (1) the in-magazine `slots` scan and (2) the BinTable `is_free`
//! bitmap. Neither catches a free of an address in the UNCARVED payload region
//! of one of our own segments (`off >= bump`): the alloc bitmap for an
//! uncarved / decommitted-then-reset region is all-zero = "allocated", so the
//! bitmap oracle reads `is_free(off) == false` and the bogus pointer falls
//! through to `push`. A later alloc then hands out an address the bump cursor
//! never reached; when `carve_block` later advances to that offset it hands the
//! SAME bytes to a second caller → silent aliasing / double-issue.
//!
//! `dealloc_small` already rejects `off >= bump` under `alloc-decommit`; the
//! magazine path did not. R1 adds the identical, identically-cfg'd guard.
//!
//! ## Counterfactual (verified RED without the guard — see task #153 report)
//!
//! Comment out the `#[cfg(feature = "alloc-decommit")] if (off as usize) >=
//! meta.bump_of() { return; }` block in `heap_core.rs::dealloc_own_thread` and
//! re-run: `bogus_uncarved_free_is_noop` goes RED — the bogus uncarved address
//! is issued by a subsequent alloc, so either the "not issued" assertion trips
//! or the cold-storm distinctness assertion trips (the bogus address aliases a
//! genuinely-carved block). Restoring the guard → GREEN.
//!
//! Gated to the production-shaped combo (`alloc-global` + `fastbin` +
//! `alloc-decommit`): the guard is cfg'd to `alloc-decommit`, mirroring
//! `dealloc_small`. Without `alloc-decommit` segments are never reset, the
//! stale-reset hazard does not exist, and the guard is compiled OUT — so this
//! test is gated out there too.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit"
))]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::SegmentLayout;

// Serialise: the registry is a process-global static.
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

const SEGMENT: usize = SegmentLayout::SEGMENT;

/// Free a block-aligned address deep in the UNCARVED payload region of one of
/// our live segments, using a small class's layout. The guard must reject it
/// (`off >= bump`) → the free is a no-op. Assert (i) the next alloc of that
/// class does NOT return the bogus address, and (ii) a cold-storm of that class
/// yields globally distinct pointers (no aliasing of the never-reached bytes).
#[test]
fn bogus_uncarved_free_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    // 16B class: block_size == 16 divides SEGMENT/2 (a large power of two), so
    // the chosen uncarved offset is a valid multiple of the class block size.
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Alloc one real block to establish a live segment and obtain its base.
    let anchor = unsafe { (*heap).alloc(layout) };
    assert!(!anchor.is_null());
    let base = (anchor as usize) & !(SEGMENT - 1);
    assert_eq!(
        base,
        SegmentLayout::segment_base_of(anchor as usize),
        "segment base mismatch"
    );

    // A block-aligned offset HALF-WAY up the segment: far above the current
    // bump cursor (only a handful of 16B blocks + metadata have been carved),
    // still strictly inside the segment. SEGMENT/2 is a power of two ≥ 16, so
    // it is a valid 16B-block offset that the bump cursor has NOT reached.
    let bogus_off = SEGMENT / 2;
    assert!(bogus_off < SEGMENT);
    assert_eq!(bogus_off % 16, 0, "bogus offset not block-aligned");
    let bogus = (base + bogus_off) as *mut u8;

    // Sanity: the bogus address must NOT be a currently-live pointer.
    assert_ne!(
        bogus, anchor,
        "bogus address collided with the anchor block"
    );

    // The hazardous free: a bogus, never-carved in-segment address. The guard
    // (`off >= bump`) must make this a NO-OP. Without it, `bogus` is pushed
    // into the magazine.
    unsafe { (*heap).dealloc(bogus, layout) };

    // (i) The next alloc of this class must NOT hand back the bogus address.
    // (Without the guard, LIFO magazine pop returns `bogus` immediately.)
    let after = unsafe { (*heap).alloc(layout) };
    assert!(!after.is_null());
    assert_ne!(
        after, bogus,
        "STALE-FREE GUARD BROKEN: a bogus uncarved address was pushed into the \
         magazine and re-issued by the next alloc (off >= bump guard missing)."
    );

    // (ii) Cold-storm the class: every issued pointer must be globally
    // distinct. If the bogus address slipped into circulation, `carve_block`
    // eventually reaches offset SEGMENT/2 and hands those same bytes to a
    // second caller → a duplicate appears here.
    const N: usize = 8192;
    let mut issued: Vec<*mut u8> = Vec::with_capacity(N + 2);
    issued.push(anchor);
    issued.push(after);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "cold-storm alloc returned null");
        issued.push(p);
    }
    let distinct: HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE POINTER: the bogus uncarved address aliased a genuinely \
         carved block — silent double-issue (off >= bump guard missing)."
    );

    // The bogus address must ALSO never appear in the issued set.
    assert!(
        !distinct.contains(&(bogus as usize)),
        "bogus uncarved address was issued during the cold-storm"
    );

    // Also confirm a re-free of the bogus address remains a no-op (idempotent),
    // and the allocator stays healthy afterwards.
    unsafe { (*heap).dealloc(bogus, layout) };
    let healthy = unsafe { (*heap).alloc(layout) };
    assert!(
        !healthy.is_null(),
        "allocator unhealthy after bogus re-free"
    );
    assert_ne!(healthy, bogus, "bogus re-free leaked into the magazine");
    issued.push(healthy);

    // Clean up: free every genuinely-issued block (NOT the bogus address).
    for &p in &issued {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}
