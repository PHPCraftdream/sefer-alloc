//! R6-T01: foreign frees must route through the route DIRECTORY, not the
//! (now inert) segment-header `owner_state` word.
//!
//! Task R6-T01 removed the dead owner stamp: `owner_state` is initialised by
//! the header constructors and never updated afterwards. Foreign frees resolve the
//! owning heap through `RouteDirectory` (`Entry.owner` /
//! `canonical_root_for`), never through the header word. This file pins that
//! routing on the two paths where the removed stamping used to run:
//!
//! - Scenario A (magazine refill): the owner heap allocates enough small
//!   blocks to cross a magazine refill, then a SECOND independent thread
//!   (its own registry slot, bound via a warmup alloc) frees them all.
//! - Scenario B (Large cache hit): the owner heap allocates a Large block,
//!   frees it into the Large cache, re-allocates the same size (the cache-HIT
//!   path — exactly where the removed code used to reset `owner_state`), and
//!   the second thread foreign-frees the re-issued pointer.
//!
//! COUNTERFACTUAL (behavior-preservation guard for the routing invariant —
//! NOT red-on-revert): the actual pre-removal code already routed foreign
//! frees through the route directory, so reverting the stamp removal alone
//! does NOT turn this test red. What it catches is a REGRESSION in which
//! foreign-free routing starts consulting the header `owner_state` word
//! (permanently `OWNER_ID_NONE` after this change): such a free would fail
//! to resolve an owner and take the foreign/unroutable path — incrementing
//! `AllocCore::dbg_foreign_or_unroutable_frees` and never returning the
//! block to the owner. On any routing that reads `owner_state` the test is
//! red: the counter assertions detect the increment, and the owner-side
//! assertions (every freed small block FREE after a sidecar-ingress drain;
//! the freed 4 MiB span re-issued at the SAME pointer from the owner's
//! Large cache) detect the block never coming back.
//!
//! Serial, no subprocesses, no global allocator installation: the foreign
//! thread uses a local `static ALLOC: SeferAlloc` (never
//! `#[global_allocator]`), and the registry is claimed under a `SerialGuard`
//! so the owner lease and the foreign thread's warmup slot cannot collide.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals"
))]
// `alloc-decommit` is required because Scenario B's Large cache is compiled
// only under it (src/alloc_core/large/alloc_core_large.rs); in CI this test
// runs in check-matrix row `test-r8-production` and, with the cache-hit
// counter assertion, in the `production alloc-stats bench-internals
// internals` test step.

use core::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::SeferAlloc;

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

/// Raw-pointer wrapper so freed blocks can cross a thread boundary.
struct SendPtr(*mut u8);
unsafe impl Send for SendPtr {}

fn small_layout() -> Layout {
    Layout::from_size_align(16, 8).expect("valid small layout")
}

fn large_layout() -> Layout {
    Layout::from_size_align(4 * 1024 * 1024, 8).expect("valid large layout")
}

/// Enough 16 B blocks to force at least one magazine refill (the magazine cap
/// is 16 blocks, so 64 forces several refill cycles — the same sizing pattern
/// the retired `tests/heap_core_tcache_stamp.rs` used).
const REFILL_BLOCKS: usize = 64;

#[test]
fn foreign_frees_route_without_the_owner_stamp() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // The OWNER heap: claimed and held for the whole test, which guarantees
    // the foreign thread's warmup alloc below binds a DIFFERENT registry slot.
    let mut lease = HeapRegistry::dbg_claim_lease().expect("exclusive heap lease");
    let heap = lease.core();

    static ALLOC: SeferAlloc = SeferAlloc::new();

    // ── Scenario A: magazine refill, then foreign free of every block ─────
    let mut small: Vec<*mut u8> = Vec::with_capacity(REFILL_BLOCKS);
    for i in 0..REFILL_BLOCKS {
        let p = heap.alloc(small_layout());
        assert!(!p.is_null(), "scenario A: alloc null at i={i}");
        // SAFETY: `p` is a live 16 B allocation issued above.
        unsafe { core::ptr::write_bytes(p, (i & 0xFF) as u8, 16) };
        small.push(p);
    }

    let unroutable_before_a = AllocCore::dbg_foreign_or_unroutable_frees();

    {
        let (tx, rx) = mpsc::channel::<SendPtr>();
        for &p in &small {
            tx.send(SendPtr(p)).expect("send scenario A pointer");
        }
        let worker = std::thread::spawn(move || {
            // Warmup alloc binds this thread's TLS to its own (different)
            // registry slot, making every free below a genuinely FOREIGN free.
            let warmup = unsafe { ALLOC.alloc(small_layout()) };
            if !warmup.is_null() {
                // SAFETY: `warmup` was issued by this thread with this layout.
                unsafe { ALLOC.dealloc(warmup, small_layout()) };
            }
            while let Ok(SendPtr(p)) = rx.recv() {
                // SAFETY: `p` is a live allocation from the owner heap with
                // the same layout; this thread frees it exactly once.
                unsafe { ALLOC.dealloc(p, small_layout()) };
            }
        });
        drop(tx);
        worker.join().expect("scenario A worker panicked");
    }

    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees(),
        unroutable_before_a,
        "scenario A: every foreign small free must resolve an owner through \
         the route directory — an increment here means a free took the \
         foreign/unroutable path (the counterfactual)"
    );

    // The routed frees must have reached the OWNER heap: drain its sidecar
    // ingress and confirm every block is on the owner's free list.
    heap.dbg_drain_sidecar_ingress();
    for (i, &p) in small.iter().enumerate() {
        assert!(
            heap.dbg_is_free_for(p),
            "scenario A: block {i} did not return to the owning heap's free \
             list after the foreign free"
        );
    }

    // ── Scenario B: Large cache HIT, then foreign free of the hit pointer ──
    let first = heap.alloc(large_layout());
    assert!(!first.is_null(), "scenario B: large alloc failed");
    // SAFETY: `first` is a live 4 MiB allocation issued above.
    unsafe { core::ptr::write_bytes(first, 0x5A, large_layout().size()) };
    // SAFETY: exact layout, freed exactly once.
    unsafe { heap.dealloc(first, large_layout()) };

    // Cache-hit path: the same size comes straight back out of the Large
    // cache. This is the path where the removed code used to reset
    // `owner_state`. The hit itself is proven by the cache-hit counter.
    // The counter increments only under `alloc-stats`; that row
    // (`production alloc-stats bench-internals internals`) proves the hit.
    #[cfg(feature = "alloc-stats")]
    let hits_before = heap.dbg_large_cache_hits();
    let second = heap.alloc(large_layout());
    assert!(
        !second.is_null(),
        "scenario B: large cache-hit alloc failed"
    );
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        heap.dbg_large_cache_hits(),
        hits_before + 1,
        "scenario B: the re-alloc must be a Large-cache HIT — the \
         dbg_large_cache_hits counter is the path-activation oracle for \
         the cache-hit path this scenario targets"
    );
    assert!(
        heap.dbg_segment_registered_for(second),
        "scenario B: the cache-hit block must resolve in the owner's table"
    );

    let unroutable_before_b = AllocCore::dbg_foreign_or_unroutable_frees();
    {
        let (tx, rx) = mpsc::channel::<SendPtr>();
        tx.send(SendPtr(second)).expect("send scenario B pointer");
        let worker = std::thread::spawn(move || {
            let warmup = unsafe { ALLOC.alloc(small_layout()) };
            if !warmup.is_null() {
                // SAFETY: `warmup` was issued by this thread with this layout.
                unsafe { ALLOC.dealloc(warmup, small_layout()) };
            }
            let SendPtr(p) = rx.recv().expect("recv scenario B pointer");
            // SAFETY: `p` is a live Large allocation from the owner heap with
            // the exact 4 MiB layout; freed exactly once here.
            unsafe { ALLOC.dealloc(p, large_layout()) };
        });
        drop(tx);
        worker.join().expect("scenario B worker panicked");
    }

    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees(),
        unroutable_before_b,
        "scenario B: the foreign free of the Large cache-hit block must \
         resolve its owner through the route directory, not take the \
         foreign/unroutable path"
    );

    // The routed Large free must have been consumed by the owner heap: drain
    // its sidecar ingress and re-allocate the same 4 MiB — it must come back
    // at the SAME pointer, which only holds if the freed span returned to the
    // owner's Large cache (a dropped/unroutable free could never satisfy
    // this, matching Scenario A's strength).
    heap.dbg_drain_sidecar_ingress();
    let third = heap.alloc(large_layout());
    assert_eq!(
        third, second,
        "scenario B: the foreign-freed Large span must return to the owner's \
         Large cache and be re-issued at the SAME pointer — a dropped or \
         unroutable free could not satisfy this"
    );
    // SAFETY: `third` is a live allocation issued just above.
    unsafe { heap.dealloc(third, large_layout()) };

    // Recycle: the lease's Drop publishes LIVE → FREE (Release).
    drop(lease);
}
