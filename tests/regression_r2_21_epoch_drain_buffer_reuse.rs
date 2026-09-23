#![allow(deprecated)]
//! R2-21 (independent src review round 2) — remote-free queue BUFFER REUSE
//! across `EpochRegion::drain_remote_free` cycles.
//!
//! ## The mechanism
//!
//! Before this fix the drain ended its locked section with
//! `core::mem::take(&mut *q)`: every NON-EMPTY drain handed the remote-free
//! queue's buffer to a local `Vec` (destroyed when the call returned) and left
//! the queue with a fresh ZERO-CAPACITY `Vec`. The next remote-free burst
//! therefore re-grew the queue from scratch — and those pushes happen under
//! the remote-queue mutex (the one lock every remote evictor contends on), so
//! the re-growth was paid INSIDE that critical section.
//!
//! The fix swaps the queue with an owner-side scratch buffer
//! (`FreeState::drain_scratch`, reachable because the drain already runs with
//! `&mut FreeState`): an O(1) pointer exchange INSIDE the lock that leaves a
//! buffer on EACH side — the queue keeps the scratch's former buffer, the
//! scratch keeps the queue's — with the index-by-index transfer into the free
//! list and the scratch's `drain(..)` clearing moved OUTSIDE the lock. Both
//! capacities grow lazily to the largest burst seen and never shrink; after
//! the first two drain cycles the steady state allocates nothing.
//!
//! ## RED / GREEN
//!
//! `remote_free_drain_reuses_queue_buffer_across_cycles` is the
//! counterfactual test: it FAILED pre-fix, because with `core::mem::take`
//! every post-drain queue probe read capacity 0 — the first load-bearing
//! assertion (at cycle 1) is `drained[1].capacity >= BURST - 1`, i.e.
//! `0 >= 15`.
//!
//! `full_drain_via_swap_keeps_len_and_reuse_accounting_correct` is the
//! accounting guard for the new path (no index lost or stranded when the queue
//! holds EVERY slot's index at drain time). It exercises the same code both
//! before and after the fix — an accounting regression is independent of the
//! buffer-reuse bug — so it must stay green on BOTH sides; it carries no
//! capacity assertion on purpose (that would make it a second counterfactual
//! and destroy its role as a regression guard).
//!
//! ## How the scenarios reach the remote path
//!
//! The only public route to `EpochRegion::remote_evict` is
//! `ShardedRegion::remove` performed by a thread that has NOT claimed the
//! handle's shard. So each scenario below has ONE unbound remover thread
//! (`remove` never claims a shard) remote-evicting into a shard whose owner
//! (the test's main thread, which claimed the lowest free shard of the fresh
//! region) drains the queue on its next op. The queue never holds duplicate
//! indices, because re-installing an index requires an owner op, which drains
//! first.

#![cfg(feature = "experimental")]

use std::sync::Arc;
use std::thread::scope;

use sefer_alloc::{ShardedHandle, ShardedRegion};

/// How many drain cycles to run. Cycles 0 and 1 are WARM-UP (the scratch
/// buffer is still finding its steady capacity) and are recorded only; the
/// counterfactual assertions start at cycle 1 (capacity) and cycle 2
/// (cross-cycle stability), so six cycles leaves three steady-state rounds of
/// evidence. Cheap: each cycle is 16 inserts, 16 removes, and one drain.
const CYCLES: usize = 6;

/// Handles inserted per cycle. The queue receives `BURST - 1` indices per
/// cycle: the remover thread takes `BURST - 1` of them and the OWNER takes the
/// last one locally (see the loop below for why one must stay behind).
const BURST: usize = 16;

#[test]
fn remote_free_drain_reuses_queue_buffer_across_cycles() {
    // 64 slots per shard: `BURST` a cycle never approaches the fixed capacity,
    // so every `insert` in the loop must succeed and the scenario stays purely
    // about the queue's BUFFER identity, not about a full-region `Err`.
    let region = Arc::new(ShardedRegion::<u64>::with_shards(2, 64));

    // Queue probes taken with a burst LOADED (before the owner drains).
    let mut loaded_before: Vec<(usize, usize, usize)> = Vec::with_capacity(CYCLES);
    // Queue probes taken right after the owner's non-empty drain.
    let mut drained: Vec<(usize, usize, usize)> = Vec::with_capacity(CYCLES);

    for cycle in 0..CYCLES {
        // (a) The owner (this thread) inserts BURST fresh values. Its first
        // insert in this region claims the lowest free shard — shard 0 — so
        // every handle below lives in shard 0, and `handles[0].shard()` is the
        // shard whose remote-free queue the remover thread will fill.
        let mut handles: Vec<ShardedHandle<u64>> = Vec::with_capacity(BURST);
        for k in 0..BURST {
            let value = u64::try_from(cycle * 1000 + k).expect("id fits u64");
            let handle = region.insert(value).expect("owner insert must succeed");
            handles.push(handle);
        }
        assert_eq!(
            handles[0].shard(),
            0,
            "route fact: a fresh region's owner thread claims shard 0"
        );
        let shard = handles[0].shard();

        // (b) ONE unbound remover thread. It never inserts (so it never claims
        // a shard and stays remote for every handle), taking the
        // `remote_evict` path for each of its BURST - 1 removes — those
        // evicts enqueue BURST - 1 indices into shard 0's remote-free queue
        // WITHOUT taking the owner's writer mutex. `get_cloned` before the
        // removal pins I1 (the value is live until removed) and pins the
        // per-cycle id mapping so a mis-routed handle cannot pass silently.
        let remover_region = region.as_ref();
        scope(|s| {
            let job = s.spawn(|| {
                for (i, handle) in handles.iter().take(BURST - 1).enumerate() {
                    let value = u64::try_from(cycle * 1000 + i).expect("id fits u64");
                    assert_eq!(
                        remover_region.get_cloned(*handle),
                        Some(value),
                        "I1: a freshly inserted handle must resolve to its value"
                    );
                    assert!(
                        remover_region.remove(*handle),
                        "remote evict of a live handle must return true"
                    );
                    assert!(
                        remover_region.get_cloned(*handle).is_none(),
                        "I2: a removed handle must resolve to None forever"
                    );
                }
            });
            job.join().expect("remover thread must not panic");
        });

        // (c) Probe the queue with the burst loaded: `BURST - 1` indices
        // pending, capacity already paid for (>= the burst length).
        let (ptr, len, cap) = region
            ._remote_free_queue_buffer_identity_for_tests(shard)
            .expect("shard in range");
        assert_eq!(len, BURST - 1, "every remote evict enqueues exactly once");
        assert!(
            cap >= BURST - 1,
            "a burst of {BURST} - 1 pushes must have a real buffer (got cap {cap})"
        );
        loaded_before.push((ptr, len, cap));

        // (d) The OWNER removes the last handle. Its `remove` takes the writer
        // mutex, drains the queue (flag check + swap outside/inside the queue
        // lock), and then pushes its own index. One handle must stay behind
        // for exactly this: the owner op is what drains the burst the remover
        // left pending. Afterwards nothing may accumulate — the region must be
        // back to empty (I4), so the next cycle starts from the same state.
        assert!(
            region.remove(handles[BURST - 1]),
            "owner remove of a live handle must return true"
        );
        assert_eq!(region.len(), 0, "nothing accumulates across drain cycles");

        // (e) Probe the drained queue: every index reached the free list
        // (nothing stranded by the swap).
        let (ptr, len, cap) = region
            ._remote_free_queue_buffer_identity_for_tests(shard)
            .expect("shard in range");
        assert_eq!(len, 0, "a non-empty drain must leave the queue empty");
        drained.push((ptr, len, cap));
    }

    // Warm-up: cycles 0 and 1 are recorded only. In cycle 0 the scratch starts
    // empty (zero capacity) and the queue grows from scratch, so both probes
    // are transient shapes; cycle 1 is the cycle that leaves a real buffer on
    // the queue side of the swap.

    // LOAD-BEARING COUNTERFACTUAL ASSERTIONS. Pre-fix (`core::mem::take`), the
    // post-drain queue was ALWAYS a fresh zero-capacity Vec, so every
    // `drained[i].capacity` read 0 and the first of these failed at cycle 1.
    for (i, probe) in drained.iter().enumerate().skip(1) {
        assert!(
            probe.2 >= BURST - 1,
            "cycle {i}: a non-empty drain must leave a real buffer behind, \
             got capacity {} (pre-fix `core::mem::take` left 0 here)",
            probe.2,
        );
    }
    // Refilling the queue for a new burst must NOT grow it beyond what the
    // previous drain left: with the swap, the queue refills into the buffer
    // the scratch handed back. Pre-fix, a fresh burst reallocated from 0, so
    // the cycle-2 loaded capacity (16) disagreed with the cycle-1 drained
    // capacity (0) here. `loaded_before[2..]` zipped with `drained[1..]` is
    // exactly the `(loaded_before[i], drained[i - 1])` pair sequence for
    // `i in 2..CYCLES`.
    for (i, (loaded, previous)) in loaded_before[2..]
        .iter()
        .zip(&drained[1..])
        .enumerate()
        .map(|(k, pair)| (k + 2, pair))
    {
        assert_eq!(
            loaded.2, previous.2,
            "cycle {i}: refilling the queue reused the previous drain's buffer \
             (loaded cap {} vs previous drained cap {})",
            loaded.2, previous.2,
        );
    }
    // Capacity never grows after warm-up: the acceptance criterion is a
    // STEADY STATE of zero new allocations, which is exactly "same capacity
    // forever once both buffers are warm".
    let steady_cap = drained[1].2;
    for (i, probe) in drained.iter().enumerate().skip(2) {
        assert_eq!(
            probe.2, steady_cap,
            "cycle {i}: drain capacity must not creep after warm-up \
             ({} vs {})",
            probe.2, steady_cap,
        );
    }

    // Rotation structure (secondary). The queue and the scratch must alternate
    // exactly TWO buffers, so the set of post-drain queue pointers observed
    // across steady-state cycles has at most two members; a regression that
    // allocates a fresh buffer per drain (even a right-sized one) grows this
    // set. NOTE: this check passes VACUOUSLY pre-fix, because `core::mem::take`
    // left the queue the same dangling zero-capacity Vec pointer every cycle
    // — the capacity assertions above are what carry the counterfactual.
    let mut rotated: Vec<usize> = drained[1..].iter().map(|d| d.0).collect();
    rotated.sort_unstable();
    rotated.dedup();
    assert!(
        rotated.len() <= 2,
        "the queue and scratch must alternate at most two buffers, saw {}: \
         {rotated:?}",
        rotated.len(),
    );
}

#[test]
fn full_drain_via_swap_keeps_len_and_reuse_accounting_correct() {
    // Per-shard capacity EXACTLY BURST: the owner fills its shard completely,
    // so the drain below is the maximal one — the queue holds EVERY slot's
    // index at drain time.
    const BURST: usize = 16;
    let region = Arc::new(ShardedRegion::<u64>::with_shards(2, BURST));

    let mut handles: Vec<ShardedHandle<u64>> = Vec::with_capacity(BURST);
    for k in 0..BURST {
        let value = u64::try_from(k).expect("id fits u64");
        let handle = region.insert(value).expect("insert into a fresh shard");
        handles.push(handle);
    }
    assert_eq!(region.len(), BURST, "the shard holds BURST live values");
    let shard = handles[0].shard();
    // The free list is now empty, so the owner cannot install anything more.
    assert!(
        region.insert(u64::MAX).is_err(),
        "a full region must hand the value back honestly"
    );

    // One unbound remover thread remote-evicts ALL 16 handles: all 16 indices
    // are PENDING in the queue, the free list is still empty, and `len` is
    // decremented by each evict's fetch_sub.
    let remover_region = region.as_ref();
    scope(|s| {
        let job = s.spawn(|| {
            for (i, handle) in handles.iter().enumerate() {
                let value = u64::try_from(i).expect("id fits u64");
                assert_eq!(
                    remover_region.get_cloned(*handle),
                    Some(value),
                    "I1: a freshly inserted handle must resolve to its value"
                );
                assert!(
                    remover_region.remove(*handle),
                    "remote evict of a live handle must return true"
                );
                assert!(
                    remover_region.get_cloned(*handle).is_none(),
                    "I2: a removed handle must resolve to None forever"
                );
            }
        });
        job.join().expect("remover thread must not panic");
    });
    assert_eq!(region.len(), 0, "all 16 values were live, all are gone now");
    let (_, pending, _) = region
        ._remote_free_queue_buffer_identity_for_tests(shard)
        .expect("shard in range");
    assert_eq!(
        pending, BURST,
        "all 16 indices sit pending in the remote-free queue, free list still empty"
    );

    // The OWNER re-installs BURST new values. The very first insert drains the
    // 16 pending indices into the free list, so EVERY install must succeed —
    // this is what fails if the swap loses or strands a single index (a lost
    // index is a slot that can never be handed out again, which would surface
    // here as `Err`).
    const OFFSET: u64 = 1000;
    let mut new_handles: Vec<ShardedHandle<u64>> = Vec::with_capacity(BURST);
    for k in 0..BURST {
        let value = OFFSET + u64::try_from(k).expect("id fits u64");
        let handle = region
            .insert(value)
            .expect("the drain must have returned every pending index to the free list");
        new_handles.push(handle);
    }
    assert_eq!(region.len(), BURST, "the shard is full again (I4)");

    for (k, handle) in new_handles.iter().enumerate() {
        let value = OFFSET + u64::try_from(k).expect("id fits u64");
        assert_eq!(
            region.get_cloned(*handle),
            Some(value),
            "I1: a re-installed value must resolve to the NEW value"
        );
        assert!(
            region.get_cloned(handles[k]).is_none(),
            "I2/I3: the generation bump from the remote evict must keep the \
             old handle stale"
        );
    }
    // The queue was fully drained by the first re-install; owner inserts push
    // nothing (only `remote_evict` does), so it must still be empty.
    let (_, still_pending, _) = region
        ._remote_free_queue_buffer_identity_for_tests(shard)
        .expect("shard in range");
    assert_eq!(
        still_pending, 0,
        "the queue must be empty after a full drain"
    );

    // Structural cleanup: the region is dropped exactly once here, taking the
    // still-live values with it (I5). No drop counter is needed for this
    // finding — the accounting assertions above are what pin it — so the drop
    // is asserted implicitly by the scope ending without a leak report.
    drop(region);
}
