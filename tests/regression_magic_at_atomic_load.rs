//! R6-MS-5 regression — `SegmentHeader::magic_at` must be an ATOMIC load.
//!
//! `magic` is atomically zeroed (`AtomicU32::store(0, Release)`) when a Large
//! segment is recycled back to the OS-reservation cache
//! (`AllocCore::dealloc`'s Large-cache-deposit branch, and
//! `AllocCore::alloc_large`'s eviction branch — UBFIX-6). The cross-thread
//! dealloc-routing path validates a segment base with `magic_at` BEFORE
//! touching any further header state. Pre-R6-MS-5, `magic_at` was a PLAIN
//! (`Node::read_u32`) load — mixing a non-atomic read with the recycler's
//! atomic store on the same field from different threads is a data race under
//! Rust's memory model, independent of whether the current calling protocol
//! happens to serialize the two. The defensive-free contract exists precisely
//! to stay safe under caller misuse (a stale/duplicate remote free is exactly
//! the misuse that interleaves the recycler's zeroing store with this read),
//! so the validation route itself must not become a data-race source when that
//! misuse happens.
//!
//! ## What this test exercises
//!
//! The recycle-then-cross-thread-dealloc-routing interleaving the field
//! protects. The owner thread deallocates a batch of Large segments to the
//! cache (each dealloc = one atomic `magic = 0` Release store), CONCURRENTLY
//! with a remote thread that cross-thread-frees the SAME addresses (each free
//! = one `magic_at` load via `dealloc_routing`). The two access the `magic`
//! field of the same segments from different threads with no synchronisation
//! between store and load other than the field's own atomicity.
//!
//! This is a deliberate stale/duplicate-remote-free scenario — caller misuse
//! under the `unsafe fn` dealloc contract (R6-MS-1/2) — exercised concurrently
//! to provoke the access-kind mismatch. The assertions are the defensive
//! contract the crate guarantees on such misuse: no crash, and the heap
//! remains usable afterward. Under ThreadSanitizer this is the data-race
//! regression for the access-kind fix: pre-fix TSan reports a race on the
//! `magic` field (plain read vs atomic store); post-fix (atomic Acquire load
//! pairing the Release store) it is clean. The counterfactual was verified by
//! hand during development (revert `magic_at` to `Node::read_u32` → TSan
//! reports the race; restore → clean).
//!
//! ## Design note: why the owner does only deallocs (no alloc-reuse)
//!
//! The alloc-reuse (large-cache-hit) path rewrites the WHOLE header via a
//! non-atomic `Node::write_struct` — a separate, pre-existing §11 surface. To
//! keep THIS test focused on the `magic` access-kind mismatch (atomic
//! zero-store vs `magic_at` load) and not conflate it with that other race,
//! the owner here performs ONLY dealloc-to-cache (whose sole header effect is
//! the atomic `magic = 0` store, per UBFIX-6) — no concurrent alloc-reuse, so
//! no `write_struct` runs against the remote's reads.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise: the registry is a process-global static; concurrent test-fn
// execution against it would conflate this test's reclaims with another's.
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

/// R6-MS-5: `magic_at` is an atomic Acquire load, so the cross-thread
/// dealloc-routing base validation does not race the recycler's atomic
/// `magic = 0` Release store when a stale/duplicate remote free interleaves
/// with a Large-segment recycle-to-cache.
#[test]
fn magic_at_atomic_load_survives_recycle_xthread_race() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // 2 MiB — comfortably above `SMALL_MAX` in every feature combination
    // (even `medium-classes`, which raises it to 1 MiB), so every allocation
    // is unambiguously routed to the Large path (whose dealloc-to-cache
    // branch performs the atomic `magic = 0` Release store this test races
    // against).
    const SIZE: usize = 2 * 1024 * 1024;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    // Modest batch — stays within the large cache's slot count so the owner's
    // deallocs DEPOSIT to cache (zeroing `magic`) rather than evicting to OS
    // release. Miri-shrink: the concurrent thread spawn is the cost under
    // miri; native keeps the batch + iteration count that reliably overlaps
    // the store and load windows.
    #[cfg(not(miri))]
    const M: usize = 4;
    #[cfg(not(miri))]
    const ITERS: usize = 200;
    #[cfg(miri)]
    const M: usize = 2;
    #[cfg(miri)]
    const ITERS: usize = 4;

    let owner_heap = HeapRegistry::claim();
    assert!(!owner_heap.is_null(), "HeapRegistry::claim returned null");

    // ── Owner allocates M live Large segments (magic == SEGMENT_MAGIC). ─────
    let ptrs: Vec<*mut u8> = (0..M)
        .map(|_| unsafe { (*owner_heap).alloc(layout) })
        .collect();
    for (i, &p) in ptrs.iter().enumerate() {
        assert!(!p.is_null(), "owner alloc[{i}] returned null");
    }
    // Ship the addresses (raw pointers are `!Send`) to the remote as `usize`.
    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();

    // ── Remote thread: cross-thread-free the SAME addresses in a tight loop.
    // Each `dealloc` goes through `dealloc_routing` → `magic_at` (Acquire load
    // post-fix; plain load pre-fix) on the very segments the owner is
    // concurrently recycling below.
    let remote = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        for _ in 0..ITERS {
            for &addr in &addrs {
                // SAFETY (R6-MS-1/2 + raw-deref): `remote_heap` is the live
                // heap this remote thread claimed. `addr` is a stale address
                // the owner allocated and is concurrently recycling — a
                // deliberate duplicate/stale remote free (caller misuse under
                // the `unsafe fn` dealloc contract) exercised concurrently to
                // provoke the `magic_at`-vs-`magic = 0` access-kind mismatch.
                // The allocator's defensive routing degrades it to a no-op
                // (magic mismatch → foreign/no-op branch) or a guarded
                // deferred-free push (double-push CAS guard), never corrupting
                // the substrate.
                unsafe { (*remote_heap).dealloc(addr as *mut u8, layout) };
            }
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    });

    // ── Owner: dealloc each segment to cache CONCURRENTLY with the remote's
    // reads. Each dealloc performs exactly ONE atomic `magic = 0` Release
    // store (UBFIX-6); no alloc-reuse runs here, so the only header write the
    // remote's `magic_at` load can race is this zeroing store. A brief spin
    // between deallocs spreads the stores across the remote's read window.
    for &p in &ptrs {
        for _ in 0..32 {
            std::hint::spin_loop();
        }
        // SAFETY (R6-MS-1/2): `owner_heap` is the live heap this thread
        // claimed; `p` is a live allocation it owns (allocated above). This
        // single own-thread free recycles the segment to the large cache
        // (magic → 0), the racing store.
        unsafe { (*owner_heap).dealloc(p, layout) };
    }

    remote.join().expect("remote cross-thread freer panicked");

    // ── Defensive contract: the concurrent stale/duplicate remote free +
    // recycle did not corrupt the substrate. The owner heap is still usable.
    let final_small = Layout::from_size_align(16, 8).unwrap();
    let p = unsafe { (*owner_heap).alloc(final_small) };
    assert!(!p.is_null(), "heap unusable after recycle+xthread race");
    unsafe { (*owner_heap).dealloc(p, final_small) };

    unsafe { HeapRegistry::recycle(owner_heap) };
}
