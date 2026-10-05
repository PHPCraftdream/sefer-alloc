//! Ph5c gap (в) — `trim_current_thread()` idempotency, trim while another
//! thread is live, trim after joining that thread, and trim with live objects
//! (matrix #2095, inventory gap P3, item 6).
//!
//! Not duplicated from the existing coverage:
//! - `tests/r31_10_trim_current_thread_api.rs` — AC1 equivalence, AC2 single
//!   alloc→trim→alloc, AC3 cross-thread isolation, AC4 no-bind/TORN no-ops;
//! - `tests/r7_cold_small_trim_current.rs` — cold small-pool trim + the
//!   `on_joined_thread` pattern;
//! - `tests/r11_ph4b_no_worker_pending_stays.rs` oracle (b) — *indirect*
//!   repeated-trim observation via the worker policy.
//!
//! What is NEW here:
//! 1. **Explicit double-trim idempotency**: after a churn phase, the FIRST
//!    `trim_current_thread()` is allowed to release an arbitrary (≥ 0) number
//!    of segments; the SECOND, back-to-back trim MUST be a release no-op
//!    (`segments_released_total` delta == 0) and must leave the allocator
//!    fully functional.
//! 2. **Trim on the calling thread while a peer thread is alive** (the peer
//!    also trims itself twice), then **trim again after `join`** — every call
//!    succeeds and the calling thread keeps allocating.
//! 3. **Live objects across trims**: objects allocated BEFORE any trim keep
//!    their exact byte contents through the peer thread's lifetime, both
//!    trims, and the join; they are dropped cleanly afterwards and fresh
//!    `Box`/`Vec` round-trips still succeed with correct contents.
//!
//! Hygiene: the whole body runs on a dedicated worker thread (the registry
//! and `segments_released_total` are process-global); the harness thread only
//! joins. The peer thread is coordinated with a mutex/condvar gate with a
//! bounded poll (`wait_timeout`) — no unbounded spin-waits; peer panics are
//! re-raised via `join().unwrap()`.
//!
//! Run:
//!   cargo test --test r11_ph5c_trim_idempotent_after_join --features "production internals"

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

// Installed as the process global allocator so the `Box`/`Vec` round-trips
// below (and the peer thread's churn) genuinely exercise SeferAlloc, not
// `System`. This binary never saturates the registry, so the shared global
// installation cannot interfere with the observations above.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const LIVE_OBJECTS: usize = 32;
const LIVE_PATTERN: u8 = 0xC7;
// Enough live small bytes (> `SEGMENT` = 4 MiB) that the heap's CURRENT
// small segment is a freshly-carved `Small` segment full of live blocks —
// NOT the `Primordial` one. This is what puts the
// `live_count_of() != 0` guard of `release_empty_current_small_for_trim`
// (the trim path `trim_current_thread` drives) genuinely under test.
const LIVE_SMALL_BLOCKS: usize = 96 * 1024;
const SMALL_LAYOUT: Layout = match Layout::from_size_align(64, 16) {
    Ok(l) => l,
    Err(_) => panic!("bad const layout"),
};

#[test]
fn trim_idempotent_across_peer_lifetime_and_join() {
    // Hygiene: whole body on its own thread; harness thread only joins.
    let handle = std::thread::Builder::new()
        .name("ph5c-trim-body".into())
        .spawn(trim_body)
        .expect("spawn body thread");
    handle.join().expect("body thread panicked");
}

fn trim_body() {
    let allocator = SeferAlloc::new();

    // ---- Live objects allocated BEFORE any trim ---------------------------
    let live: Vec<Box<[u8; 256]>> = (0..LIVE_OBJECTS)
        .map(|i| {
            let mut b = Box::new([0u8; 256]);
            b.fill((i as u8).wrapping_mul(7).wrapping_add(LIVE_PATTERN));
            b
        })
        .collect();
    assert_live_objects_intact(&live, "before any trim");

    // Live small blocks spanning fresh Small segments (> 4 MiB total), each
    // with a position-stamped fill, so a trim that releases a segment with
    // live blocks corrupts observably.
    let mut live_small: Vec<usize> = Vec::with_capacity(LIVE_SMALL_BLOCKS);
    for i in 0..LIVE_SMALL_BLOCKS {
        // SAFETY: valid non-zero layout; each block is freed exactly once.
        unsafe {
            let ptr = allocator.alloc(SMALL_LAYOUT);
            assert!(!ptr.is_null(), "live small alloc returned null");
            ptr.write_bytes((i % 251) as u8 + 1, SMALL_LAYOUT.size());
            live_small.push(ptr as usize);
        }
    }
    assert_live_small_intact(&live_small, "before any trim");

    // Some churn on THIS thread so its heap has cached/pooled state to trim.
    churn(&allocator);

    // ---- Peer thread: churn + double trim, gated until we trim mid-flight -
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let peer_gate = gate.clone();
    let peer = std::thread::spawn(move || {
        let (lock, cv) = &*peer_gate;
        // Bounded wait for the gate; a lost wakeup times out and re-checks.
        let mut started = lock.lock().expect("gate mutex poisoned");
        while !*started {
            let (guard, timeout) = cv
                .wait_timeout(started, Duration::from_secs(10))
                .expect("gate mutex poisoned while waiting");
            started = guard;
            if timeout.timed_out() && !*started {
                panic!("peer gate never opened (10 s deadline)");
            }
        }
        drop(started);
        let peer_alloc = SeferAlloc::new();
        churn(&peer_alloc);
        peer_alloc.trim_current_thread();
        peer_alloc.trim_current_thread(); // peer-side double trim: no-op + no panic
    });

    // ---- Trim on THIS thread while the peer is alive ----------------------
    // The peer is parked on the gate, so "alive" is deterministic here.
    allocator.trim_current_thread();
    {
        let (lock, cv) = &*gate;
        *lock.lock().expect("gate mutex poisoned") = true;
        cv.notify_all();
    }

    // ---- Join, then trim again after the join ------------------------------
    peer.join().expect("peer thread panicked");
    allocator.trim_current_thread();

    // ---- Explicit idempotency oracle: second back-to-back trim = no-op ----
    churn(&allocator);
    let released_after_first = {
        let before = SeferAlloc::new().stats().segments_released_total;
        allocator.trim_current_thread();
        let after = SeferAlloc::new().stats().segments_released_total;
        after.saturating_sub(before) // first trim of the pair: delta ≥ 0, unchecked
    };
    let _ = released_after_first; // any value is legal for the FIRST trim
    {
        let before = SeferAlloc::new().stats().segments_released_total;
        allocator.trim_current_thread();
        let after = SeferAlloc::new().stats().segments_released_total;
        assert_eq!(
            after, before,
            "second back-to-back trim released segments (non-idempotent): \
             {before} -> {after}"
        );
    }

    // ---- Live objects survived everything ---------------------------------
    assert_live_objects_intact(&live, "after trims + peer join");
    assert_live_small_intact(&live_small, "after trims + peer join");
    for addr in live_small.drain(..) {
        // SAFETY: exact original layout, single free of a live block.
        unsafe {
            allocator.dealloc(
                std::ptr::with_exposed_provenance_mut::<u8>(addr),
                SMALL_LAYOUT,
            );
        }
    }
    drop(live);

    // ---- Allocator still fully functional after all trims ------------------
    // Raw-face round trip on the trimmed thread.
    let layout = Layout::from_size_align(128, 16).expect("valid layout");
    // SAFETY: valid non-zero layout; single alloc, init, read, free.
    unsafe {
        let ptr = allocator.alloc(layout);
        assert!(!ptr.is_null(), "post-trim raw alloc returned null");
        ptr.write_bytes(0x3C, layout.size());
        assert_eq!(*ptr.add(layout.size() - 1), 0x3C, "raw block corrupted");
        allocator.dealloc(ptr, layout);
    }
    // Collection round trips on the trimmed thread (through the installed
    // global allocator above).
    let v: Vec<u32> = (0u32..4096).map(|i| i.wrapping_mul(2654435761)).collect();
    assert_eq!(v.len(), 4096, "Vec round-trip length");
    assert_eq!(v[1234], 1234u32.wrapping_mul(2654435761), "Vec contents");
    let b = Box::new([7u8; 64]);
    assert_eq!(b[63], 7, "Box round-trip contents");
}

fn assert_live_small_intact(addrs: &[usize], phase: &str) {
    for (i, &addr) in addrs.iter().enumerate() {
        let expected = (i % 251) as u8 + 1;
        // SAFETY: `addr` is a live uniquely-owned SMALL_LAYOUT allocation.
        let block = unsafe {
            std::slice::from_raw_parts(
                std::ptr::with_exposed_provenance::<u8>(addr),
                SMALL_LAYOUT.size(),
            )
        };
        assert!(
            block.iter().all(|&byte| byte == expected),
            "live small block {i} corrupted {phase}: expected 0x{expected:02X} fill",
        );
    }
}

fn churn(allocator: &SeferAlloc) {
    let layout = Layout::from_size_align(64, 16).expect("valid layout");
    for _ in 0..64 {
        // SAFETY: valid non-zero layout; single alloc/free per iteration.
        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null(), "churn alloc returned null");
            ptr.write_bytes(0x11, layout.size());
            allocator.dealloc(ptr, layout);
        }
    }
    // Also churn through collection-shaped layouts (driven through the raw
    // face for a deterministic route; the global-allocator face is covered by
    // the live `Box`es and the final round-trips).
    for size in [256usize, 1024, 4096] {
        // SAFETY: valid non-zero layout; single alloc/free per iteration.
        unsafe {
            let layout = Layout::array::<u8>(size).expect("valid array layout");
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null(), "churn Vec-shaped alloc returned null");
            allocator.dealloc(ptr, layout);
        }
    }
}

fn assert_live_objects_intact(live: &[Box<[u8; 256]>], phase: &str) {
    for (i, b) in live.iter().enumerate() {
        let expected = (i as u8).wrapping_mul(7).wrapping_add(LIVE_PATTERN);
        assert!(
            b.iter().all(|&byte| byte == expected),
            "live object {i} corrupted {phase}: expected 0x{expected:02X} fill"
        );
    }
}
