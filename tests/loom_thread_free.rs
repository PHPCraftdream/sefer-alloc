//! loom model-check of the **cross-thread free Treiber stack protocol**
//! (Phase 10, M7).
//!
//! # Scope — what loom covers
//!
//! This harness models the Treiber stack push/drain protocol in isolation
//! using `loom::sync::atomic` (NOT the real `ThreadFreeStack`, which uses
//! `core::sync::atomic`). It asserts the core Phase 10 safety property:
//!
//! > A remote freer's push is visible to the owner's drain. No pushed block
//! > is lost. The drain sees a consistent chain. Multiple concurrent pushers
//! > do not corrupt the stack.
//!
//! # The counterfactual
//!
//! The naive non-CAS protocol — "load head, write next, store head" (three
//! separate non-atomic steps) — is UNSOUND under concurrency: two pushers can
//! both load the same `old_head`, both write their `next` to point to it, and
//! both store their block as the new head — the second store overwrites the
//! first, LOSING the first pusher's block. The `compare_exchange` in the
//! correct protocol prevents this: exactly one pusher wins the CAS per head
//! state, and the loser retries with the updated head.
//!
//! The `push_naive_broken` function implements the broken protocol. The test
//! `counterfactual_naive_push_loses_blocks` demonstrates that loom catches this
//! bug: with the naive push, the owner's drain sometimes sees fewer blocks than
//! were pushed (a lost block).
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features alloc --test loom_thread_free
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

/// A minimal Treiber stack modelling the real `ThreadFreeStack`.
///
/// Each "block" is represented by a `Box<Node>` (loom tracks its allocations).
/// In the real allocator, the node is intrusive (stored inside the freed block);
/// here we use explicit nodes for loom's benefit.
struct Node {
    /// The "block address" — a unique identifier for this freed block.
    block_id: usize,
    /// The next node in the stack (null = tail).
    next: *mut Node,
}

/// The correct CAS-based push (mirrors `ThreadFreeStack::push`).
///
/// Ordering justification (same as the real code):
/// - `Release` on CAS success: the drain's `Acquire` swap sees the `next`
///   pointer we wrote.
/// - `Relaxed` on CAS failure: retry loop; no side-effects on failure.
fn push_correct(head: &AtomicPtr<Node>, block_id: usize) {
    let node = Box::into_raw(Box::new(Node {
        block_id,
        next: core::ptr::null_mut(),
    }));
    loop {
        // Relaxed: just read the current head for the CAS expected value.
        // The CAS itself (Release on success) provides the synchronization.
        let old = head.load(Ordering::Relaxed);
        unsafe {
            (*node).next = old;
        }
        // Release on success: the owner's Acquire swap will see our `next` write.
        // Relaxed on failure: no side-effects, just retry.
        match head.compare_exchange_weak(old, node, Ordering::Release, Ordering::Relaxed) {
            Ok(_) => return,
            Err(_) => continue,
        }
    }
}

/// The BROKEN naive push (no CAS — the counterfactual).
///
/// This is the protocol that loom MUST catch as broken: two concurrent pushers
/// can both load the same `old_head`, both write `next = old_head`, and both
/// store their node as the new head. The second store overwrites the first,
/// LOSING the first node (a lost-block bug).
fn push_naive_broken(head: &AtomicPtr<Node>, block_id: usize) {
    let node = Box::into_raw(Box::new(Node {
        block_id,
        next: core::ptr::null_mut(),
    }));
    // Load head (non-atomic relative to the store below — this is the bug).
    let old = head.load(Ordering::Relaxed);
    unsafe {
        (*node).next = old;
    }
    // Store without CAS: if another pusher stored between our load and this
    // store, we overwrite their node (lost block).
    head.store(node, Ordering::Release);
}

/// Drain the stack: swap head to null (Acquire), walk the chain, return the
/// set of block_ids. Mirrors the real `ThreadFreeStack::drain`.
///
/// Ordering: Acquire so we see all `next` pointers written by pushers'
/// Release stores.
fn drain(head: &AtomicPtr<Node>) -> Vec<usize> {
    // Acquire: see all writes from pushers whose Release CAS/store succeeded.
    let mut cur = head.swap(core::ptr::null_mut(), Ordering::Acquire);
    let mut ids = Vec::new();
    while !cur.is_null() {
        let node = unsafe { Box::from_raw(cur) };
        ids.push(node.block_id);
        cur = node.next;
    }
    ids
}

/// Free any remaining nodes on the stack (cleanup for loom leak detection).
fn cleanup(head: &AtomicPtr<Node>) {
    let mut cur = head.load(Ordering::Relaxed);
    while !cur.is_null() {
        let node = unsafe { Box::from_raw(cur) };
        cur = node.next;
    }
    head.store(core::ptr::null_mut(), Ordering::Relaxed);
}

// =========================================================================
// Tests
// =========================================================================

/// loom model-check: 1 owner (drainer) + 2 remote pushers using the CORRECT
/// CAS protocol. Asserts that ALL pushed blocks appear in the drain (no lost
/// blocks) and no block appears twice (no duplication).
///
/// Bounded exploration: `preemption_bound = 3`.
#[test]
fn correct_push_never_loses_blocks() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let head = Arc::new(AtomicPtr::new(core::ptr::null_mut()));
        let total_pushed = Arc::new(AtomicUsize::new(0));

        // Remote pusher 1: pushes block 1.
        let h1 = Arc::clone(&head);
        let tp1 = Arc::clone(&total_pushed);
        let t1 = thread::spawn(move || {
            push_correct(&h1, 1);
            tp1.fetch_add(1, Ordering::Relaxed);
        });

        // Remote pusher 2: pushes block 2.
        let h2 = Arc::clone(&head);
        let tp2 = Arc::clone(&total_pushed);
        let t2 = thread::spawn(move || {
            push_correct(&h2, 2);
            tp2.fetch_add(1, Ordering::Relaxed);
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Owner drains.
        let ids = drain(&head);

        // All pushed blocks must appear exactly once.
        let pushed = total_pushed.load(Ordering::Relaxed);
        assert_eq!(
            ids.len(),
            pushed,
            "lost or duplicated blocks: expected {pushed}, got {}",
            ids.len()
        );
        // Both block 1 and block 2 must be present.
        assert!(ids.contains(&1), "block 1 lost");
        assert!(ids.contains(&2), "block 2 lost");

        cleanup(&head);
    });
}

/// loom model-check: owner pushes locally + remote pusher, drain sees both.
/// This tests the owner-as-pusher + remote-pusher + drain interleaving.
#[test]
fn owner_and_remote_push_both_visible() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let head = Arc::new(AtomicPtr::new(core::ptr::null_mut()));

        // Remote pusher: pushes block 10.
        let h1 = Arc::clone(&head);
        let remote = thread::spawn(move || {
            push_correct(&h1, 10);
        });

        // Owner pushes block 20 (owner can push locally too — e.g. during
        // cross-thread dealloc of its own blocks by another thread).
        push_correct(&head, 20);

        remote.join().unwrap();

        // Drain.
        let ids = drain(&head);
        assert_eq!(ids.len(), 2, "expected 2 blocks, got {}", ids.len());
        assert!(ids.contains(&10), "remote block 10 lost");
        assert!(ids.contains(&20), "owner block 20 lost");

        cleanup(&head);
    });
}

/// COUNTERFACTUAL: the naive non-CAS push LOSES blocks under concurrency.
///
/// This test demonstrates that loom catches the bug in the broken protocol.
/// With two concurrent naive pushers, the drain sometimes sees only 1 block
/// instead of 2 (the second store overwrites the first pusher's node).
///
/// **How the non-vacuousness is verified:** this test is `#[should_panic]`
/// because loom explores all interleavings and FINDS the one where the naive
/// push loses a block. If this test PASSES (does not panic), the
/// counterfactual is vacuous and the loom harness is broken.
#[test]
#[should_panic(expected = "lost")]
fn counterfactual_naive_push_loses_blocks() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let head = Arc::new(AtomicPtr::new(core::ptr::null_mut()));

        // Remote pusher 1: naive broken push of block 1.
        let h1 = Arc::clone(&head);
        let t1 = thread::spawn(move || {
            push_naive_broken(&h1, 1);
        });

        // Remote pusher 2: naive broken push of block 2.
        let h2 = Arc::clone(&head);
        let t2 = thread::spawn(move || {
            push_naive_broken(&h2, 2);
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Owner drains.
        let ids = drain(&head);

        // With the naive push, some interleavings lose a block (ids.len() == 1
        // instead of 2). Assert that we got both — loom will find the
        // interleaving where this assertion fails.
        assert!(
            ids.len() == 2,
            "lost block: expected 2, got {} (naive push is broken)",
            ids.len()
        );

        cleanup(&head);
    });
}

/// Stale drain is a no-op: draining an empty stack returns nothing.
#[test]
fn drain_empty_is_noop() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let head = Arc::new(AtomicPtr::new(core::ptr::null_mut()));
        let ids = drain(&head);
        assert!(ids.is_empty(), "drain of empty stack should be empty");
        cleanup(&head);
    });
}
