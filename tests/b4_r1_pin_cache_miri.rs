//! Plain-Miri cell for the b4-r1 per-thread route pin cache. This proves, with
//! real cross-thread frees, that the cached-pin fast path is sound: a cache
//! MISS followed by a cache HIT on the same segment base, publication through
//! `publish_small_borrowed` while the pin is retained by the TLS cache, and the
//! TLS thread-exit release of a retained pin against a route that is still
//! live (the descriptor outlives via the owner ref — no leak, no UAF).
//!
//! The cache-hit path performs exactly one shared RMW (`pending.fetch_or`) on
//! the sidecar and takes no lock. Owner allocations run concurrently with the
//! producer publications so owner frames overlap the worker frees; a start
//! gate maximises that window without sleeping.
//!
//! Address transport uses exposed provenance (usize round-trip). Plain
//! provenance, Stacked Borrows and preemption 0.5 come from the CI job's
//! MIRIFLAGS; this is not a strict-provenance or Tree-Borrows acceptance
//! target.
//!
//! The whole test asserts a zero delta of
//! `AllocCore::dbg_foreign_or_unroutable_frees`: every free here must resolve
//! through the cached/uncached route paths, never as foreign-or-unroutable.

#![cfg(all(
    all(feature = "alloc-global", feature = "alloc-xthread"),
    feature = "internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::bootstrap;
use sefer_alloc::SeferAlloc;

// One shared allocator, driven directly through its `GlobalAlloc` impl (NOT
// installed as this binary's `#[global_allocator]`; mirrors
// `stress_concurrent_boundaries.rs`). `SeferAlloc` is `Sync`; every thread
// binds its own per-thread heap — and pin cache — via TLS on first use.
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Serialise against sibling tests of this binary: the pin cache and the route
// registry are process-global TLS/static state, and the libtest harness may
// run this file's tests in parallel threads.
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

#[global_allocator]
static ALLOCATOR: SeferAlloc = SeferAlloc::new();

#[test]
fn pin_cache_hit_and_thread_exit_release_miri() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // Miri runs ~1e5x slower than native; keep the block count tiny under it.
    const BLOCKS: usize = if cfg!(miri) { 16 } else { 64 };
    const SIZE: usize = 256;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    let foreign_before = AllocCore::dbg_foreign_or_unroutable_frees();

    // Owner: allocate every block up front so all frees land on registered,
    // owner-stamped segments. The owner keeps none — all blocks are handed to
    // the workers, so the post-join block below exercises the own path on an
    // otherwise quiescent heap.
    let mut blocks: Vec<usize> = Vec::with_capacity(BLOCKS);
    for i in 0..BLOCKS {
        // SAFETY: `layout` is a valid non-zero-sized layout.
        let p = unsafe { GLOBAL.alloc(layout) };
        assert!(!p.is_null(), "owner alloc[{i}] returned null");
        // Exposed-provenance transport: raw pointers are `!Send`.
        blocks.push(p as usize);
    }

    let split = BLOCKS / 2;
    let start = Arc::new(AtomicBool::new(false));

    let start_a = Arc::clone(&start);
    let mut a_blocks = blocks.split_off(split);
    // Worker A: the first free of a fresh segment is a cache MISS + insert;
    // the second free has the SAME segment base, so it takes the cached
    // fast path (HIT). The rest churn the same cache.
    let first_a = a_blocks.remove(0);
    let worker_a = thread::spawn(move || {
        let _ = bootstrap::ensure();
        while !start_a.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // SAFETY: `first_a` was allocated by the owner with `layout`.
        unsafe { GLOBAL.dealloc(first_a as *mut u8, layout) };
        // Same segment base as `first_a`: this is the cache HIT.
        // SAFETY: same provenance as above, same layout.
        unsafe { GLOBAL.dealloc(a_blocks.remove(0) as *mut u8, layout) };
        for &addr in &a_blocks {
            // SAFETY: each address was allocated by the owner with `layout`.
            unsafe { GLOBAL.dealloc(addr as *mut u8, layout) };
        }
    });

    let start_b = Arc::clone(&start);
    // B frees ONLY this block: the resulting pin is inserted into B's own TLS
    // cache and stays retained when B returns, so B's thread exit runs the TLS
    // Drop and releases the pin while the route is still live (its descriptor
    // survives via the owner ref). The rest of B's half stays with the owner,
    // which reclaims it after the join.
    let first_b = blocks.remove(0);
    let worker_b = thread::spawn(move || {
        let _ = bootstrap::ensure();
        while !start_b.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // SAFETY: `first_b` was allocated by the owner with `layout`.
        unsafe { GLOBAL.dealloc(first_b as *mut u8, layout) };
    });

    // Owner: release the gate and immediately service its own small
    // allocation so owner frames overlap the worker publications.
    start.store(true, Ordering::Release);

    worker_a.join().unwrap();
    worker_b.join().unwrap();

    // B exited with a pin retained (its first free); the rest of B's blocks
    // are returned here so the leak checker sees a balanced heap while the
    // route is still live — the retained pin's descriptor outlives via the
    // owner ref, so these are routed frees, not foreign ones.
    for &addr in &blocks {
        // SAFETY: each address was allocated by the owner with `layout`.
        unsafe { GLOBAL.dealloc(addr as *mut u8, layout) };
    }

    // Owner drains nothing extra: every block was given away. One further
    // alloc+dealloc of its own keeps the leak checker satisfied and exercises
    // the own (uncached) path after both producers are gone.
    // SAFETY: `layout` is a valid non-zero-sized layout.
    let p = unsafe { GLOBAL.alloc(layout) };
    assert!(!p.is_null(), "owner post-join alloc returned null");
    // SAFETY: `p` came from `alloc` with `layout` above.
    unsafe { GLOBAL.dealloc(p, layout) };

    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees() - foreign_before,
        0,
        "every free must resolve via the routed paths, none foreign/unroutable"
    );
}
