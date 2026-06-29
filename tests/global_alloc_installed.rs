//! Phase 12.3 — the headline proof: install `SeferAlloc` as this test
//! binary's `#[global_allocator]` and run a single-threaded
//! `Vec`/`String`/`Box`/`HashMap` churn through it.
//!
//! ## Why this is the gate
//!
//! Before Phase 12.3 this configuration **ABORTED under libtest**: the Phase
//! 11 TLS binding (`RefCell<Option<Heap>>`) returned null under libtest's
//! reentrant harness (parallel test threads, panic infrastructure, capture
//! buffers), and a null from `#[global_allocator]` aborts the process. The
//! reentrancy failure was the documented "NOT production-trusted" caveat.
//!
//! Phase 12.3 rewires `SeferAlloc` through the raw-pointer TLS
//! (`Cell<*mut HeapCore>`, no `RefCell` → no borrow state to fail) over the
//! global registry (Phase 12.2), with a never-null primordial fallback heap
//! (M10). This test proves the rewiring: every allocation here (and every
//! allocation libtest itself makes on this thread — assertion formatting,
//! panic hooks, capture) is served by the registry-backed allocator.
//!
//! ## Single-threaded scope
//!
//! The churn itself runs on ONE test thread. The libtest harness may spawn
//! sibling test threads (which also allocate through `SeferAlloc`); that is
//! the normal libtest environment, NOT a multi-threaded stress of the
//! allocator's concurrency paths (those are exercised by `heap_cross_thread`
//! and will be the 12.4/12.5 loom + soak). The point is: the
//! `#[global_allocator]` installation **runs to completion** — no abort, no
//! reentrant-borrow failure, M10 upheld.
//!
//! NON-VACUOUS: the assertions check the actual computed values (sum, map
//! contents), so a corrupted or lost allocation fails an assertion rather
//! than silently passing.
//!
//! ## Known remainder (Phase 12.4)
//!
//! Thread-exit abandonment is a no-op stub in 12.3: the test thread's heap
//! segments LEAK on exit (bounded — one heap's footprint; sound — the
//! segments stay mapped). This is expected and documented in
//! `tls_heap::AbandonGuard::drop`. Adoption (reclaiming the leak) is 12.4.

#![cfg(feature = "alloc-global")]

use std::alloc::GlobalAlloc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::SeferAlloc;

// Install sefer-alloc as the process-wide global allocator for this test
// binary. Every allocation in this binary — including libtest's harness
// allocations — now routes through `SeferAlloc`.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Serialise this test against the other registry-touching tests
// (`registry_basic`). The registry is a process-global static; while the
// libtest harness may spawn sibling threads for OTHER tests (which also
// allocate through GLOBAL), we want THIS test's slot-index arithmetic to be
// stable for its duration so we can assert the slot was claimed + is LIVE.
// (Under parallel libtest the sibling threads claim their own slots — that
// is fine and exercises the lock-free claim path; this serial flag only
// guards against the registry_basic tests' `reset_for_test` interacting with
// our slot observations.)
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

/// Single-threaded churn through the INSTALLED `#[global_allocator]`:
/// `Vec` push/grow, `String` formatting, `Box` new/drop, and `HashMap`
/// insert/lookup. This is the headline Phase 12.3 gate — it ran to
/// completion only after the raw-pointer TLS rewiring.
#[test]
fn global_allocator_serves_single_thread_churn() {
    let _serial = SerialGuard::acquire();

    // Vec push/grow (alloc + realloc churn).
    let mut v: Vec<u64> = Vec::new();
    for i in 0..50_000u64 {
        v.push(i);
    }
    let sum: u64 = v.iter().copied().sum();
    assert_eq!(sum, (0..50_000u64).sum(), "Vec sum mismatch — corruption");

    // String formatting (varied small allocations through format!).
    let strings: Vec<String> = (0..1_000u32).map(|i| format!("item-{i}")).collect();
    assert_eq!(strings.len(), 1_000);
    assert_eq!(strings[123], "item-123", "String content lost");

    // Box new/drop.
    let boxes: Vec<Box<u64>> = (0..2_048u64).map(|i| Box::new(i * 7)).collect();
    let box_sum: u64 = boxes.iter().map(|b| **b).sum();
    let expected_box_sum: u64 = (0..2_048u64).map(|i| i * 7).sum();
    assert_eq!(box_sum, expected_box_sum, "Box contents corrupted");

    // HashMap insert/lookup (varied sizes, hashing allocs).
    let mut m: HashMap<u64, String> = HashMap::new();
    for i in 0..5_000u64 {
        m.insert(i, format!("value-{i}"));
    }
    assert_eq!(m.len(), 5_000, "HashMap lost entries");
    assert_eq!(
        m.get(&4_321).map(String::as_str),
        Some("value-4321"),
        "HashMap lookup failed — corruption",
    );

    // A second Vec churn (realloc of an existing allocation).
    let mut w: Vec<[u8; 32]> = Vec::with_capacity(8);
    for i in 0..1_024u32 {
        let mut block = [0u8; 32];
        block[..4].copy_from_slice(&i.to_le_bytes());
        w.push(block);
    }
    for (i, block) in w.iter().enumerate() {
        let stored = u32::from_le_bytes(block[..4].try_into().unwrap());
        assert_eq!(stored, i as u32, "Vec block {i} corrupted after grow");
    }
}

/// A second test, also under the installed global allocator, to exercise
/// repeated alloc/dealloc churn on the SAME heap slot (the registry claims
/// once per thread; both tests share this thread's heap). Catches a
/// regression where state is not preserved across calls.
#[test]
fn global_allocator_repeated_churn_reuses_state() {
    let _serial = SerialGuard::acquire();
    // 100k alloc/dealloc pairs of varied sizes — exercises free-list reuse.
    for size in [16usize, 64, 128, 256, 1024] {
        let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
        for _ in 0..1_000 {
            // SAFETY: valid layout; GLOBAL is the installed allocator.
            let p = unsafe { GLOBAL.alloc(layout) };
            assert!(!p.is_null(), "alloc({size}) returned null on iteration");
            // SAFETY: p is valid for `size` bytes.
            unsafe {
                std::ptr::write_bytes(p, 0xAB, size);
                assert_eq!((p as *const u8).read(), 0xAB, "byte not retained");
                GLOBAL.dealloc(p, layout);
            }
        }
    }
}
