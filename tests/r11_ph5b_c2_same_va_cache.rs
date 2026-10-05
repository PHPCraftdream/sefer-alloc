//! Owner-heap Large-cache reuse after a worker remotely frees its allocation.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "alloc-stats"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::thread;

use sefer_alloc::{LargeCacheConfig, SeferAlloc};

const MIB: usize = 1024 * 1024;
const LAYOUT: Layout = match Layout::from_size_align(2 * MIB, 8) {
    Ok(layout) => layout,
    Err(_) => panic!("invalid large layout"),
};
const CONFIG: LargeCacheConfig = LargeCacheConfig::new().budget_bytes(32 * MIB);
static ALLOC: SeferAlloc = SeferAlloc::with_config(CONFIG);
static REMOTE_FREE: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());

#[test]
fn owner_reuses_large_va_after_remote_free_worker_join() {
    // Initialize and allocate on the owner thread, establishing the heap whose
    // cache is observed again after the worker publishes its remote free.
    // SAFETY: valid layout; owner keeps allocation live until worker frees it.
    let original = unsafe { ALLOC.alloc(LAYOUT) };
    assert!(!original.is_null(), "owner allocation failed");
    // SAFETY: the allocation is live and spans the exact layout.
    unsafe { original.write_bytes(0xA7, LAYOUT.size()) };
    REMOTE_FREE.store(original, Ordering::Release);
    let original_va = original as usize;

    let worker = thread::spawn(|| {
        let ptr = REMOTE_FREE.swap(ptr::null_mut(), Ordering::Acquire);
        assert!(!ptr.is_null(), "owner did not publish allocation");
        // SAFETY: this pointer was allocated by ALLOC with LAYOUT and is freed
        // exactly once here; AtomicPtr transfers its pointer value to worker.
        unsafe { ALLOC.dealloc(ptr, LAYOUT) };
    });
    worker.join().expect("worker panicked");

    let hits_before = ALLOC.stats().large_cache_hits;
    // The owner allocation drains the worker's terminal publication and
    // reclaims the freed Large span into this same heap's cache.
    // SAFETY: valid layout; validate the allocation and free it below.
    unsafe {
        let reused = ALLOC.alloc_zeroed(LAYOUT);
        assert!(!reused.is_null(), "owner allocation after join failed");
        let hits_after = ALLOC.stats().large_cache_hits;
        assert_eq!(
            hits_after - hits_before,
            1,
            "expected exactly one Large cache hit"
        );
        assert_eq!(
            reused as usize, original_va,
            "expected cached allocation at original VA"
        );
        for index in 0..LAYOUT.size() {
            assert_eq!(reused.add(index).read(), 0, "nonzero byte at {index}");
        }
        reused.write(0x5C);
        assert_eq!(reused.read(), 0x5C);
        ALLOC.dealloc(reused, LAYOUT);
    }
}
