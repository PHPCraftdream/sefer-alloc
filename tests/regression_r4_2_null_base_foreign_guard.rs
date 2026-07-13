//! R4-2 / memory_safety_review (R4-MS-1/MS-2 kernel) — null-base guard in
//! `HeapCore::realloc`'s foreign leg and `HeapCore::dealloc_foreign_slow`.
//!
//! ## The defect
//!
//! Under `alloc-xthread`, `HeapCore::realloc`'s foreign-pointer leg and
//! `dealloc_foreign_slow` (reached from `HeapCore::dealloc` →
//! `dealloc_routing` when `contains_base(base)` is false) both did:
//!
//! ```text
//! let base = os::segment_base_of_ptr(ptr);
//! if SegmentHeader::magic_at(base) != SEGMENT_MAGIC { ... }
//! ```
//!
//! `segment_base_of_ptr` masks the address down to the SEGMENT boundary
//! (`ptr.map_addr(|a| a & !(SEGMENT - 1))`). For a garbage pointer like
//! `1 as *mut u8` this masks to `base == 0` (null). `magic_at(base)` is a raw
//! `u32` load at `base + offset_of!(SegmentHeader, magic)` — so with
//! `base == 0` it dereferences a low, structurally-impossible address with
//! ZERO guard beforehand. Immediate UB / crash, reachable from the 100%-safe
//! public `HeapCore::realloc` / `HeapCore::dealloc` with a garbage pointer.
//!
//! ## The fix
//!
//! In BOTH locations, immediately after `segment_base_of_ptr`, reject a null
//! `base` BEFORE the first `magic_at` read:
//!   - `realloc` foreign leg: `if base.is_null() { return null_mut(); }`
//!   - `dealloc_foreign_slow`:     `if base.is_null() { return; }`
//!
//! This narrows the genuinely-free class (a base that cannot be a real segment
//! by construction) without attempting cross-heap staleness detection — the
//! case-(a)-vs-(b) residual noted in `dealloc_foreign_slow`'s own doc comment
//! is explicitly out of scope and is NOT what this fix closes.
//!
//! ## Counterfactual (non-vacuity)
//!
//! With the guard REMOVED, `magic_at(0)` reads address
//! `offset_of!(SegmentHeader, magic)` (a low, never-mapped address) — under
//! miri this is reported as a memory-access UB error; on real hardware it
//! segfaults / AVs. With the guard PRESENT, both calls return null / no-op
//! before any read. The assertions below are the deterministic signal:
//! `realloc` returns null and `dealloc` is a safe no-op (the test completing
//! without crashing is the dealloc assertion).
//!
//! (The guard is a pure Rust null-pointer check, so it fires identically in
//! debug and release; this case has no debug/release nuance, unlike bug (a).)
//!
//! Per project convention: tests live in `tests/`, not inline.

#![cfg(feature = "alloc-xthread")]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests in this file: the registry is a process-global static.
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

/// The exact reviewed example: `1usize as *mut u8`. `segment_base_of_ptr`
/// masks it to `base == 0` (null), since SEGMENT is a power of two and the
/// address is below the first segment boundary.
///
/// `clippy::manual_dangling_ptr` is intentionally allowed: this is NOT a
/// "create a valid aligned dangling pointer" use — it is the deliberate
/// garbage integer address from the memory-safety review, the whole point
/// being that it masks to a null segment base.
#[allow(clippy::manual_dangling_ptr)]
const GARBAGE_PTR: *mut u8 = 1usize as *mut u8;

/// `realloc`'s foreign leg must return null for a garbage pointer whose
/// computed base is null, WITHOUT a raw read at address
/// `offset_of!(SegmentHeader, magic)`.
#[test]
fn realloc_foreign_garbage_null_base_returns_null() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let old_layout = Layout::from_size_align(16, 8).unwrap();
    // SAFETY: `heap` was just claimed and is LIVE + initialised; we are its
    // sole user (serialised). We hold a `&mut HeapCore` for this call only.
    let result = unsafe { (*heap).realloc(GARBAGE_PTR, old_layout, 64) };

    assert!(
        result.is_null(),
        "HeapCore::realloc on a garbage pointer masking to base 0 must return \
         null (null-base guard) before any raw segment-header read"
    );

    // SAFETY: done with the heap; return it to the pool.
    unsafe { HeapRegistry::recycle(heap) };
}

/// `dealloc`'s foreign leg (`dealloc_foreign_slow`) must be a safe no-op for a
/// garbage pointer whose computed base is null. The deterministic signal that
/// the guard fired is simply reaching the end of the test without crashing /
/// faulting (the unguarded path reads address 0 + magic-offset and faults).
#[test]
fn dealloc_foreign_garbage_null_base_is_safe_noop() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(16, 8).unwrap();
    // SAFETY: `heap` is LIVE + initialised and solely ours. The point of this
    // test is that the call must NOT read through a null base; if the guard
    // were absent this would fault reading address `offset_of!(magic)`.
    unsafe { (*heap).dealloc(GARBAGE_PTR, layout) };

    // Establish the allocator is still fully functional after the no-op: a
    // genuine alloc/dealloc round-trip must succeed, proving the garbage
    // dealloc neither corrupted state nor crashed.
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null(), "allocator unusable after garbage dealloc");
    unsafe { (*heap).dealloc(p, layout) };

    // SAFETY: done with the heap; return it to the pool.
    unsafe { HeapRegistry::recycle(heap) };
}
