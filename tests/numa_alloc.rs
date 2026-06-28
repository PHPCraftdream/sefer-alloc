//! Phase D — functional correctness tests for NUMA-aware segment allocation.
//!
//! ## Overview
//!
//! These tests verify that the NUMA integration (Phases A–C) correctly stamps
//! `node_id` in segment headers, that same-thread allocations share the same
//! node, and that cross-thread free between segments of potentially different
//! NUMA nodes is safe and leak-free.
//!
//! ## When to run
//!
//! The tests are **guarded by an environment variable**: every test body begins
//! by checking `SEFER_NUMA_TEST=1`.  Without this env var the test prints a
//! note and returns immediately (passes trivially on single-NUMA / CI machines
//! without NUMA topology).
//!
//! ## How to run with a real multi-NUMA topology
//!
//! ### Option A — QEMU fake-NUMA (Linux VM)
//!
//! ```text
//! qemu-system-x86_64 \
//!   -m 2G \
//!   -smp 4,sockets=2,cores=2,threads=1 \
//!   -numa node,nodeid=0,cpus=0-1,mem=1G \
//!   -numa node,nodeid=1,cpus=2-3,mem=1G \
//!   -numa dist,src=0,dst=1,val=20 \
//!   ...
//! ```
//! Inside the VM verify with `numactl --hardware` (should show 2 nodes).
//! Then:
//! ```text
//! SEFER_NUMA_TEST=1 \
//!   cargo test \
//!     --features "alloc-core alloc-global alloc-xthread alloc-decommit numa-aware" \
//!     --test numa_alloc
//! ```
//!
//! ### Option B — kernel boot param `numa=fake=N`
//!
//! Add `numa=fake=4` to the kernel command line (e.g. via GRUB).  This
//! creates N virtual NUMA nodes on a single physical socket without a VM.
//! Run the same cargo invocation.
//!
//! ### What to verify on a real multi-NUMA machine
//!
//! - `numactl --hardware` shows ≥ 2 nodes before the run.
//! - `same_thread_segments_same_node` prints non-NO_NODE values and both match.
//! - `cross_node_handoff_safe` exits without panic.
//! - `find_segment_prefers_local_node` may show different node IDs per thread
//!   if CPU affinity is set (e.g. with `numactl --cpunodebind`); on a
//!   single-socket machine both threads land on node 0 — still passes.

// Gate: requires all four features used in the integration path.
#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "numa-aware"
))]

use core::alloc::Layout;
use std::sync::mpsc;
use std::thread;

use sefer_alloc::alloc_core::{numa, AllocCore};
use sefer_alloc::Heap;

// ---------------------------------------------------------------------------
// Guard helper
// ---------------------------------------------------------------------------

/// Returns `true` if `SEFER_NUMA_TEST=1` is set in the environment.
///
/// Call at the top of every test; if `false`, print a skip note and return
/// early so CI on non-NUMA machines sees a pass, not a failure.
fn numa_test_enabled() -> bool {
    std::env::var("SEFER_NUMA_TEST").as_deref() == Ok("1")
}

// ---------------------------------------------------------------------------
// Test 1: same_thread_segments_same_node
// ---------------------------------------------------------------------------

/// Two consecutive small allocations on the **same thread** must yield
/// segments that both carry the **same `node_id`** — either the real NUMA
/// node of this thread (on a multi-NUMA machine / QEMU) or both `NO_NODE`
/// (on single-NUMA platforms where `current_node()` returns `NO_NODE`).
///
/// Validates that `reserve_small_segment` consistently stamps `node_id` from
/// `numa::current_node()` at reservation time, and that
/// `find_segment_with_free` respects node preference so a second allocation
/// also lands in a same-node segment.
#[test]
fn same_thread_segments_same_node() {
    if !numa_test_enabled() {
        eprintln!(
            "SEFER_NUMA_TEST != 1 — пропускаю \
             (нужна multi-NUMA топология: QEMU -numa или numa=fake)"
        );
        return;
    }

    let mut core = AllocCore::new().expect("AllocCore::new failed");

    let layout = Layout::from_size_align(64, 8).unwrap();

    // First allocation — forces reservation of a new small segment.
    let ptr_a = core.alloc(layout);
    assert!(!ptr_a.is_null(), "first alloc returned null");

    // Second allocation — reuses the same segment (small enough to bump-alloc)
    // or at minimum a segment on the same node.
    let ptr_b = core.alloc(layout);
    assert!(!ptr_b.is_null(), "second alloc returned null");

    let node_a = core
        .dbg_node_id_for(ptr_a)
        .expect("dbg_node_id_for: ptr_a not in a known segment");
    let node_b = core
        .dbg_node_id_for(ptr_b)
        .expect("dbg_node_id_for: ptr_b not in a known segment");

    eprintln!(
        "same_thread_segments_same_node: node_a={node_a}, node_b={node_b}, NO_NODE={}",
        numa::NO_NODE
    );

    // Both segments must belong to the same node.
    // On single-NUMA both will be NO_NODE; on a NUMA machine both will be
    // the calling thread's current node.
    assert_eq!(
        node_a, node_b,
        "same-thread allocations landed in segments of different NUMA nodes \
         ({node_a} vs {node_b}) — NUMA preference in find_segment_with_free broken"
    );

    // Non-vacuous: write and read back each block.
    unsafe {
        std::ptr::write_bytes(ptr_a, 0xAA, layout.size());
        std::ptr::write_bytes(ptr_b, 0xBB, layout.size());
        assert_eq!((ptr_a as *const u8).read(), 0xAA, "ptr_a readback failed");
        assert_eq!((ptr_b as *const u8).read(), 0xBB, "ptr_b readback failed");
    }

    core.dealloc(ptr_a, layout);
    core.dealloc(ptr_b, layout);
}

// ---------------------------------------------------------------------------
// Test 2: cross_node_handoff_safe
// ---------------------------------------------------------------------------

/// Allocate N blocks on **thread A** (main thread using `Heap`) then hand
/// the raw addresses to **thread B** which frees them via
/// `Heap::dealloc_any_thread`.
///
/// This is the smoke test for cross-thread free on segments that may (on a
/// multi-NUMA machine) belong to different NUMA nodes than thread B's node.
/// The allocator must not panic, corrupt memory, or leak segments regardless
/// of the node mismatch.
///
/// ## Why we use `Heap` (not `AllocCore`) here
///
/// `Heap::dealloc_any_thread` works by pushing the block's
/// (offset, class) pair into the segment's `RemoteFreeRing`.  The ring is
/// only consulted when the segment's `owner_thread_free` field is non-null —
/// that field is stamped by `Heap` on first dealloc of a segment's block.
/// `AllocCore` alone does not stamp this field, so a cross-thread free via
/// `dealloc_any_thread` would silently no-op without exercising the ring.
/// Using `Heap` ensures the stamp is set and the cross-thread path fires.
///
/// ## Safety contract for the pointer handoff
///
/// - The `Heap` stays alive in the main thread until thread B finishes
///   freeing, so the backing segments remain mapped.
/// - Pointers are sent as `usize` (raw `*mut u8` is `!Send`).
/// - Pattern is written before send and verified by the consumer before
///   freeing — corruption would be caught by the assertion.
#[test]
fn cross_node_handoff_safe() {
    if !numa_test_enabled() {
        eprintln!(
            "SEFER_NUMA_TEST != 1 — пропускаю \
             (нужна multi-NUMA топология: QEMU -numa или numa=fake)"
        );
        return;
    }

    const N: usize = 64;
    let layout = Layout::from_size_align(128, 8).unwrap();

    // Thread A (main thread): allocate N blocks and write a unique pattern.
    let mut heap = Heap::new().expect("Heap::new failed");
    let mut ptrs: Vec<(usize, u8)> = Vec::with_capacity(N);

    for i in 0..N {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null(), "heap alloc #{i} returned null");

        let pat = ((i * 7 + 0xA5) & 0xFF) as u8;
        // SAFETY: ptr is valid for layout.size() bytes — just allocated.
        unsafe {
            std::ptr::write_bytes(ptr, pat, layout.size());
            // Immediate readback — catches any immediately-visible corruption.
            assert_eq!(
                (ptr as *const u8).read(),
                pat,
                "main thread: pattern readback failed at block {i}"
            );
        }
        ptrs.push((ptr as usize, pat));
    }

    // Send the list of (addr, pattern) to thread B via mpsc.
    let (tx, rx) = mpsc::channel::<Vec<(usize, u8)>>();
    tx.send(ptrs).expect("channel send failed");
    drop(tx);

    // Thread B: receive, verify pattern still intact, then free cross-thread.
    let consumer = thread::spawn(move || {
        let blocks = rx.recv().expect("channel recv failed");
        for (addr, pat) in blocks {
            let ptr = addr as *mut u8;
            // Verify pattern before freeing — catches cross-node UAF.
            // SAFETY: main thread wrote this pattern and keeps the Heap alive.
            unsafe {
                assert_eq!(
                    (ptr as *const u8).read(),
                    pat,
                    "consumer: pattern corrupted before cross-thread free"
                );
            }
            // Free via the cross-thread path — this exercises the
            // RemoteFreeRing push path across potential NUMA boundaries.
            Heap::dealloc_any_thread(ptr, layout);
        }
    });

    // Keep the Heap alive until the consumer finishes so the segments remain mapped.
    consumer.join().expect("consumer thread panicked");

    // After the consumer frees all blocks into the ring, trigger a drain
    // by allocating one more block on the main thread (slow-path drain).
    let drain_ptr = heap.alloc(layout);
    assert!(
        !drain_ptr.is_null(),
        "post-handoff alloc returned null (ring drain may have failed)"
    );
    unsafe {
        std::ptr::write_bytes(drain_ptr, 0xCC, layout.size());
        assert_eq!(
            (drain_ptr as *const u8).read(),
            0xCC,
            "post-handoff alloc readback failed"
        );
    }
    heap.dealloc(drain_ptr, layout);
    // `heap` drops here — segments are abandoned cleanly (M10 no-leak contract
    // is upheld by the shard model).
}

// ---------------------------------------------------------------------------
// Test 3: find_segment_prefers_local_node
// ---------------------------------------------------------------------------

/// Allocate on **two independent `AllocCore` instances in two separate
/// threads** and verify that each thread's allocation is stamped with a
/// `node_id` consistent with what `numa::current_node()` reported on that
/// thread.
///
/// On a real NUMA machine with CPU-per-node affinity (e.g. via `numactl
/// --cpunodebind=0` / `--cpunodebind=1`) the two threads will observe
/// different node IDs and each segment will be stamped accordingly —
/// verifying that `reserve_small_segment` calls `current_node()` at
/// reservation time and passes the result to the OS seam.
///
/// On a single-socket machine (or without affinity) both threads will see
/// node 0 (or `NO_NODE`) — the test still passes: it verifies the consistency
/// invariant `stamped == observed_at_alloc_time`, not a specific node number.
#[test]
fn find_segment_prefers_local_node() {
    if !numa_test_enabled() {
        eprintln!(
            "SEFER_NUMA_TEST != 1 — пропускаю \
             (нужна multi-NUMA топология: QEMU -numa или numa=fake)"
        );
        return;
    }

    const N_THREADS: usize = 2;

    // Channel: each thread sends (observed_current_node, stamped_node_id).
    let (tx, rx) = mpsc::channel::<(u32, u32)>();

    let mut handles = Vec::with_capacity(N_THREADS);
    for _tid in 0..N_THREADS {
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            let mut core = AllocCore::new().expect("AllocCore::new in thread failed");

            // Snapshot the NUMA node BEFORE allocating.
            let observed_node = numa::current_node();

            let layout = Layout::from_size_align(64, 8).unwrap();
            let ptr = core.alloc(layout);
            assert!(!ptr.is_null(), "thread alloc returned null");

            let stamped_node = core
                .dbg_node_id_for(ptr)
                .expect("dbg_node_id_for returned None — ptr not in known segment");

            // Non-vacuous: write and read back.
            unsafe {
                std::ptr::write_bytes(ptr, 0xDE, layout.size());
                assert_eq!((ptr as *const u8).read(), 0xDE, "readback failed in thread");
            }

            tx.send((observed_node, stamped_node))
                .expect("result channel send failed");

            core.dealloc(ptr, layout);
        }));
    }
    drop(tx); // close the sender so the rx loop below terminates

    for h in handles {
        h.join().expect("worker thread panicked");
    }

    // Verify consistency for every thread: stamped_node == observed_node.
    let mut n_results = 0usize;
    for (observed, stamped) in rx {
        eprintln!(
            "find_segment_prefers_local_node: \
             observed_current_node={observed}, stamped_node_id={stamped}"
        );
        assert_eq!(
            stamped, observed,
            "segment node_id ({stamped}) != current_node() ({observed}) — \
             NUMA stamping in reserve_small_segment is broken"
        );
        n_results += 1;
    }

    assert_eq!(
        n_results, N_THREADS,
        "expected results from {N_THREADS} threads, got {n_results}"
    );
}
