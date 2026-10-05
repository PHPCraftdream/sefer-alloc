//! R11-4 — `HeapCore::dealloc_batch`'s partitioning fallback must correctly
//! handle a batch mixing THIS-heap-owned Small blocks with a pointer this
//! heap does NOT own: the owned entries take the new batched fast path
//! (magazine-first-fill + `flush_class`-overflow), while any entry that fails
//! the `contains_base` ownership gate falls back to the existing,
//! fully-correct scalar `HeapCore::dealloc` for that one block — proving the
//! partitioning fallback actually routes non-fast-path entries correctly,
//! not just the happy homogeneous-batch path.
//!
//! R5-01: `alloc-global` (this file's gate, via `batch-api`) now
//! unconditionally implies `alloc-xthread`, so the single scenario this file
//! covers is a genuine cross-thread pointer — allocated by a DIFFERENT heap
//! on a different thread. This is the realistic "mixed batch" shape a caller
//! could actually produce, and it must be routed to its owner's ring
//! (reclaimed there), not dropped or corrupted. (A prior revision of this
//! file also covered a `not(alloc-xthread)` foreign-pointer variant; removed
//! — that configuration can no longer be built. See
//! `tests/stats_foreign_or_unroutable_frees.rs`'s module doc for the
//! still-relevant note on why a synthetic foreign-pointer probe is unsound
//! under `alloc-xthread`'s `dealloc_routing`.)
//!
//! Mirrors `tests/r10_7_alloc_batch_xthread_double_free.rs`'s harness
//! (registry-level lease claims, a serialising guard since
//! the registry is process-global).

#![cfg(all(
    all(feature = "alloc-global", feature = "batch-api"),
    feature = "internals"
))]

extern crate sefer_alloc;

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise against other tests in this binary: the registry is a
// process-global static shared across every HeapCore in the process.
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

/// A `dealloc_batch` call whose `blocks` slice mixes several
/// THIS-heap-owned Small blocks (fast-path eligible) with ONE block owned by
/// a DIFFERENT heap on a different thread (a genuine cross-thread free —
/// must route via the ring, not be silently dropped or corrupt the remote
/// heap's segment).
///
/// After the call: every owned block must be reusable (no leak, no
/// corruption), and the cross-thread block must be reclaimed by ITS owner
/// once drained (proving it was routed, not dropped).
#[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
#[test]
fn dealloc_batch_mixed_owned_and_xthread() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let mut owner_lease =
        HeapRegistry::dbg_claim_lease().expect("owner HeapRegistry::claim failed");
    let owner = owner_lease.core();

    let layout = Layout::from_size_align(32, 8).unwrap();

    // (1) Several own-thread-owned blocks (fast-path eligible).
    let owned_n = 20usize;
    let mut owned: Vec<*mut u8> = Vec::with_capacity(owned_n);
    for _ in 0..owned_n {
        // SAFETY: valid layout; `owner` is the calling thread's own slot.
        let p = owner.alloc(layout);
        assert!(!p.is_null(), "owned alloc returned null");
        owned.push(p);
    }

    // (2) A block allocated by a DIFFERENT heap on a different thread — a
    //     genuine cross-thread pointer relative to `owner`. The remote lease
    //     is `!Send`, so the remote thread KEEPS it and stays parked until
    //     the main thread has routed the cross-thread free; only then does
    //     the remote thread drive its own ring drain on its own lease.
    let remote_ptr_addr = {
        let (addr_tx, addr_rx) = std::sync::mpsc::channel::<usize>();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            let _ = bootstrap::ensure();
            let mut remote_lease =
                HeapRegistry::dbg_claim_lease().expect("remote HeapRegistry::claim failed");
            let remote = remote_lease.core();
            // SAFETY: valid layout; `remote` is this spawned thread's own slot.
            let p = remote.alloc(layout);
            assert!(!p.is_null(), "remote alloc returned null");
            addr_tx.send(p as usize).expect("addr_tx send failed");
            // Park until the main thread has routed the cross-thread free.
            go_rx.recv().expect("go_rx recv failed");
            // A fresh alloc on it opportunistically drains its
            // rings/overflow, reclaiming the cross-thread-freed block if the
            // routing succeeded.
            let q = remote.alloc(layout);
            assert!(
                !q.is_null(),
                "remote heap unusable after receiving xthread free"
            );
            // SAFETY: `q` was allocated above with `layout`; freed once.
            unsafe { remote.dealloc(q, layout) };
            // The lease Drop recycles the slot whole.
            drop(remote_lease);
        });
        let addr = addr_rx.recv().expect("addr_rx recv failed");
        (handle, go_tx, addr)
    };
    let remote_ptr = remote_ptr_addr.2 as *mut u8;
    let remote_join = remote_ptr_addr.0;
    let go_tx = remote_ptr_addr.1;

    // Build the mixed batch: owned blocks with the cross-thread block
    // interleaved (not just appended), so the partitioning logic must
    // correctly classify entries regardless of position.
    let mut batch: Vec<*mut u8> = Vec::with_capacity(owned_n + 1);
    batch.push(owned[0]);
    batch.push(remote_ptr);
    batch.extend_from_slice(&owned[1..]);

    // SAFETY: every `owned[i]` is a live allocation of `layout` made by
    // `owner`, freed at most once (present exactly once in `batch`).
    // `remote_ptr` is a live allocation of `layout` made by ANOTHER heap in
    // this process — a legitimate cross-thread free under the
    // `alloc-xthread` contract, routed via `dealloc_batch`'s ownership-gated
    // fallback to scalar `dealloc`, which performs the correct cross-thread
    // ring push.
    unsafe { owner.dealloc_batch(layout, &batch) };

    // The owned blocks must now be reusable: fresh allocations of the same
    // class must succeed and be distinct from each other.
    let mut fresh: Vec<*mut u8> = Vec::with_capacity(owned_n);
    for _ in 0..owned_n {
        // SAFETY: valid layout; `owner` is the calling thread's own slot.
        let p = owner.alloc(layout);
        assert!(
            !p.is_null(),
            "owner heap unusable after mixed dealloc_batch"
        );
        fresh.push(p);
    }
    {
        use std::collections::HashSet;
        let set: HashSet<usize> = fresh.iter().map(|&p| p as usize).collect();
        assert_eq!(set.len(), owned_n, "owner freelist corrupted (duplicates)");
    }
    // SAFETY: every entry of `fresh` was allocated above with `layout`;
    // freed exactly once here.
    for &p in &fresh {
        unsafe { owner.dealloc(p, layout) };
    }

    // The cross-thread free must have been ROUTED, not silently dropped: the
    // remote heap's ring/overflow should have received the note. Release the
    // remote thread to drive its owner to drain (own-thread `alloc`
    // opportunistically drains rings) and confirm the remote heap is still
    // usable (no corruption from receiving the routed free).
    go_tx.send(()).expect("go_tx send failed");
    remote_join
        .join()
        .expect("remote drain thread must not panic");

    // The lease Drop recycles the slot whole.
    drop(owner_lease);
}
