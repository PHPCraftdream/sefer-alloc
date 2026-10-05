//! C6 system-allocator oracle: deallocation must not call System alloc/realloc.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "alloc-stats"
))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::{AllocStats, SeferAlloc};

struct CountSystem;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static REALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwards the caller's valid layout to System.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwards the original pointer and layout to System.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        REALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwards the original pointer/layout and requested size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static SYSTEM_COUNTER: CountSystem = CountSystem;

const SMALL: Layout = match Layout::from_size_align(512, 8) {
    Ok(layout) => layout,
    Err(_) => panic!("invalid small layout"),
};
const LARGE: Layout = match Layout::from_size_align(2 * 1024 * 1024, 8) {
    Ok(layout) => layout,
    Err(_) => panic!("invalid large layout"),
};
static REMOTE: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());
static START: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);

fn stats(a: &SeferAlloc) -> AllocStats {
    a.stats()
}

/// Bounded spin: a panicked or wedged peer fails the test instead of hanging.
fn wait_for(flag: &AtomicBool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !flag.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::hint::spin_loop();
    }
}

#[test]
fn deallocations_do_not_call_system_alloc_or_realloc() {
    static ALLOCATOR: SeferAlloc = SeferAlloc::new();
    let allocator = &ALLOCATOR;

    // Warm both heap-local paths and the route descriptor before measuring.
    unsafe {
        for layout in [SMALL, LARGE] {
            let p = allocator.alloc(layout);
            assert!(!p.is_null());
            allocator.dealloc(p, layout);
        }
        let route = allocator.alloc(SMALL);
        assert!(!route.is_null());
        allocator.dealloc(route, SMALL);
    }
    let (small, large, remote) = unsafe {
        let s = allocator.alloc(SMALL);
        let l = allocator.alloc(LARGE);
        let r = allocator.alloc(SMALL);
        assert!(!s.is_null() && !l.is_null() && !r.is_null());
        (s, l, r)
    };
    REMOTE.store(remote, Ordering::Relaxed);

    // Worker is created, heap-bound, and parked before the measured window.
    let worker_allocator = SeferAlloc::new();
    let worker = thread::spawn(move || {
        // Materialize this thread's heap/TLS state and free path before timing.
        let warm = unsafe { worker_allocator.alloc(SMALL) };
        assert!(!warm.is_null());
        unsafe { worker_allocator.dealloc(warm, SMALL) };
        let p = REMOTE.load(Ordering::Acquire);
        READY.store(true, Ordering::Release);
        wait_for(&START, "main START");
        // SAFETY: p is the live Small block allocated by the main thread.
        unsafe {
            ALLOCATOR.dealloc(p, SMALL);
        }
        DONE.store(true, Ordering::Release);
    });

    wait_for(&READY, "worker READY");
    let before_stats = stats(allocator);
    let before = (
        ALLOCS.load(Ordering::SeqCst),
        REALLOCS.load(Ordering::SeqCst),
    );

    START.store(true, Ordering::Release);
    // SAFETY: these are the still-live allocations and exact layouts.
    unsafe {
        allocator.dealloc(small, SMALL);
        allocator.dealloc(large, LARGE);
    }
    wait_for(&DONE, "worker DONE");
    let after = (
        ALLOCS.load(Ordering::SeqCst),
        REALLOCS.load(Ordering::SeqCst),
    );
    let after_stats = stats(allocator);
    worker.join().unwrap();

    assert_eq!(after.0, before.0, "System.alloc during dealloc");
    assert_eq!(after.1, before.1, "System.realloc during dealloc");
    assert_eq!(
        after_stats.segments_reserved_total,
        before_stats.segments_reserved_total
    );
    assert_eq!(
        after_stats.heaps_claimed_high_water,
        before_stats.heaps_claimed_high_water
    );
    assert_eq!(
        after_stats.foreign_or_unroutable_frees,
        before_stats.foreign_or_unroutable_frees
    );
}
