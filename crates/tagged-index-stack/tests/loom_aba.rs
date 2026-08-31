//! loom model-check of the REAL [`TaggedIndexStack`] / [`TaggedIndex`] types.
//!
//! Under `--cfg loom` the crate aliases its atomics to `loom::sync::atomic`,
//! so the head atomic and the `TaggedIndex` packing loom explores here ARE the
//! code that ships. How much of each model calls the shipped `push`/`pop`
//! directly varies and is stated per model below: **three** models
//! (`pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`,
//! `push_push_conservation`, `pop_pop_conservation`) run end-to-end through
//! the real `push`/`pop`, most of the rest hand-inline one side of an
//! interaction through `cas_head_for_test` (real head atomic, real packing)
//! to pin an interleaving — the one exception is the untagged-ABA
//! counterfactual, which drives a locally-defined buggy stand-in stack
//! instead of the real type, to prove the harness non-vacuous. This module
//! doc is the source of truth for this per-model breakdown; other published
//! copies (crate-root rustdoc, README.md, CHANGELOG.md) point back here
//! rather than repeating a specific count.
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
//!     A's single-shot CAS attempt races against B's repush and may
//!     legitimately SUCCEED or FAIL depending on scheduling — there is no
//!     property requiring a specific outcome. (A specific outcome is only
//!     assertable in a rendezvous-pinned scenario such as (d) below, which
//!     guarantees B's full pop+push completes between A's load and A's CAS,
//!     so the tag bump always defeats A's stale CAS.)
//! (b) Regardless of which way (a)'s race resolves, the free-list stays
//!     loss/duplication-free — the actual property
//!     `aba_repush_keeps_free_list_conservation` asserts.
//! (c) **Untagged counterfactual** (`#[should_panic]`): a bare `AtomicU32` head
//!     with NO tag lets a same-shape interleaving corrupt the free-list —
//!     proving the harness is non-vacuous and the tag is load-bearing. Also
//!     checked under a live-consumer-shaped variant (B pops TWO indices and
//!     re-pushes only the first, holding the second) via
//!     `counterfactual_untagged_head_lets_aba_corrupt_free_list` and its
//!     tagged companion `tagged_stack_survives_the_same_resurrection_pattern`.
//! (d) **H-2 empty-transition:** the REAL `pop` preserves the running tag across
//!     a drain-to-empty, so a stalled popper's CAS fails (fixed); a buggy pop
//!     that packs `TaggedIndex::empty()` (tag 0) on the drain lets the stale CAS
//!     recur — the `#[should_panic]` counterfactual
//!     `counterfactual_empty_transition_tag_reset_lets_aba_recur`.
//! (e) **CAS-failure ordering:** a pop that retries after a failed CAS must
//!     Acquire-synchronize with the push that caused the failure, checked both
//!     via a hand-inlined exposition harness and an end-to-end regression test
//!     calling the real `pop`/`push` directly — the `#[should_panic]`
//!     counterfactual `counterfactual_relaxed_cas_failure_corrupts_free_list`
//!     proves a `Relaxed` failure ordering lets this corrupt the free-list.
//! (f) **push‖push and pop‖pop conservation:** two threads each doing ONE
//!     real `push` (`push_push_conservation`) — or, on a stack pre-seeded
//!     with exactly 2 free indices, ONE real `pop`
//!     (`pop_pop_conservation`) — concurrently, driven end-to-end through the
//!     shipped `push`/`pop` with no hand-inlining. Both prove the free-list
//!     stays loss/duplication-free under the two most ordinary interleavings
//!     a production free-list sees (two threads freeing concurrently, two
//!     threads allocating concurrently); each also asserts its own
//!     activation oracle — `push_push_conservation` a `PUSH_RETRY_COUNT`
//!     delta, `pop_pop_conservation` a `POP_RETRY_COUNT` delta — mirroring
//!     the other's.
//!
//! # How to run
//!
//! ```text
//! RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --features loom --test loom_aba
//! ```
//!
//! No model sets loom's `preemption_bound`: every run is exhaustive over the
//! interleavings these small models admit, so a green positive model is a
//! complete result for its scenario, not a bounded sample — and the
//! whole suite still runs in a fraction of a second, so completeness costs
//! nothing here. (Loom's separate per-execution branch cap, 1000 by default,
//! stays armed as a LOUD panic valve, not a silent truncation.)
//!
//! # `MODEL_LOCK` serialization
//!
//! Every `#[test]` in this file drives its model through one of two helpers,
//! [`model`] or [`model_with_oracle`], both of which acquire `MODEL_LOCK`
//! internally — the guard is acquired BY CONSTRUCTION, not by per-test
//! discipline. A new test cannot forget to serialize with the rest of this
//! file's tests the way two earlier tests once did (round-4 scoped the lock
//! to only `pop`'s oracle; round-5's own remediation then added
//! `push_push_conservation` with a `PUSH_RETRY_COUNT` oracle outside that
//! scope, unnoticed for a full review round) — see `MODEL_LOCK`'s own doc
//! comment for why serialization matters at all.

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

use tagged_index_stack::{ArrayLinks, Links, TaggedIndex, TaggedIndexStack, TAIL};

/// Serializes every test in this file that drives the REAL `push` or `pop`
/// under contention. `POP_RETRY_COUNT` / `PUSH_RETRY_COUNT` (`src/lib.rs`)
/// are single process-global counters incremented inside `pop`'s / `push`'s
/// own CAS-retry arm; libtest's default (parallel) harness runs this
/// binary's `#[test]` functions concurrently, so without this lock a delta
/// measured by one test's snapshot-before / assert-after window is NOT
/// exclusive to that test's own `check()` run — any other test in this
/// binary racing the real `push`/`pop` at the same time increments the SAME
/// counter, making the activation-oracle assertions in
/// `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` and
/// `push_push_conservation` vacuously satisfiable by cross-test noise
/// instead of by their own models.
/// `unwrap_or_else(|e| e.into_inner())`, not `.unwrap()`: a FAILING locked
/// model (e.g. `push_push_conservation` itself, if its own assertion failed
/// while holding the lock) would otherwise poison this mutex for every test
/// that acquires it afterward.
///
/// The guard is acquired by construction via [`model`] (or
/// [`model_with_oracle`]), not by per-test discipline: every `#[test]` in
/// this file that drives the real `push`/`pop` routes through one of those
/// two helpers, so a new test cannot forget the lock the way two earlier
/// rounds did (round-4 scoped the lock to only `pop`'s oracle; round-5's own
/// remediation then added `push_push_conservation` with a `PUSH_RETRY_COUNT`
/// oracle outside that scope, unnoticed for a full review round).
static MODEL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Every model in this file that does not read an activation-oracle counter
/// runs through here: the guard is acquired by construction, so a new test
/// cannot forget it. No model in this file sets any `Builder` field (the
/// module doc's "no preemption_bound" note above states this is deliberate),
/// so collapsing every call site's identical `Builder::new()` into this one
/// function also removes that duplication.
///
/// The three tests whose activation-oracle snapshot/assert window must span
/// the entire `check()` call — not just wrap it — use
/// [`model_with_oracle`] instead: `MODEL_LOCK` is a plain `std::sync::Mutex`,
/// which is not reentrant, so this function cannot be nested inside an
/// already-held `MODEL_LOCK` guard, and its own guard is dropped before
/// returning, so a snapshot taken after this call returns is not exclusive
/// to this call's `check()` run.
fn model<F>(f: F)
where
    F: Fn() + Sync + Send + 'static,
{
    let _g = MODEL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    loom::model::Builder::new().check(f);
}

/// Variant of [`model`] for the three tests whose activation-oracle
/// snapshot/assert window must cover the ENTIRE `check()` call, not just
/// wrap it: `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`,
/// `push_push_conservation`, `pop_pop_conservation`. Each snapshots a
/// process-global retry counter, runs its model, then asserts the counter
/// advanced — and that delta is only exclusive to this call's own `check()`
/// run if no other test's `check()` can interleave between the snapshot and
/// the assert, which is exactly what holding `MODEL_LOCK` the whole time
/// guarantees.
///
/// `snapshot` runs AFTER the lock is acquired and BEFORE `check` starts, so
/// the "before" reading is already inside the same critical section `check`
/// runs under; the guard is then returned to the caller, STILL HELD, so the
/// caller's own post-check "after" reading and delta assertion stay covered
/// by the same lock acquisition — the full "acquire lock -> snapshot counter
/// -> run model -> assert delta -> drop lock" ordering the oracle depends on.
fn model_with_oracle<F, S, T>(snapshot: S, f: F) -> (T, std::sync::MutexGuard<'static, ()>)
where
    F: Fn() + Sync + Send + 'static,
    S: FnOnce() -> T,
{
    let g = MODEL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let before = snapshot();
    loom::model::Builder::new().check(f);
    (before, g)
}

type Tag = TaggedIndex<16>;

// A 2-slot backing is sufficient for the ABA scenario when designed correctly.
const N: usize = 2;

/// Seed an `ArrayLinks<2>` + `TaggedIndexStack<16>` into the state "slot 0 on
/// top, chained to slot 1, chained to TAIL" — i.e. both slots free. Because the
/// crate's stack is lazy (a fresh stack is empty), we materialise this state by
/// pushing 1 then 0 through the REAL `push` (which sets links + tag exactly as
/// production does), leaving a running tag of 2.
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
fn aba_repush_keeps_free_list_conservation() {
    model(|| {
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
            "free-list corrupted (lost or duplicated index): draining after \
             A's single-shot CAS raced B's pop+repush yielded {popped:?}, \
             expected exactly [0, 1] — A's CAS may legitimately succeed or \
             fail depending on scheduling; either outcome must conserve the \
             free-list set"
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
    // Routed through `model()` too, even though this is the one model that
    // drives no crate code (a locally-defined `UntaggedStack`, never the real
    // `push`/`pop`/retry counters) and so does not strictly NEED `MODEL_LOCK`
    // for exclusivity — one extra serialization costs nothing here, and a
    // single call-site pattern for every `Builder::new()` use in this file is
    // simpler than carving out an exemption.
    model(|| {
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
        let (_b_popped_0, b_held_1) = tb.join().unwrap();

        // Conservation invariant, robust to scheduling: A's popped item (if
        // any), B's held-and-never-repushed item (if any), and everything the
        // final drain yields must together account for EXACTLY {0, 1} — no
        // index may appear twice (duplication/resurrection) AND no index may
        // be missing (loss). This mirrors
        // tagged_stack_survives_the_same_resurrection_pattern's oracle above,
        // which the tag defeats under the identical B-does-two-pops-then-one-
        // push scenario. The oracle is conservation-based rather than
        // `assert!(!popped.contains(&1))` because the latter is
        // scheduling-DEPENDENT: it fires on any schedule where A completes
        // cleanly before B's second pop can race it — a benign,
        // non-corrupting interleaving loom visits first — so the genuine ABA
        // interleaving is never reached before loom aborts model-checking at
        // that spurious panic.
        let mut accounted: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            accounted.push(idx);
        }
        if let Some(idx) = b_held_1 {
            accounted.push(idx);
        }

        // Drain the remaining stack.
        while let Some(idx) = reg.pop() {
            accounted.push(idx);
        }
        accounted.sort_unstable();
        assert_eq!(
            accounted,
            vec![0, 1],
            "free-list corrupted (lost or duplicate index) via the untagged \
             model: {accounted:?}. The untagged stack allowed A's stale CAS \
             to commit an incorrect chain, which the tag prevents (see \
             tagged_stack_survives_the_same_resurrection_pattern)."
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
// aba_repush_keeps_free_list_conservation.
// ============================================================================

#[test]
fn tagged_stack_survives_the_same_resurrection_pattern() {
    model(|| {
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
        // the final drain yields must together account for EXACTLY {0, 1} —
        // no index may appear twice (duplication/resurrection) AND no index
        // may be missing (loss — e.g. a stale CAS installing `empty` where
        // `next` was a real index, truncating the chain past a live slot).
        // Which specific index lands in which bucket is scheduling-dependent
        // and not itself significant; only the final SET matters.
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
        accounted.sort_unstable();
        assert_eq!(
            accounted,
            vec![0, 1],
            "tagged stack: an index was lost or resurrected/duplicated \
             across A's pop, B's held item, and the final drain: {accounted:?}"
        );
    });
}

// ============================================================================
// (d) H-2 empty-transition. The FIXED side runs the REAL `stack.pop` (which
// preserves the running tag on drain). The BUGGY side inlines a pop whose drain
// branch packs `TaggedIndex::empty()` (tag 0) — the exact buggy behaviour this
// counterfactual exists to expose — using the crate's own packing primitives.
// A two-flag rendezvous guarantees B's full pop+push is sandwiched between
// A's load and A's CAS: a free race would admit benign orderings (A completing
// entirely before B) that false-positive the "stale CAS must fail" assertion.
// ============================================================================

fn run_h2(preserve_tag_on_drain: bool) {
    model(move || {
        // Seed: ONE real `push` puts slot 0 on a fresh (lazy, empty) stack
        // and leaves the running tag at exactly 1. Tag 1 is not merely
        // sufficient but the ONLY seed that can exercise this counterfactual:
        // B's buggy drain resets the tag to 0 and its refill computes 0 + 1
        // = 1, so a collision requires A's stale snapshot to carry exactly
        // that tag 1 — no higher seeded tag can ever recur through this
        // drain.
        let links = Arc::new(ArrayLinks::<1>::new());
        let stack = Arc::new(TaggedIndexStack::<16>::new());
        stack.push(&*links, 0);
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
        // NOTE: this branch on `preserve_tag_on_drain` computes `new_head` --
        // the value A's CAS would WRITE on success -- but a
        // `compare_exchange`'s success/failure depends only on `current`
        // (`head`, captured above, before B's cycle) matching the atomic's
        // present value, never on `new`. So this branch cannot itself affect
        // whether A's CAS below succeeds or fails; only thread B's drain
        // behaviour (real `pop` vs `bug_pop_drain_to_empty`) does that. It's
        // here for faithfulness -- computing exactly what a real caller in
        // A's position would compute -- not because it changes the asserted
        // outcome.
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
/// ever regresses.
///
/// That last claim used to rest on a one-off manual experiment ("revert
/// `pop`'s failure ordering to `Relaxed` and this test fails with
/// `left: [0, 0, 1]`, `right: [0, 1]` — index 0 duplicated, a real
/// double-allocated free-list slot, then passes again once reverted") — a
/// receipt about a mutated working tree that no longer exists and cannot be
/// re-run. It is SUPERSEDED by the live activation oracle asserted on every
/// run below: the process-global `pop_retry_count_for_test` counter
/// (incremented in `pop`'s own retry arm) must ADVANCE across this model's
/// explored schedules, so a green run proves the retry branch actually
/// executed — not merely that its absence went unnoticed.
#[test]
fn pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type() {
    // Activation oracle: snapshot the process-global retry counter BEFORE the
    // exploration and assert below that it advanced. A DELTA, not the raw
    // count — but a delta alone is not enough under libtest's default
    // parallel harness: `MODEL_LOCK`, held from the snapshot through the
    // assert below via `model_with_oracle`'s returned guard, is what
    // actually makes the delta exclusive to this test's own `check()` run.
    // Without it, any other test in this file that drives the real
    // `push`/`pop` (see `MODEL_LOCK`'s own doc comment for the mechanism)
    // could run concurrently on another libtest thread and increment the
    // same counter, making this assertion pass on cross-test noise instead
    // of on this test's own model.
    let (retries_before, _g) =
        model_with_oracle(tagged_index_stack::pop_retry_count_for_test, || {
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

    let retried = tagged_index_stack::pop_retry_count_for_test() - retries_before;
    assert!(
        retried > 0,
        "activation oracle: `pop`'s CAS-retry branch was never reached in \
         any explored schedule — this test is vacuously green, since its \
         free-list conservation assertion cannot catch a stale-retry \
         corruption if no retry ever executes"
    );
}

/// Shared harness for the two `(e)` tests below: thread A hand-expands TWO
/// iterations of `pop`'s loop so loom must explore the retry path (the first
/// CAS fails because B interposes; the retry must then succeed with fresh
/// data), thread B pushes concurrently, and a free-list conservation oracle
/// runs at the end. `failure_ordering` is the failure ordering passed to BOTH
/// `cas_head_for_test` calls — `Ordering::Acquire` (the shipped behaviour)
/// must keep the free-list intact; `Ordering::Relaxed` (the counterfactual)
/// must let the retry read a stale link and corrupt it.
fn run_cas_retry(failure_ordering: Ordering) {
    model(move || {
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
            let (idx_v, tag) = Tag::unpack(head);
            let idx = idx_v as u32;
            let next = links_a.load_next(idx);
            let new_head = if next == TAIL {
                Tag::pack(Tag::empty_index(), tag)
            } else {
                Tag::pack(next as u64, tag)
            };

            // CAS fails (B pushed in between). The CAS may succeed if B
            // hasn't run yet — only the failure path exercises the bug.
            let result = stack_a.cas_head_for_test(
                head,
                new_head,
                Ordering::Acquire,
                failure_ordering, // the knob under test — see the counterfactual below
            );
            if result.is_ok() {
                // No race — B didn't interpose, nothing to test.
                return Ok(idx);
            }

            // Iteration 2: RETRY with the actual head from the failure.
            head = result.unwrap_err();
            let (idx_v2, tag2) = Tag::unpack(head);
            let idx2 = idx_v2 as u32;
            let next2 = links_a.load_next(idx2);
            // Both candidate heads pack the tag actually observed off the
            // head (`tag` / `tag2`), mirroring the real `pop`'s H-2
            // tag-preservation rule exactly — the running tag is kept
            // across the empty transition and the non-empty transition
            // alike, with no hardcoded placeholder. (The end-to-end
            // regression guard at
            // `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`
            // above additionally drives the real `pop`'s own retry loop.)
            let new_head2 = if next2 == TAIL {
                Tag::pack(Tag::empty_index(), tag2)
            } else {
                Tag::pack(next2 as u64, tag2)
            };

            // Second CAS must succeed.
            stack_a
                .cas_head_for_test(head, new_head2, Ordering::Acquire, failure_ordering)
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

/// Test the CAS retry path: thread A loads head, reads link; thread B pushes
/// (changing head with a fresh tag); A's CAS fails; A retries — the retry's
/// head observation AND its subsequent link read must both synchronize with
/// B's push.
#[test]
fn cas_retry_path_must_acquire_with_concurrent_push() {
    run_cas_retry(Ordering::Acquire);
}

/// Counterfactual: using Relaxed on CAS failure lets the retry read stale
/// link data, corrupting the free-list. This test demonstrates that the
/// Acquire ordering is load-bearing.
#[test]
#[should_panic(expected = "corrupted")]
fn counterfactual_relaxed_cas_failure_corrupts_free_list() {
    run_cas_retry(Ordering::Relaxed);
}

// ============================================================================
// (f) push‖push and pop‖pop: the two most ordinary interleavings a
// production free-list sees (two threads freeing concurrently, two threads
// allocating concurrently), driven end-to-end through the REAL `push`/`pop`
// — neither hand-inlined nor raced against a hand-unrolled stand-in.
// ============================================================================

/// Two threads each do ONE real [`TaggedIndexStack::push`] of a DIFFERENT
/// index onto a shared fresh stack, concurrently. Proves push‖push
/// conservation: regardless of which push's CAS wins the race (the loser
/// retries with the winner's new head as its base, re-reading the current
/// head and re-chaining its own link before retrying — see `push`'s loop),
/// draining the stack afterward yields both pushed indices exactly once
/// each, in EITHER order. LIFO order between two concurrent pushes is not
/// commit-ordered by anything this crate promises, so the oracle checks the
/// drained MULTISET against the pushed multiset, not a specific pop order.
///
/// Also asserts the `PUSH_RETRY_COUNT` activation oracle advances across
/// this model's explored schedules (mirroring
/// `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`'s use
/// of `POP_RETRY_COUNT`): a green run with zero retries would only prove
/// two independent pushes can succeed when they never collide, not that a
/// losing push's retry re-chains correctly — the actual property this test
/// exists to cover.
#[test]
fn push_push_conservation() {
    let (retries_before, _g) =
        model_with_oracle(tagged_index_stack::push_retry_count_for_test, || {
            let links = Arc::new(ArrayLinks::<N>::new());
            let stack = Arc::new(TaggedIndexStack::<16>::new());

            let stack_a = Arc::clone(&stack);
            let links_a = Arc::clone(&links);
            let ta = thread::spawn(move || {
                stack_a.push(&*links_a, 0);
            });

            let stack_b = Arc::clone(&stack);
            let links_b = Arc::clone(&links);
            let tb = thread::spawn(move || {
                stack_b.push(&*links_b, 1);
            });

            ta.join().unwrap();
            tb.join().unwrap();

            let mut popped: Vec<u32> = Vec::new();
            while let Some(idx) = stack.pop(&*links) {
                popped.push(idx);
            }
            popped.sort_unstable();
            assert_eq!(
                popped,
                vec![0, 1],
                "free-list corrupted (lost or duplicated index) after two \
             concurrent real pushes: draining yielded {popped:?}, expected \
             exactly [0, 1] regardless of which push's CAS won the race"
            );
        });

    let retried = tagged_index_stack::push_retry_count_for_test() - retries_before;
    assert!(
        retried > 0,
        "activation oracle: `push`'s CAS-retry branch was never reached in \
         any explored schedule — this test is vacuously green, since its \
         free-list conservation assertion cannot catch a stale-retry \
         corruption if no retry ever executes"
    );
}

/// Two threads each do ONE real [`TaggedIndexStack::pop`] concurrently
/// against a stack pre-seeded with exactly 2 free indices. Proves pop‖pop
/// conservation for the 2-elements/2-poppers case.
///
/// **Derivation of the asserted outcome (traced from `pop`'s actual CAS
/// loop, not assumed):** `pop` never returns `None` from inside its retry
/// loop except at loop-top, when it observes the head as empty
/// (`TaggedIndex::is_empty`). A failed CAS does not exit the loop — it
/// re-reads the CAS failure's `actual` head and retries with THAT fresh
/// state, which is always synchronized (the failure ordering is `Acquire`)
/// with whatever the other popper's successful CAS just installed. With
/// exactly 2 elements and exactly 2 concurrent poppers and nothing else
/// touching the stack: whichever popper's CAS wins first pops one index and
/// advances the head to the other index (pop never bumps the tag, so no ABA
/// tag-mismatch can spuriously fail the loser's retry beyond the one
/// legitimate head-changed failure). The loser's CAS fails once, retries
/// against the now-single-element head, reads that element's link (`TAIL`),
/// and its retry CAS succeeds uncontested (no third party can race it) —
/// so it also returns `Some`. There is no reachable schedule in which
/// either popper observes an empty stack before completing: the stack does
/// not become empty until AFTER both pops have committed. Therefore the
/// only reachable outcome is BOTH poppers return `Some` with the two
/// DISTINCT seeded indices — never `None` for either, never a duplicate.
/// This is stronger than the general "subset, no duplicates" invariant used
/// elsewhere in this file for scenarios with a third concurrent actor (e.g.
/// `aba_repush_keeps_free_list_conservation`); here there is no third actor,
/// so the outcome space collapses to exactly one shape.
///
/// Also asserts the `POP_RETRY_COUNT` activation oracle advances across this
/// model's explored schedules (mirroring `push_push_conservation`'s use of
/// `PUSH_RETRY_COUNT`): a green run with zero retries would only prove two
/// independent pops can succeed when they never collide, not that the
/// loser's retry — reading the now-single-element head's link and
/// succeeding uncontested — actually recovers correctly, which is the
/// property the derivation above traces and this test exists to cover.
#[test]
fn pop_pop_conservation() {
    let (retries_before, _g) =
        model_with_oracle(tagged_index_stack::pop_retry_count_for_test, || {
            let (stack, links) = both_free();

            let stack_a = Arc::clone(&stack);
            let links_a = Arc::clone(&links);
            let ta = thread::spawn(move || stack_a.pop(&*links_a));

            let stack_b = Arc::clone(&stack);
            let links_b = Arc::clone(&links);
            let tb = thread::spawn(move || stack_b.pop(&*links_b));

            let a_result = ta.join().unwrap();
            let b_result = tb.join().unwrap();

            assert!(
                a_result.is_some() && b_result.is_some(),
                "a concurrent popper observed None against a 2-element stack \
             with only 2 poppers and no other concurrent actor: a={a_result:?}, \
             b={b_result:?} — per this test's doc comment, this outcome is \
             unreachable through pop's real retry loop, so its occurrence \
             means the traced derivation was wrong or the shipped `pop` \
             regressed"
            );
            let mut popped = [a_result.unwrap(), b_result.unwrap()];
            popped.sort_unstable();
            assert_eq!(
                popped,
                [0, 1],
                "free-list corrupted (duplicated index) after two concurrent \
             real pops: got {popped:?}, expected exactly [0, 1] — both \
             poppers returned Some but did not partition the two seeded \
             indices"
            );
            assert!(
                stack.pop(&*links).is_none(),
                "stack not fully drained by the two concurrent pops: a third \
             index was available afterward, meaning fewer than 2 indices \
             were actually handed out despite both poppers reporting Some"
            );
        });

    let retried = tagged_index_stack::pop_retry_count_for_test() - retries_before;
    assert!(
        retried > 0,
        "activation oracle: `pop`'s CAS-retry branch was never reached in \
         any explored schedule — this test is vacuously green, since its \
         free-list conservation assertion cannot catch a stale-retry \
         corruption if no retry ever executes"
    );
}
