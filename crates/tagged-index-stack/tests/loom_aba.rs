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

// A tiny 2-slot backing is enough to exercise the ABA scenario (A targets slot
// 0; B pops slot 0, re-pushes it, bumping its tag).
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
                .cas_head_for_test(head, new_head, Ordering::Acquire, Ordering::Relaxed)
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
    fn both_free() -> Arc<Self> {
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
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let reg = UntaggedStack::both_free();

        let reg_a = Arc::clone(&reg);
        let ta = thread::spawn(move || {
            let head = reg_a.head.load(Ordering::Acquire);
            if head == TAIL {
                return Err(head);
            }
            let next = reg_a.next[head as usize].load(Ordering::Acquire);
            reg_a
                .head
                .compare_exchange(head, next, Ordering::Acquire, Ordering::Relaxed)
        });

        let reg_b = Arc::clone(&reg);
        let tb = thread::spawn(move || {
            if let Some(idx) = reg_b.pop() {
                reg_b.push(idx);
            }
        });

        let a_result = ta.join().unwrap();
        tb.join().unwrap();

        let mut popped: Vec<u32> = Vec::new();
        if a_result.is_ok() {
            popped.push(0);
        }
        while let Some(idx) = reg.pop() {
            popped.push(idx);
        }
        popped.sort_unstable();
        assert_eq!(
            popped,
            vec![0, 1],
            "free-list corrupted (loss or duplication): got {popped:?}"
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
