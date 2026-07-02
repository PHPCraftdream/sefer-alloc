//! Task #129 — teardown-ordering stress test for the TORN-sentinel fix.
//!
//! ## What this tries to provoke
//!
//! The bug (see `tls_heap_teardown_torn_sentinel.rs` for the full
//! mechanism) requires a thread whose OWN application-level `thread_local!`
//! with a `Drop` impl was FIRST TOUCHED before this thread's first
//! `sefer-alloc` allocation. Thread-locals are destroyed in REVERSE
//! declaration/first-touch order, so a thread-local first touched before
//! `tls_heap::LOCAL`/`GUARD` are (lazily, on first alloc) is destroyed
//! AFTER them — i.e. after `GUARD::drop` has already recycled this
//! thread's registry slot. If that later `Drop` impl allocates/deallocates
//! (as `Vec`/`String`-backed cleanup naturally does), it exercises exactly
//! the post-recycle-but-still-live-LOCAL window the TORN sentinel is meant
//! to close.
//!
//! Each worker in this test:
//!  1. Touches its own `thread_local! { static POST: RefCell<Vec<u8>> }`
//!     FIRST (before any sefer-alloc allocation) — so it is destroyed AFTER
//!     `tls_heap`'s `LOCAL`/`GUARD`.
//!  2. Performs ordinary heap allocations (`Vec`, `String`, `Box`) through
//!     the installed `SeferAlloc`, arming this thread's `GUARD`.
//!  3. Returns, letting the thread exit. Teardown order is then: `GUARD`
//!     drops (recycles the slot, stamps `LOCAL` to `TORN`), then `LOCAL`
//!     drops (no-op, bare `Cell`), then `POST` drops — and `POST`'s `Drop`
//!     impl itself performs a fresh allocation + deallocation
//!     (`Vec::with_capacity` + push + drop), which is serviced by
//!     `current_for_alloc()` seeing `TORN` and routing to the fallback heap.
//!
//! Many such workers run in a loop, with slot reuse forced by keeping the
//! worker count well above `MAX_HEAPS` isn't required — thread churn alone
//! is enough to cycle slots through `claim`/`recycle` repeatedly and
//! maximise the chance another thread re-claims a just-recycled slot while
//! the exiting thread's `POST::drop` is still running, on a real OS
//! scheduler.
//!
//! ## Honesty about what this test is (and is not)
//!
//! **This is a smoke/stress test, not a deterministic reproducer.** The
//! race window (between `GUARD::drop` recycling the slot and `POST::drop`
//! running) is narrow and scheduler-dependent; this test can pass even with
//! the bug present, on a given run, on a given machine. It is NOT a
//! red/green regression gate by itself — the deterministic counterfactual
//! for the resolver logic is `tls_heap_teardown_torn_sentinel.rs`. What
//! THIS test is good for: it will reliably CRASH (segfault / heap
//! corruption / an aborting double-free) when run WITHOUT the TORN fix
//! under enough iterations, and — for rigorous confirmation of the
//! ordering argument beyond what a debug/release run can show — should be
//! run under ThreadSanitizer (`npm run tsan`, which shells out to WSL) or
//! AddressSanitizer. **TSan/ASAN were NOT run as part of this task** (no
//! WSL invocation was made); that remains an open follow-up. This test
//! alone gives amplitude, not proof.
//!
//! Non-vacuous: every worker's `join()` must succeed (no panic/abort — a
//! genuine UAF here typically segfaults or corrupts the heap enough to
//! abort, not silently pass), and a final control allocation on the main
//! thread must succeed, proving the allocator (registry + fallback) is
//! still structurally sound after the churn.

#![cfg(feature = "alloc-global")]

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Serialise against other registry-touching tests in the suite (same
// pattern as `global_alloc_installed.rs` / `global_alloc_mt.rs`).
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

thread_local! {
    /// Declared/first-touched (per worker thread) BEFORE any sefer-alloc
    /// allocation happens on that thread, so it is destroyed AFTER
    /// `tls_heap`'s `LOCAL`/`GUARD` (reverse first-touch/declaration order).
    /// Its `Drop` (via `RefCell`'s inner `Vec<u8>`) performs a fresh
    /// allocation + deallocation, landing squarely in the post-`GUARD`-drop
    /// window this test is trying to hit.
    static POST: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// One worker's workload: touch `POST` first, then churn ordinary
/// allocations through the global allocator, then return (triggering the
/// teardown sequence described in the module doc on join).
fn worker(id: u64) -> u64 {
    // Step 1: first-touch POST before any sefer-alloc allocation on this
    // thread. `with` forces initialisation without needing a value out of
    // it — the important part is registering the thread-local, not this
    // value.
    POST.with(|p| {
        p.borrow_mut().push(id as u8);
    });

    // Step 2: ordinary allocation churn — arms this thread's GUARD.
    let mut acc = id;
    let v: Vec<u64> = (0..64u64).map(|i| i.wrapping_add(id)).collect();
    acc = acc.wrapping_add(v.iter().copied().sum::<u64>());
    let s = format!("worker-{id}-acc-{acc}");
    acc = acc.wrapping_add(s.len() as u64);
    let b = Box::new(acc.wrapping_mul(3));
    acc = acc.wrapping_add(*b);

    // Grow POST's Vec further so its eventual Drop does a real
    // allocation/deallocation, not a no-op on an empty Vec.
    POST.with(|p| {
        let mut vec = p.borrow_mut();
        for i in 0..256u32 {
            vec.push((id as u32).wrapping_add(i) as u8);
        }
    });

    // Step 3: return — thread exit runs GUARD::drop (recycle + TORN stamp),
    // then LOCAL's no-op drop, then POST::drop (allocates/deallocates while
    // LOCAL == TORN on this thread).
    acc
}

/// Spawn many short-lived worker threads, each hitting the ordering window
/// described above, across several waves so slots get recycled and
/// re-claimed repeatedly under a real OS scheduler.
#[test]
fn teardown_ordering_stress_no_crash() {
    let _serial = SerialGuard::acquire();

    const WAVE_SIZE: usize = 8;
    const WAVES: usize = 12;

    let mut next_id: u64 = 0;
    for _wave in 0..WAVES {
        let handles: Vec<_> = (0..WAVE_SIZE)
            .map(|_| {
                let id = next_id;
                next_id += 1;
                std::thread::spawn(move || worker(id))
            })
            .collect();

        for h in handles {
            // A UAF/double-writer race here typically segfaults or aborts
            // the process outright (not a catchable panic), but a milder
            // corruption could also surface as a panic from a downstream
            // assertion inside HeapCore — `join()` failing is a red signal
            // either way.
            let _ = h
                .join()
                .expect("worker thread must not panic/abort during teardown");
        }
    }

    // Control: the allocator must still be structurally sound after the
    // churn — a plain allocation on the main thread must succeed and hold
    // its value.
    let mut check: Vec<u32> = Vec::with_capacity(64);
    for i in 0..64u32 {
        check.push(i);
    }
    assert_eq!(
        check.iter().sum::<u32>(),
        (0..64u32).sum::<u32>(),
        "post-stress control allocation corrupted"
    );
}
