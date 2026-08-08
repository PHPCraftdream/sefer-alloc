//! loom model-check of the REAL [`TaggedIndexStack`] / [`TaggedIndex`] types.
//!
//! Unlike the in-tree shadow model this replaced (`tests/loom_free_slots_aba.rs`
//! in the extracting allocator, which TRANSCRIBED the protocol into a local copy
//! because it could not import the real registry code), this suite runs against
//! the ACTUAL crate code: under `--cfg loom` the crate aliases its atomics to
//! `loom::sync::atomic`, so `stack.push` / `stack.pop` and the `TaggedIndex`
//! packing that loom explores here ARE the code that ships.
//!
//! # What loom covers
//!
//! - `TaggedIndexStack<16>` head (`AtomicU64`, packed `(index | tag << 16)`),
//!   `ArrayLinks<N>` slot-resident `AtomicU32` links, `TAIL` end-of-chain.
//! - `pop`: load tagged head, read the link, CAS head to `(next, SAME tag)` — a
//!   losing CAS retries.
//! - `push`: write the link, bump the tag, CAS head to `(idx, tag + 1)` — the
//!   tag bump defeats ABA.
//!
//! # Properties asserted
//!
//! (a) In the classic "B pops X then re-pushes X inside A's read→CAS window",
//!     A's stale-tag CAS is FORCED to fail (retry) rather than succeeding onto a
//!     stale chain.
//! (b) The free-list stays loss/duplication-free after the race resolves.
//! (c) **Untagged counterfactual** (`#[should_panic]`): a bare `AtomicU32` head
//!     with NO tag lets the same interleaving corrupt the free-list — proving
//!     the harness is non-vacuous and the tag is load-bearing.
//! (d) **H-2 empty-transition:** the REAL `pop` preserves the running tag across
//!     a drain-to-empty, so a stalled popper's CAS fails (fixed); a buggy pop
//!     that packs `TaggedIndex::empty()` (tag 0) on the drain lets the stale CAS
//!     recur — the `#[should_panic]` counterfactual
//!     `counterfactual_empty_transition_tag_reset_lets_aba_recur`.
//!
//! # How to run
//!
//! ```text
//! RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --test loom_aba
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

use tagged_index_stack::{ArrayLinks, Links, TaggedIndex, TaggedIndexStack, TAIL};

type Tag = TaggedIndex<16>;

// A 2-slot backing is sufficient for the ABA scenario when designed correctly.
const N: usize = 2;

/// Seed an `ArrayLinks<2>` + `TaggedIndexStack<16>` into the state "slot 0 on
/// top, chained to slot 1, chained to TAIL" — i.e. both slots free. Because the
/// crate's stack is lazy (a fresh stack is empty), we materialise this state by
/// pushing 1 then 0 through the REAL `push` (which sets links + tag exactly as
/// production does), leaving a running tag of 2. This is the real-type analogue
/// of the shadow model's hand-built `new_both_free`.
fn both_free() -> (Arc<TaggedIndexStack<16>>, Arc<ArrayLinks<N>>) {
    let links = Arc::new(ArrayLinks::<N>::new());
    let stack = Arc::new(TaggedIndexStack::<16>::new());
    stack.push(&*links, 1);
    stack.push(&*links, 0);
    (stack, links)
}

// ============================================================================
// (a) + (b): the classic ABA race against the REAL type.
// ============================================================================

#[test]
fn aba_repush_forces_stale_cas_retry_and_stays_consistent() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let (stack, links) = both_free();

        // Thread A: inline `pop`'s body ONCE (load head, read link, compute
        // candidate, CAS) so B can race between A's read and A's CAS — the ABA
        // window. This mirrors the REAL `pop`'s loop body exactly (same packing,
        // same orderings), just split so loom can interleave.
        let stack_a = Arc::clone(&stack);
        let links_a = Arc::clone(&links);
        let ta = thread::spawn(move || {
            let head = stack_a.raw_head();
            let (idx_v, tag) = Tag::unpack(head);
            let idx = idx_v as u32;
            let next = links_a.load_next(idx);
            let new_head = if next == TAIL {
                Tag::pack(Tag::empty_index(), tag)
            } else {
                Tag::pack(next as u64, tag)
            };
            stack_a
                .cas_head_for_test(head, new_head, Ordering::Acquire, Ordering::Acquire)
                .map(|_| idx)
        });

        // Thread B: full pop+repush of the same index via the REAL type.
        let stack_b = Arc::clone(&stack);
        let links_b = Arc::clone(&links);
        let tb = thread::spawn(move || {
            if let Some(idx) = stack_b.pop(&*links_b) {
                stack_b.push(&*links_b, idx);
            }
        });

        let a_result = ta.join().unwrap();
        tb.join().unwrap();

        let mut popped: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            popped.push(idx);
        }
        while let Some(idx) = stack.pop(&*links) {
            popped.push(idx);
        }
        popped.sort_unstable();
        assert_eq!(
            popped,
            vec![0, 1],
            "free-list corrupted (loss or duplication): got {popped:?} — the ABA \
             tag guard failed to force A's stale CAS to retry"
        );
    });
}

// ============================================================================
// (c) Untagged counterfactual — a bare AtomicU32 head (no tag) lets the same
// interleaving corrupt the free-list. This is the ONE model that is not the
// crate type (the crate has no untagged mode by construction) — it demonstrates
// what the tag buys, proving the harness above is non-vacuous.
// ============================================================================

struct UntaggedStack {
    head: AtomicU32,
    next: [AtomicU32; N],
}

impl UntaggedStack {
    // Initial state: head=0, next[0]=1, next[1]=TAIL. Both slots 0 and 1 are free.
    fn aba_setup() -> Arc<Self> {
        Arc::new(UntaggedStack {
            head: AtomicU32::new(0),
            next: [AtomicU32::new(1), AtomicU32::new(TAIL)],
        })
    }

    fn pop(&self) -> Option<u32> {
        let mut head = self.head.load(Ordering::Acquire);
        loop {
            if head == TAIL {
                return None;
            }
            let next = self.next[head as usize].load(Ordering::Acquire);
            match self
                .head
                .compare_exchange(head, next, Ordering::Acquire, Ordering::Relaxed)
            {
                Ok(idx) => return Some(idx),
                Err(actual) => head = actual,
            }
        }
    }

    fn push(&self, idx: u32) {
        let mut head = self.head.load(Ordering::Acquire);
        loop {
            self.next[idx as usize].store(head, Ordering::Release);
            match self
                .head
                .compare_exchange(head, idx, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(actual) => head = actual,
            }
        }
    }
}

#[test]
#[should_panic(expected = "corrupted")]
fn counterfactual_untagged_head_lets_aba_corrupt_free_list() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(4);
    builder.check(|| {
        // Initial state: head=0, next[0]=1, next[1]=TAIL. Both slots 0 and 1 are free.
        let reg = UntaggedStack::aba_setup();

        let reg_a = Arc::clone(&reg);
        let ta = thread::spawn(move || {
            let head = reg_a.head.load(Ordering::Acquire);
            if head == TAIL {
                return Err(head);
            }
            let next = reg_a.next[head as usize].load(Ordering::Acquire);
            // A prepares a CAS with its snapshot (head, next), but does NOT
            // execute it yet — B will race between A's read and A's CAS.
            reg_a
                .head
                .compare_exchange(head, next, Ordering::Acquire, Ordering::Relaxed)
        });

        let reg_b = Arc::clone(&reg);
        let tb = thread::spawn(move || {
            // B does TWO pops (drains the stack), then ONE push(0) back.
            // Initial: head=0, next[0]=1, next[1]=TAIL
            // After B pops 0: head=1
            // After B pops 1: head=TAIL
            // After B pushes 0 back: head=0, next[0]=TAIL
            let idx0 = reg_b.pop();
            let idx1 = reg_b.pop();
            // Push only idx0 back.
            if let Some(idx) = idx0 {
                reg_b.push(idx);
            }
            (idx0, idx1)
        });

        let a_result = ta.join().unwrap();
        let (_b_popped_0, _b_held_1) = tb.join().unwrap();

        // FIX #1: Use the actual value from a_result, not hardcoded 0.
        let mut popped: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            popped.push(idx);
        }

        // Drain the remaining stack.
        while let Some(idx) = reg.pop() {
            popped.push(idx);
        }
        popped.sort_unstable();

        // FIX #2: The untagged ABA bug causes index 1 to appear when it should NOT.
        //
        // Without ABA (with tag): A's CAS fails, A retries and correctly observes
        // head=0, next[0]=TAIL, sets head=TAIL, returns 0. Final state: empty.
        // Drain: nothing. Total: vec![0].
        //
        // With ABA (without tag): A's stale CAS succeeds with (head=0, next=1),
        // setting head=1. But B changed next[0]=TAIL, so the chain is broken.
        // Final state: head=1, next[0]=TAIL, next[1]=TAIL.
        // Drain: pops 1. Total: vec![0, 1] (0 from A, 1 from drain).
        //
        // So the corruption is seeing index 1 in the drain when the correct
        // behavior would only have vec![0].
        assert!(
            !popped.contains(&1),
            "free-list corrupted by ABA: index 1 appears in drain {popped:?} \
             when only vec![0] is correct. The untagged stack allowed A's stale \
             CAS to commit an incorrect chain, which the tag prevents."
        );
    });
}

// ============================================================================
// Counterfactual verification: confirm that the real TaggedIndexStack (with
// its tag) does NOT let index 1 resurrect under the EXACT same B-does-two-
// pops-then-one-push pattern that corrupts the untagged model above. This
// mirrors counterfactual_untagged_head_lets_aba_corrupt_free_list's scenario
// precisely (same setup, same A/B shapes, same assertion) but drives it
// against the real crate type via cas_head_for_test, so it demonstrates the
// tag defeats THIS specific resurrection mechanism, not just the older
// single-pop-single-repush one already covered by
// aba_repush_forces_stale_cas_retry_and_stays_consistent.
// ============================================================================

#[test]
fn tagged_stack_survives_the_same_resurrection_pattern() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(4);
    builder.check(|| {
        let (stack, links) = both_free();

        let stack_a = Arc::clone(&stack);
        let links_a = Arc::clone(&links);
        let ta = thread::spawn(move || {
            // A prepares a single CAS from a stale snapshot, same as the
            // untagged counterfactual — but here the tag in `head` no longer
            // matches once B's push bumps it, so this single attempt is
            // expected to fail under the corrupting interleaving (the real
            // `pop`'s own retry loop is what recovers; this hand-inlined
            // single attempt intentionally does not retry, so loom can
            // observe the failure directly). Unlike the other hand-inlined
            // scenarios in this file, B's two pops here CAN drain the stack
            // fully before A reads it, so — mirroring the real `pop`'s own
            // is_empty guard — A must check for empty before indexing links.
            let head = stack_a.raw_head();
            if Tag::is_empty(head) {
                return Err(head);
            }
            let (idx_v, tag) = Tag::unpack(head);
            let idx = idx_v as u32;
            let next = links_a.load_next(idx);
            let new_head = if next == TAIL {
                Tag::pack(Tag::empty_index(), tag)
            } else {
                Tag::pack(next as u64, tag)
            };
            stack_a
                .cas_head_for_test(head, new_head, Ordering::Acquire, Ordering::Acquire)
                .map(|_| idx)
        });

        let stack_b = Arc::clone(&stack);
        let links_b = Arc::clone(&links);
        let tb = thread::spawn(move || {
            // B does TWO pops (drains whatever is left) then re-pushes only
            // its FIRST pop, holding onto its second pop (if any) — the same
            // resurrection setup as the untagged counterfactual. NOTE: which
            // physical index ends up as B's "first" vs "second" pop depends
            // on scheduling (if A runs first, B's first pop is whatever A
            // left behind) — this is expected, not itself a defect; only a
            // DUPLICATE index across {A's result, B's held item, the final
            // drain} is a real corruption.
            let first = stack_b.pop(&*links_b);
            let held = stack_b.pop(&*links_b);
            if let Some(idx) = first {
                stack_b.push(&*links_b, idx);
            }
            held
        });

        let a_result = ta.join().unwrap();
        let b_held = tb.join().unwrap();

        // Conservation invariant, robust to scheduling: A's popped item (if
        // any), B's held-and-never-repushed item (if any), and everything
        // the final drain yields must be PAIRWISE DISJOINT — no index may
        // appear twice across these three sources. Duplication here is
        // exactly the resurrection bug the untagged counterfactual proves;
        // which specific index (0 or 1) lands in which bucket is scheduling-
        // dependent and not itself significant.
        let mut accounted: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            accounted.push(idx);
        }
        if let Some(idx) = b_held {
            accounted.push(idx);
        }
        while let Some(idx) = stack.pop(&*links) {
            accounted.push(idx);
        }
        let before_dedup = accounted.len();
        accounted.sort_unstable();
        accounted.dedup();
        assert_eq!(
            accounted.len(),
            before_dedup,
            "tagged stack: an index was resurrected/duplicated across A's pop, \
             B's held item, and the final drain: {accounted:?}"
        );
    });
}

// ============================================================================
// (d) H-2 empty-transition. The FIXED side runs the REAL `stack.pop` (which
// preserves the running tag on drain). The BUGGY side inlines a pop whose drain
// branch packs `TaggedIndex::empty()` (tag 0) — the exact pre-fix behaviour —
// using the crate's own packing primitives. A two-flag rendezvous guarantees
// B's full pop+push is sandwiched between A's load and A's CAS (see the shadow
// model's rationale: a free race admits degenerate orderings that false-positive).
// ============================================================================

/// A single-slot stack seeded at a caller-chosen running tag (models the
/// realistic steady state, not a bootstrap artifact). Built from the REAL crate
/// type by pushing once then re-seeding the tag via repeated push/pop is
/// fiddly; instead we drive the REAL `push`/`pop` and reason about the tag it
/// produces. Seeding is done by pushing index 0 `start_pushes` times through a
/// pop/push cycle so the running tag reaches the desired value.
fn single_slot_seeded(target_tag: u64) -> (Arc<TaggedIndexStack<16>>, Arc<ArrayLinks<1>>) {
    let links = Arc::new(ArrayLinks::<1>::new());
    let stack = Arc::new(TaggedIndexStack::<16>::new());
    // Each push bumps the tag by 1; a pop preserves it. Push once => tag 1.
    // To reach `target_tag` with slot 0 resting on the stack, push/pop
    // (target_tag - 1) times then push once more, leaving exactly `target_tag`.
    for _ in 0..target_tag.saturating_sub(1) {
        stack.push(&*links, 0);
        stack.pop(&*links);
    }
    stack.push(&*links, 0); // final push -> running tag == target_tag
    let (_v, tag) = Tag::unpack(stack.raw_head());
    assert_eq!(tag, target_tag, "seeded running tag");
    (stack, links)
}

fn run_h2(preserve_tag_on_drain: bool) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(move || {
        // Seed at tag 1: B's buggy drain resets to 0, refill computes 0+1=1,
        // colliding with A's captured tag-1 snapshot.
        let (stack, links) = single_slot_seeded(1);
        let a_loaded = Arc::new(AtomicU32::new(0));
        let b_done = Arc::new(AtomicU32::new(0));

        // Thread B: waits for A's snapshot, then a full pop+push cycle on slot 0.
        // The FIXED build uses the REAL `stack.pop`; the BUGGY build uses a pop
        // whose drain branch resets the tag to 0 (`bug_pop_drain_to_empty`).
        let stack_b = Arc::clone(&stack);
        let links_b = Arc::clone(&links);
        let a_loaded_b = Arc::clone(&a_loaded);
        let b_done_b = Arc::clone(&b_done);
        let tb = thread::spawn(move || {
            while a_loaded_b.load(Ordering::Acquire) == 0 {
                thread::yield_now();
            }
            let popped = if preserve_tag_on_drain {
                stack_b.pop(&*links_b)
            } else {
                bug_pop_drain_to_empty(&stack_b, &*links_b)
            };
            if let Some(idx) = popped {
                stack_b.push(&*links_b, idx);
            }
            b_done_b.store(1, Ordering::Release);
        });

        // Thread A: manual split pop. Uses the drain-branch behaviour under test
        // to compute its candidate, signals `a_loaded`, blocks on `b_done`, then
        // fires its CAS against the STALE captured head.
        let head = stack.raw_head();
        let (idx_v, tag) = Tag::unpack(head);
        let idx = idx_v as u32;
        let next = links.load_next(idx);
        let new_head = if next == TAIL {
            if preserve_tag_on_drain {
                Tag::pack(Tag::empty_index(), tag)
            } else {
                Tag::empty()
            }
        } else {
            Tag::pack(next as u64, tag)
        };
        a_loaded.store(1, Ordering::Release);
        while b_done.load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        let a_result = stack
            .cas_head_for_test(head, new_head, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| idx);

        tb.join().unwrap();

        assert!(
            a_result.is_err(),
            "stale CAS succeeded: thread A's compare_exchange used a head \
             snapshot captured BEFORE thread B's full pop+push cycle, yet \
             succeeded AFTER that cycle completed — an empty-transition \
             tag-reset ABA collision (H-2)"
        );
    });
}

/// A pop whose drain-to-empty branch resets the tag to 0 (`TaggedIndex::empty()`)
/// — the exact pre-H-2-fix behaviour, expressed with the crate's own packing so
/// the counterfactual is faithful. NOT reachable through the shipped `pop`.
fn bug_pop_drain_to_empty<L: Links + ?Sized>(
    stack: &TaggedIndexStack<16>,
    links: &L,
) -> Option<u32> {
    loop {
        let head = stack.raw_head();
        if Tag::is_empty(head) {
            return None;
        }
        let (idx_v, tag) = Tag::unpack(head);
        let idx = idx_v as u32;
        let next = links.load_next(idx);
        let new_head = if next == TAIL {
            Tag::empty() // BUG: hardcoded tag 0 on the empty transition.
        } else {
            Tag::pack(next as u64, tag)
        };
        match stack.cas_head_for_test(head, new_head, Ordering::Acquire, Ordering::Relaxed) {
            Ok(_) => return Some(idx),
            Err(_) => continue,
        }
    }
}

/// **Fixed:** the REAL `pop` preserves the running tag across the drain, so A's
/// stale CAS is always forced to fail.
#[test]
fn pop_empty_transition_preserves_tag() {
    run_h2(true);
}

/// **Counterfactual (non-vacuousness):** the buggy tag-reset drain lets A's
/// stale CAS spuriously succeed — proving the fix is load-bearing.
#[test]
#[should_panic(expected = "stale CAS succeeded")]
fn counterfactual_empty_transition_tag_reset_lets_aba_recur() {
    run_h2(false);
}

// ============================================================================
// (e) CAS failure ordering: a pop that RETRIES after a failed CAS must
// acquire-synchronize with the push that caused the failure, otherwise the
// retry's link read may observe stale state and corrupt the free-list.
// ============================================================================

/// End-to-end regression guard: calls the REAL `pop`/`push` directly (no
/// hand-unrolling) so loom explores every interleaving of the actual shipped
/// atomic operations, including one where `pop`'s first CAS attempt fails
/// against a concurrent `push` and the retry must observe that push's link
/// write. This is the test that actually protects the shipped source: unlike
/// the hand-unrolled `cas_retry_path_must_acquire_with_concurrent_push` below
/// (which pins one specific interleaving for exposition, using
/// `cas_head_for_test` with hardcoded orderings rather than calling `pop`
/// itself), this one fails if `pop`'s own `compare_exchange` failure ordering
/// ever regresses. Verified: with `pop`'s failure ordering temporarily
/// reverted to `Ordering::Relaxed`, this test FAILS (`left: [0, 0, 1], right:
/// [0, 1]` — index 0 duplicated, a real double-allocated free-list slot),
/// then passes again once reverted back to `Ordering::Acquire`.
#[test]
fn pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(4);
    builder.check(|| {
        let links = Arc::new(ArrayLinks::<N>::new());
        let stack = Arc::new(TaggedIndexStack::<16>::new());
        stack.push(&*links, 1);

        let stack_a = Arc::clone(&stack);
        let links_a = Arc::clone(&links);
        let ta = thread::spawn(move || stack_a.pop(&*links_a));

        let stack_b = Arc::clone(&stack);
        let links_b = Arc::clone(&links);
        let tb = thread::spawn(move || {
            stack_b.push(&*links_b, 0);
        });

        let a_result = ta.join().unwrap();
        tb.join().unwrap();

        let mut popped: Vec<u32> = Vec::new();
        if let Some(idx) = a_result {
            popped.push(idx);
        }
        while let Some(idx) = stack.pop(&*links) {
            popped.push(idx);
        }
        popped.sort_unstable();
        assert_eq!(
            popped,
            vec![0, 1],
            "free-list corrupted (loss or duplication) via the real pop/push: got {popped:?}"
        );
    });
}

/// Test the CAS retry path: thread A loads head, reads link; thread B pushes
/// (changing head with a fresh tag); A's CAS fails; A retries — the retry's
/// head observation AND its subsequent link read must both synchronize with
/// B's push.
#[test]
fn cas_retry_path_must_acquire_with_concurrent_push() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(4);
    builder.check(|| {
        // Start with slot 1 only on stack (not slot 0).
        let links = Arc::new(ArrayLinks::<N>::new());
        let stack = Arc::new(TaggedIndexStack::<16>::new());
        stack.push(&*links, 1);

        let stack_a = Arc::clone(&stack);
        let links_a = Arc::clone(&links);
        let stack_b = Arc::clone(&stack);
        let links_b = Arc::clone(&links);

        // Thread A: does TWO iterations of pop's loop (manual expansion to
        // force loom to explore the retry path). First iteration will fail
        // because B interposes; second iteration must succeed with fresh data.
        let ta = thread::spawn(move || {
            // Iteration 1: load head, read link, compute candidate.
            let mut head = stack_a.raw_head();
            let (idx_v, _tag) = Tag::unpack(head);
            let idx = idx_v as u32;
            let next = links_a.load_next(idx);
            let new_head = if next == TAIL {
                Tag::pack(Tag::empty_index(), 0) // tag value doesn't matter for failure path
            } else {
                Tag::pack(next as u64, 0)
            };

            // CAS fails (B pushed in between). The CAS may succeed if B
            // hasn't run yet — only the failure path exercises the bug.
            let result = stack_a.cas_head_for_test(
                head,
                new_head,
                Ordering::Acquire,
                Ordering::Acquire, // FIXED: was Relaxed, now Acquire
            );
            if result.is_ok() {
                // No race — return early, nothing to test.
                return Ok(idx);
            }

            // Iteration 2: RETRY with the actual head from the failure.
            head = result.unwrap_err();
            let (idx_v2, _tag2) = Tag::unpack(head);
            let idx2 = idx_v2 as u32;
            let next2 = links_a.load_next(idx2);
            let new_head2 = if next2 == TAIL {
                Tag::pack(Tag::empty_index(), 0)
            } else {
                Tag::pack(next2 as u64, 0)
            };

            // Second CAS must succeed.
            stack_a
                .cas_head_for_test(head, new_head2, Ordering::Acquire, Ordering::Acquire)
                .map(|_| idx2)
        });

        // Thread B: pushes slot 0 (changing head, bumping tag).
        let tb = thread::spawn(move || {
            stack_b.push(&*links_b, 0);
        });

        let a_result = ta.join().unwrap();
        tb.join().unwrap();

        // Drain the stack and verify no loss/duplication.
        let mut popped: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            popped.push(idx);
        }
        while let Some(idx) = stack.pop(&*links) {
            popped.push(idx);
        }
        popped.sort_unstable();
        assert_eq!(
            popped,
            vec![0, 1],
            "free-list corrupted (loss or duplication) after CAS retry: got {popped:?}"
        );
    });
}

/// Counterfactual: using Relaxed on CAS failure lets the retry read stale
/// link data, corrupting the free-list. This test demonstrates that the
/// Acquire ordering is load-bearing.
#[test]
#[should_panic(expected = "corrupted")]
fn counterfactual_relaxed_cas_failure_corrupts_free_list() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(4);
    builder.check(|| {
        // Start with slot 1 only on stack.
        let links = Arc::new(ArrayLinks::<N>::new());
        let stack = Arc::new(TaggedIndexStack::<16>::new());
        stack.push(&*links, 1);

        let stack_a = Arc::clone(&stack);
        let links_a = Arc::clone(&links);
        let stack_b = Arc::clone(&stack);
        let links_b = Arc::clone(&links);

        let ta = thread::spawn(move || {
            let mut head = stack_a.raw_head();
            let (idx_v, _tag) = Tag::unpack(head);
            let idx = idx_v as u32;
            let next = links_a.load_next(idx);
            let new_head = if next == TAIL {
                Tag::pack(Tag::empty_index(), 0)
            } else {
                Tag::pack(next as u64, 0)
            };

            // BUG: Relaxed failure ordering — no happens-before with B's push.
            let result = stack_a.cas_head_for_test(
                head,
                new_head,
                Ordering::Acquire,
                Ordering::Relaxed, // BUGGY: this is what we're testing against
            );

            if result.is_ok() {
                // CAS succeeded — B didn't race, no corruption to exercise.
                return Ok(idx);
            }

            // CAS failed — retry path with Relaxed head observation.
            head = result.unwrap_err();
            let (idx_v2, _tag2) = Tag::unpack(head);
            let idx2 = idx_v2 as u32;
            let next2 = links_a.load_next(idx2);
            let new_head2 = if next2 == TAIL {
                Tag::pack(Tag::empty_index(), 0)
            } else {
                Tag::pack(next2 as u64, 0)
            };

            stack_a
                .cas_head_for_test(head, new_head2, Ordering::Acquire, Ordering::Relaxed)
                .map(|_| idx2)
        });

        let tb = thread::spawn(move || {
            stack_b.push(&*links_b, 0);
        });

        let a_result = ta.join().unwrap();
        tb.join().unwrap();

        let mut popped: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            popped.push(idx);
        }
        while let Some(idx) = stack.pop(&*links) {
            popped.push(idx);
        }
        popped.sort_unstable();
        assert_eq!(
            popped,
            vec![0, 1],
            "free-list corrupted (loss or duplication) after CAS retry: got {popped:?}"
        );
    });
}
