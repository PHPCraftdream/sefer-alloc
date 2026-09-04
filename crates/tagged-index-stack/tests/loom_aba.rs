//! loom model-check of the REAL [`ArrayIndexStack`] / [`TaggedIndex`] types.
//!
//! Under `--cfg loom` the crate aliases its atomics to `loom::sync::atomic`,
//! so the head atomic and the `TaggedIndex` packing loom explores here ARE the
//! code that ships. How much of each model calls the shipped `push`/`pop`
//! directly varies and is stated per model below: **seven** models
//! (`pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`,
//! `push_push_conservation`,
//! `counterfactual_same_index_concurrent_push_self_loops`,
//! `pop_repush_after_publish_conserves`,
//! `pop_pop_conservation`,
//! `pop_pop_single_element_loser_sees_empty_actual`,
//! `tiny_tag_seal_rejects_stale_cas_at_the_real_width`) run end-to-end through
//! ArrayIndexStack's shipped `push`/`pop` for their whole schedule (the
//! eighth, `counterfactual_bypassed_seal_lets_stale_cas_double_issue`, runs
//! the shipped `push`/`pop` for everything except its one deliberately
//! bypassed final step — see section (h)); most of the rest hand-inline one
//! side of an interaction through `cas_head_for_test` (real head atomic,
//! real packing) to pin an interleaving — the one exception is the
//! untagged-ABA counterfactual, which drives a locally-defined buggy
//! stand-in stack instead of the real type, to prove the harness
//! non-vacuous. This module doc is the source of truth for this per-model
//! breakdown; other published copies (crate-root rustdoc, README.md,
//! CHANGELOG.md) point back here rather than repeating a specific count.
//!
//! # What loom covers
//!
//! - `ArrayIndexStack<16, N>` — the fused stack: a `TaggedIndexStack`-style
//!   head (`AtomicU64`, packed `(index | tag << 16)`) owning its `ArrayLinks<N>`
//!   slot-resident `AtomicU32` links, `TAIL` end-of-chain.
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
//! (g) **Empty-`actual` retry (pop's skip-backoff arm):**
//!     `pop_pop_single_element_loser_sees_empty_actual` races TWO real
//!     `pop`s against a stack pre-seeded with exactly ONE free index: both
//!     poppers read the same head snapshot, only ONE CAS can succeed, and
//!     the loser's CAS fails against an EMPTY `actual` — pop's Err arm then
//!     skips its exponential-backoff spin (`is_empty(actual) == true`)
//!     and the loop-top empty check returns `None` without spinning. The
//!     only head transition this model admits is `(0, t) -> (empty, t)`, so
//!     its `POP_RETRY_COUNT` delta is provably an empty-`actual` retry —
//!     a path no other shipped model or test reaches.
//! (h) **Tiny-tag seal:** the stale-observer
//!     counterexample (P observes a stale head and pauses; Q pops both
//!     chained indices, churns one of them through a real push/pop cycle,
//!     then attempts a final push) replayed at the REAL 48-bit-plus tag
//!     width by seeding the head's tag `TINY_SEAL_MARGIN` pushes short of
//!     [`TaggedIndex::TAG_MAX`] — never by reducing `TAG_BITS` via a cfg.
//!     `tiny_tag_seal_rejects_stale_cas_at_the_real_width` drives Q's final
//!     step through the REAL, sealing `push` (asserts it returns
//!     `Err(TagExhausted)` and that P's stale CAS is rejected);
//!     `counterfactual_bypassed_seal_lets_stale_cas_double_issue`
//!     hand-inlines what the OLD wrapping `push` would have installed for
//!     that one step (bypassing the `TAG_MAX` check with
//!     `store_next_for_test` + a raw `cas_head_for_test`) and proves P's
//!     stale CAS then SUCCEEDS and the free-list conservation check FAILS —
//!     the load-bearing proof that the seal, not just the tag bump, is what
//!     closes the stale-CAS double-issue hole.
//! (i) **Same-index concurrent push (the caller contract's
//!     exclusive-ownership clause):**
//!     `counterfactual_same_index_concurrent_push_self_loops` races TWO
//!     real `push`es of the SAME index on a fresh stack — a deliberate
//!     violation of clause 3, with both calls satisfying the entry-time
//!     clauses (link domain, liveness). Loom finds the corrupting
//!     interleaving: the loser's CAS-retry observes the winner's
//!     just-published head and chains `next[0] = 0`, a self-loop, and the
//!     shipped `pop`'s self-loop detector panics on the schedules whose
//!     drain observes it — proving clause 3 is load-bearing. A per-schedule
//!     `PUSH_RETRY_COUNT` delta gate means only the
//!     genuinely-overlapping schedules drain, so the sequential double-push
//!     (a clause-2 violation at the second caller's own entry) can never be
//!     the schedule that satisfies `#[should_panic]`.
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
//! internally — the guard is acquired by construction, not by per-test
//! discipline, so a new test cannot forget to serialize with the rest of
//! this file's tests. See `MODEL_LOCK`'s own doc comment for why
//! serialization matters at all.

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

use tagged_index_stack::{ArrayIndexStack, TagExhausted, TaggedIndex, TAIL};

/// Serializes every test in this file that drives the REAL `push` or `pop`
/// under contention. `POP_RETRY_COUNT` / `PUSH_RETRY_COUNT` (`src/imp.rs`)
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
static MODEL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Activation oracle for
/// `pop_repush_after_publish_conserves`: whether at least one
/// explored schedule had thread B's `pop` return the index A's push had
/// already published (as opposed to popping before the publish and
/// returning `None`). A real `std::sync::atomic` (deliberately NOT
/// `loom::sync::atomic`): it is not part of the modeled state, so it adds
/// no schedules to explore, and like `PUSH_RETRY_COUNT` it survives loom's
/// re-runs. Written only by that one test's thread B, and read by the same
/// test after its `model()` call returns, so — unlike a
/// snapshot-before/assert-after counter window — it needs no `MODEL_LOCK`
/// exclusivity: no other test touches it.
static POP_OBSERVED_PUBLISHED_INDEX: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Every model in this file that does not read an activation-oracle counter
/// runs through here: the guard is acquired by construction, so a new test
/// cannot forget it. No model in this file sets any `Builder` field (the
/// module doc's "no preemption_bound" note above states this is deliberate),
/// so collapsing every call site's identical `Builder::new()` into this one
/// function also removes that duplication.
///
/// The tests whose activation-oracle snapshot/assert window must span
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

/// Variant of [`model`] for the tests whose activation-oracle
/// snapshot/assert window must cover the entire `check()` call, not just
/// wrap it: `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`,
/// `push_push_conservation`, `pop_pop_conservation`,
/// `pop_pop_single_element_loser_sees_empty_actual`. Each snapshots a
/// process-global retry counter, runs its model, then asserts the counter
/// advanced — and that delta is only exclusive to this call's own `check()`
/// run if no other test's `check()` can interleave between the snapshot and
/// the assert, which is exactly what holding `MODEL_LOCK` the whole time
/// guarantees. Because the whole "snapshot -> `check` -> snapshot -> verify
/// delta" sequence runs inside this function while the lock is held, the
/// guard never needs to leave it — there is no `MutexGuard` for any caller
/// to mishandle.
///
/// `snapshot` runs once AFTER the lock is acquired and BEFORE `check`
/// starts (the "before" reading), and once more AFTER `check` returns (the
/// "after" reading) — both inside the same critical section `check` itself
/// runs under. `verify(before, after)` then runs, still holding the lock,
/// before the guard is dropped at the end of this function: the full
/// "acquire lock -> snapshot before -> run model -> snapshot after -> verify
/// delta -> drop lock" ordering the oracle depends on, entirely internal.
fn model_with_oracle<F, S, T>(snapshot: S, f: F, verify: impl FnOnce(T, T))
where
    F: Fn() + Sync + Send + 'static,
    S: Fn() -> T,
{
    let _g = MODEL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let before = snapshot();
    loom::model::Builder::new().check(f);
    let after = snapshot();
    verify(before, after);
}

type Tag = TaggedIndex<16>;

/// The checked `pack` with the mirror model's in-range proof made
/// explicit: every index half handed to the hand-inlined pop bodies below
/// is either `TaggedIndex::empty_index()` or a link read from the model's
/// own storage (< N < 2^INDEX_BITS), and every tag comes from
/// `TaggedIndex::unpack`, so `Some` is guaranteed. The shipped
/// `push_index`/`pop_index` pack through the crate-PRIVATE truncating
/// fast path (their guards prove the same precondition); this test cannot
/// name that private item, so it produces the same words through the
/// checked public one. A `None` here would be a model bug — panicking is
/// the right outcome, and the `expect` adds no atomic ops for loom to
/// interleave.
fn tag_pack(index: u32, tag: u64) -> u64 {
    Tag::pack(index, tag).expect("mirror-model halves are in range by construction")
}

// A 2-slot backing is sufficient for the ABA scenario when designed correctly.
const N: usize = 2;

/// Seed an `ArrayIndexStack<16, 2>` into the state "slot 0 on top, chained to
/// slot 1, chained to TAIL" — i.e. both slots free. Because the crate's stack
/// is lazy (a fresh stack is empty), we materialise this state by pushing 1
/// then 0 through the REAL `push` (which sets links + tag exactly as
/// production does), leaving a running tag of 2.
fn both_free() -> Arc<ArrayIndexStack<16, N>> {
    let stack = Arc::new(ArrayIndexStack::<16, N>::new());
    // SAFETY: fresh stack (domain 0..2); indices 1 and 0 are each in-domain and pushed exactly once.
    unsafe { stack.push(1) }.expect("fresh head has tag budget");
    unsafe { stack.push(0) }.expect("fresh head has tag budget");
    stack
}

// ============================================================================
// (a) + (b): the classic ABA race against the REAL type.
// ============================================================================

#[test]
fn aba_repush_keeps_free_list_conservation() {
    model(|| {
        let stack = both_free();

        // Thread A: inline `pop`'s body ONCE (load head, read link, compute
        // candidate, CAS) so B can race between A's read and A's CAS — the ABA
        // window. This mirrors the REAL `pop`'s loop body exactly (same packing,
        // same orderings), just split so loom can interleave (packing through
        // the checked public `pack` via `tag_pack` — see that helper's doc).
        let stack_a = Arc::clone(&stack);
        let ta = thread::spawn(move || {
            let head = stack_a.raw_head();
            let (idx, tag) = Tag::unpack(head);
            let next = stack_a.load_next_for_test(idx);
            let new_head = if next == TAIL {
                tag_pack(Tag::empty_index(), tag)
            } else {
                tag_pack(next, tag)
            };
            stack_a
                .cas_head_for_test(head, new_head, Ordering::Acquire, Ordering::Acquire)
                .map(|_| idx)
        });

        // Thread B: full pop+repush of the same index via the REAL type.
        let stack_b = Arc::clone(&stack);
        let tb = thread::spawn(move || {
            if let Some(idx) = stack_b.pop() {
                // SAFETY: idx was just returned by pop, so it is not live; in-domain by construction.
                unsafe { stack_b.push(idx) }.expect("tiny loom model never nears TAG_MAX");
            }
        });

        let a_result = ta.join().unwrap();
        tb.join().unwrap();

        let mut popped: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            popped.push(idx);
        }
        while let Some(idx) = stack.pop() {
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
        let stack = both_free();

        let stack_a = Arc::clone(&stack);
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
            let (idx, tag) = Tag::unpack(head);
            let next = stack_a.load_next_for_test(idx);
            let new_head = if next == TAIL {
                tag_pack(Tag::empty_index(), tag)
            } else {
                tag_pack(next, tag)
            };
            stack_a
                .cas_head_for_test(head, new_head, Ordering::Acquire, Ordering::Acquire)
                .map(|_| idx)
        });

        let stack_b = Arc::clone(&stack);
        let tb = thread::spawn(move || {
            // B does TWO pops (drains whatever is left) then re-pushes only
            // its FIRST pop, holding onto its second pop (if any) — the same
            // resurrection setup as the untagged counterfactual. NOTE: which
            // physical index ends up as B's "first" vs "second" pop depends
            // on scheduling (if A runs first, B's first pop is whatever A
            // left behind) — this is expected, not itself a defect; only a
            // DUPLICATE index across {A's result, B's held item, the final
            // drain} is a real corruption.
            let first = stack_b.pop();
            let held = stack_b.pop();
            if let Some(idx) = first {
                // SAFETY: idx was just returned by pop, so it is not live; in-domain by construction.
                unsafe { stack_b.push(idx) }.expect("tiny loom model never nears TAG_MAX");
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
        while let Some(idx) = stack.pop() {
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
        let stack = Arc::new(ArrayIndexStack::<16, 1>::new());
        // SAFETY: fresh stack (sole in-domain index 0); this is its first push.
        unsafe { stack.push(0) }.expect("fresh head has tag budget");
        let a_loaded = Arc::new(AtomicU32::new(0));
        let b_done = Arc::new(AtomicU32::new(0));

        // Thread B: waits for A's snapshot, then a full pop+push cycle on slot 0.
        // The FIXED build uses the REAL `stack.pop`; the BUGGY build uses a pop
        // whose drain branch resets the tag to 0 (`bug_pop_drain_to_empty`).
        let stack_b = Arc::clone(&stack);
        let a_loaded_b = Arc::clone(&a_loaded);
        let b_done_b = Arc::clone(&b_done);
        let tb = thread::spawn(move || {
            while a_loaded_b.load(Ordering::Acquire) == 0 {
                thread::yield_now();
            }
            let popped = if preserve_tag_on_drain {
                stack_b.pop()
            } else {
                bug_pop_drain_to_empty(&stack_b)
            };
            if let Some(idx) = popped {
                // SAFETY: idx was just returned by pop, so it is not live; in-domain by construction.
                unsafe { stack_b.push(idx) }.expect("tiny loom model never nears TAG_MAX");
            }
            b_done_b.store(1, Ordering::Release);
        });

        // Thread A: manual split pop. Uses the drain-branch behaviour under test
        // to compute its candidate, signals `a_loaded`, blocks on `b_done`, then
        // fires its CAS against the STALE captured head.
        let head = stack.raw_head();
        let (idx, tag) = Tag::unpack(head);
        let next = stack.load_next_for_test(idx);
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
                tag_pack(Tag::empty_index(), tag)
            } else {
                Tag::empty()
            }
        } else {
            tag_pack(next, tag)
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
fn bug_pop_drain_to_empty(stack: &ArrayIndexStack<16, 1>) -> Option<u32> {
    loop {
        let head = stack.raw_head();
        if Tag::is_empty(head) {
            return None;
        }
        let (idx, tag) = Tag::unpack(head);
        let next = stack.load_next_for_test(idx);
        let new_head = if next == TAIL {
            Tag::empty() // BUG: hardcoded tag 0 on the empty transition.
        } else {
            tag_pack(next, tag)
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
/// The live activation oracle asserted on every run — the process-global
/// `pop_retry_count_for_test` counter (incremented in `pop`'s own retry arm)
/// must advance across this model's explored schedules — proves the retry
/// branch actually executed, so a green run is not vacuous.
#[test]
fn pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type() {
    // Activation oracle: `model_with_oracle` snapshots the process-global
    // retry counter both before and after the exploration and runs `verify`
    // on the delta ITSELF, still holding `MODEL_LOCK` — the delta is only
    // exclusive to this test's own `check()` run because the lock never
    // leaves that function. Without it, any other test in this file that
    // drives the real `push`/`pop` (see `MODEL_LOCK`'s own doc comment for
    // the mechanism) could run concurrently on another libtest thread and
    // increment the same counter, making this assertion pass on cross-test
    // noise instead of on this test's own model.
    model_with_oracle(
        tagged_index_stack::pop_retry_count_for_test,
        || {
            let stack = Arc::new(ArrayIndexStack::<16, N>::new());
            // SAFETY: fresh stack (domain 0..2); index 1 is in-domain and this is its first push.
            unsafe { stack.push(1) }.expect("fresh head has tag budget");

            let stack_a = Arc::clone(&stack);
            let ta = thread::spawn(move || stack_a.pop());

            let stack_b = Arc::clone(&stack);
            let tb = thread::spawn(move || {
                // SAFETY: index 0 is in-domain and was never pushed, so not live.
                unsafe { stack_b.push(0) }.expect("tiny loom model never nears TAG_MAX");
            });

            let a_result = ta.join().unwrap();
            tb.join().unwrap();

            let mut popped: Vec<u32> = Vec::new();
            if let Some(idx) = a_result {
                popped.push(idx);
            }
            while let Some(idx) = stack.pop() {
                popped.push(idx);
            }
            popped.sort_unstable();
            assert_eq!(
                popped,
                vec![0, 1],
                "free-list corrupted (loss or duplication) via the real pop/push: got {popped:?}"
            );
        },
        |before, after| {
            assert!(
                after - before > 0,
                "activation oracle: `pop`'s CAS-retry branch was never reached in \
                 any explored schedule — this test is vacuously green, since its \
                 free-list conservation assertion cannot catch a stale-retry \
                 corruption if no retry ever executes"
            );
        },
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
        let stack = Arc::new(ArrayIndexStack::<16, N>::new());
        // SAFETY: fresh stack (domain 0..2); index 1 is in-domain and this is its first push.
        unsafe { stack.push(1) }.expect("fresh head has tag budget");

        let stack_a = Arc::clone(&stack);
        let stack_b = Arc::clone(&stack);

        // Thread A: does TWO iterations of pop's loop (manual expansion to
        // force loom to explore the retry path). First iteration will fail
        // because B interposes; second iteration must succeed with fresh data.
        let ta = thread::spawn(move || {
            // Iteration 1: load head, read link, compute candidate.
            let mut head = stack_a.raw_head();
            let (idx, tag) = Tag::unpack(head);
            let next = stack_a.load_next_for_test(idx);
            let new_head = if next == TAIL {
                tag_pack(Tag::empty_index(), tag)
            } else {
                tag_pack(next, tag)
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
            let (idx2, tag2) = Tag::unpack(head);
            let next2 = stack_a.load_next_for_test(idx2);
            // Both candidate heads pack the tag actually observed off the
            // head (`tag` / `tag2`), mirroring the real `pop`'s H-2
            // tag-preservation rule exactly — the running tag is kept
            // across the empty transition and the non-empty transition
            // alike, with no hardcoded placeholder. (The end-to-end
            // regression guard at
            // `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`
            // above additionally drives the real `pop`'s own retry loop.)
            let new_head2 = if next2 == TAIL {
                tag_pack(Tag::empty_index(), tag2)
            } else {
                tag_pack(next2, tag2)
            };

            // Second CAS must succeed.
            stack_a
                .cas_head_for_test(head, new_head2, Ordering::Acquire, failure_ordering)
                .map(|_| idx2)
        });

        // Thread B: pushes slot 0 (changing head, bumping tag).
        let tb = thread::spawn(move || {
            // SAFETY: index 0 is in-domain and was never pushed, so not live.
            unsafe { stack_b.push(0) }.expect("tiny loom model never nears TAG_MAX");
        });

        let a_result = ta.join().unwrap();
        tb.join().unwrap();

        // Drain the stack and verify no loss/duplication.
        let mut popped: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            popped.push(idx);
        }
        while let Some(idx) = stack.pop() {
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

/// Two threads each do ONE real [`ArrayIndexStack::push`] of a DIFFERENT
/// index onto a shared fresh stack, concurrently. Proves push‖push
/// conserves: whichever push's CAS wins the race (the loser retries with the
/// winner's new head as its base, re-reading the current head and re-chaining
/// its own link before retrying — see `push`'s loop), draining the stack
/// afterward yields both pushed indices exactly once each, in either order.
/// LIFO order between two concurrent pushes is not commit-ordered by anything
/// this crate promises, so the oracle checks the drained multiset against the
/// pushed multiset, not a specific pop order.
///
/// Also asserts the `PUSH_RETRY_COUNT` activation oracle advances across
/// this model's explored schedules: a green run with zero retries would only
/// prove two independent pushes can succeed when they never collide, not
/// that a losing push's retry re-chains correctly — the actual property this
/// test exists to cover.
#[test]
fn push_push_conservation() {
    model_with_oracle(
        tagged_index_stack::push_retry_count_for_test,
        || {
            let stack = Arc::new(ArrayIndexStack::<16, N>::new());

            let stack_a = Arc::clone(&stack);
            let ta = thread::spawn(move || {
                // SAFETY: fresh stack (domain 0..2); index 0 is in-domain, never pushed, and
                // distinct from B's index 1, so it is never live elsewhere.
                unsafe { stack_a.push(0) }.expect("fresh head has tag budget");
            });

            let stack_b = Arc::clone(&stack);
            let tb = thread::spawn(move || {
                // SAFETY: fresh stack (domain 0..2); index 1 is in-domain, never pushed, and
                // distinct from A's index 0, so it is never live elsewhere.
                unsafe { stack_b.push(1) }.expect("fresh head has tag budget");
            });

            ta.join().unwrap();
            tb.join().unwrap();

            let mut popped: Vec<u32> = Vec::new();
            while let Some(idx) = stack.pop() {
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
        },
        |before, after| {
            assert!(
                after - before > 0,
                "activation oracle: `push`'s CAS-retry branch was never reached in \
                 any explored schedule — this test is vacuously green, since its \
                 free-list conservation assertion cannot catch a stale-retry \
                 corruption if no retry ever executes"
            );
        },
    );
}

// ============================================================================
// (i) Same-index concurrent push: a deliberate violation of the caller
// contract's exclusive-ownership-epoch clause (clause 3):
// two pushes acting on ONE duplicated authority epoch.
// ============================================================================

/// Counterfactual: two threads each do ONE real [`ArrayIndexStack::push`] of
/// the SAME index onto a shared fresh stack, concurrently. Both calls
/// literally satisfy the contract's entry-time clauses at their own
/// invocation — `index` is in-domain (clause 1) and not reachable through
/// the head at that instant (clause 2) — yet the race corrupts the
/// free-list: whichever push loses its first CAS retries, observes the
/// winner's just-published head (the same index), and chains
/// `next[0] = 0` — a self-loop — and its own CAS can then succeed too.
/// The shipped `pop`'s self-loop detector (`pop_link_out_of_range`) panics
/// on the first drain pop. This pins the contract's THIRD clause
/// (exclusive ownership epoch: each push must consume a unique,
/// not-yet-consumed publish/recycle authority over the index — freshly
/// minted, or obtained from one specific successful pop; two pushes acting
/// on one duplicated epoch are forbidden) as load-bearing.
///
/// Why the retry gate: unchecked, loom may pick the schedule where A's
/// `push(0)` completes
/// ENTIRELY before B's begins. On that schedule B's own entry-time head read
/// already observes index 0 as live, so B pushing anyway is an ordinary
/// SEQUENTIAL double-push — a clause-2 violation at B's own entry, already
/// covered elsewhere in this suite — NOT the clause-3 scenario. That schedule
/// still writes `next[0] = 0` and still panics on drain, so
/// `#[should_panic]` passes without demonstrating the claimed scenario. The
/// fix is a per-schedule gate on `PUSH_RETRY_COUNT` (the process-global
/// counter `push`'s CAS-retry arm increments on every failed CAS, read via
/// `push_retry_count_for_test`): the closure snapshots the counter BEFORE
/// spawning the threads and computes the delta AFTER both joins, and only a
/// schedule with a POSITIVE delta proceeds to the drain. The discriminator
/// is sound: a genuinely-overlapping push that loses its first CAS MUST
/// retry (its expected value — the empty head — no longer matches once the
/// winner publishes), and exactly one CAS can fail per overlapping schedule
/// (the loser's retry CAS then succeeds uncontested), so `delta == 1` on
/// gate-passing schedules. A purely-sequential B reads A's published head
/// `(0, tag+1)` on its FIRST entry read and its first CAS succeeds
/// uncontested — zero retries, gate closed. And both entry reads seeing the
/// empty head is FORCED, not assumed, on positive-delta schedules: the only
/// head values this 2-thread fresh-stack model admits are `empty` then
/// `(0, tag+1)`, so a CAS failure is only possible for a thread whose read
/// saw the empty head, and a failure requires the other thread's publish to
/// have interleaved. Hence the gate selects EXACTLY the
/// both-clauses-satisfied-at-entry concurrent scenario and structurally
/// excludes the sequential double-push. The counter reads are real
/// `core::sync::atomic` loads, not loom-modeled state, so the explored
/// state space does not grow; delta==0 schedules skip their drain ops
/// entirely, shrinking it if anything.
///
/// Plain [`model`], not [`model_with_oracle`]: the panic unwinds through
/// `Builder::check`, so `model_with_oracle`'s after-snapshot could never
/// run on the schedules that panic — an activation-oracle `verify` closure
/// here would be partially unreachable code. This matches every other
/// `#[should_panic]` counterfactual in this file. The before/after counter
/// pair therefore lives INSIDE the per-schedule closure, where the panic
/// cannot skip it.
///
/// Non-vacuousness needs only the retry gate plus `#[should_panic]`: loom
/// explores every schedule this 2-thread model admits, the genuinely-
/// overlapping ones pass the gate, and on each gate-passing schedule the
/// drain panics DETERMINISTICALLY (coherence — see the drain comment in
/// the body), so a body that completes panic-free means the gate never
/// opened, and `#[should_panic]` fails the test loudly. The former
/// process-global `SAME_INDEX_RETRY_GATE_SEEN` disambiguation flag ("gate
/// never opened" vs "gate opened but drained benignly") is gone: once the
/// gate opens, a benign drain is not a possible outcome, so the second
/// scenario it existed to diagnose cannot arise.
/// The positive counterpart
/// `pop_repush_after_publish_conserves` below independently
/// proves the overlapping scenario is reachable in this suite's models.
#[test]
#[should_panic(expected = "the index's own link points back to itself — a self-loop")]
fn counterfactual_same_index_concurrent_push_self_loops() {
    model(|| {
        let retries_before = tagged_index_stack::push_retry_count_for_test();
        let stack = Arc::new(ArrayIndexStack::<16, N>::new());

        let stack_a = Arc::clone(&stack);
        let ta = thread::spawn(move || {
            // SAFETY (counterfactual — clause 3 DELIBERATELY violated):
            // fresh stack (domain 0..2); index 0 is in-domain (clause 1)
            // and not reachable at this call's entry (clause 2), but BOTH
            // threads push the SAME index backed by the SAME duplicated
            // freshly-minted authority epoch, violating the
            // exclusive-ownership-epoch clause — that violation is exactly
            // what this test proves is load-bearing.
            // On the schedules that reach the drain, BOTH entry reads
            // observed the empty head, and concurrency is proven by this
            // schedule's positive `PUSH_RETRY_COUNT` delta (see the retry
            // gate below).
            unsafe { stack_a.push(0) }.expect("fresh head has tag budget");
        });

        let stack_b = Arc::clone(&stack);
        let tb = thread::spawn(move || {
            // SAFETY (counterfactual — clause 3 DELIBERATELY violated):
            // identical to thread A's — same index, same duplicated
            // freshly-minted authority epoch, same entry-time satisfaction
            // of clauses 1 and 2. On the schedules that reach the drain,
            // BOTH entry reads observed the empty head, and concurrency is
            // proven by this schedule's positive `PUSH_RETRY_COUNT` delta
            // (see the retry gate below) — so B is never a sequential
            // double-push here. The contract's clause 3 forbids exactly
            // this, and the self-loop panic below proves the clause is
            // real.
            unsafe { stack_b.push(0) }.expect("fresh head has tag budget");
        });

        ta.join().unwrap();
        tb.join().unwrap();

        // Retry gate: drain reached ONLY on
        // gate-passing (genuinely-overlapping) schedules.
        let retried = tagged_index_stack::push_retry_count_for_test() - retries_before;
        if retried == 0 {
            // Sequential schedule (one push fully finished before the other
            // began): B's entry read saw A's published head, making B an
            // ordinary clause-2 double-push — not this test's scenario.
            // Skip the drain; loom explores other schedules.
            return;
        }
        // On a gate-passing schedule the drain CANNOT be benign, so the
        // panic below is deterministic: retry > 0 forces BOTH initial head
        // reads to have observed the empty head (derivation above), so the
        // loser's retry loop observed the winner's just-published head —
        // index 0 itself — stored `next[0] = 0` (the self-loop), and its
        // retry CAS then succeeded UNCONTESTED (the winner performs no
        // shared-memory access after its publishing CAS), installing the
        // FINAL head. Both `join()`s above give this drain happens-after
        // every write both threads made; per-location coherence forbids a
        // happens-after read from returning an earlier write in the same
        // location's modification order, so the first drain pop necessarily
        // reads the final head — never the first push's publication, never
        // empty — and the final `next[0] == 0`, and the shipped `pop`'s
        // self-loop detector panics. There is no "stale visibility" class
        // left for the drain to fall into after both joins. If no
        // explored schedule passes the gate, this body completes
        // panic-free and `#[should_panic]` fails the test.
        while stack.pop().is_some() {}
    });
}

// ============================================================================
// (j) The PERMITTED republish: pop-then-repush of a just-published index —
// the epoch contract's positive side, pinned so clause 3 cannot be read as
// forbidding it. (The original
// push's physical-return timing is not distinguished here — see the test's
// doc.)
// ============================================================================

/// Positive counterpart of
/// `counterfactual_same_index_concurrent_push_self_loops`: thread A does
/// ONE real [`ArrayIndexStack::push`] of index 0 on a fresh stack; thread B
/// pops and — when its pop returns the just-published 0 — re-pushes it.
/// Proves exactly one property: the publish -> pop -> repush sequence
/// CONSERVES the free-list — the drain yields exactly one 0, no panic, no
/// double-issue — on EVERY schedule of the model, and the activation-oracle
/// flag proves the interesting class (B's pop genuinely observing A's
/// published index rather than popping before the publish and returning
/// `None`) was actually among the explored schedules.
///
/// What this test does NOT prove: that B's pop+repush can run while A's
/// push call is still physically executing (before it returns). After A's
/// publishing CAS succeeds, `push` performs no further shared-memory
/// operation before returning, so "B between A's CAS and A's return" and
/// "B after A's return" have an IDENTICAL observable partial order — loom
/// cannot distinguish them, and no harness gating short of a test-only hook
/// inside the shipped `push` could. Physical-return timing is also
/// irrelevant to the algorithm's correctness: A's authority over index 0
/// ended at its own CAS (nothing it does afterward touches shared memory —
/// push_index clause 3's ownership-epoch framing), and B's push is backed
/// by B's OWN successful pop, a distinct later epoch, so no two pushes
/// ever consume one epoch and the self-loop shape is structurally
/// unconstructible. The contract states the not-yet-returned window as a
/// PERMISSION (push_index clause 3); this test pins that the same
/// publish -> pop -> repush sequence conserves the free-list, without
/// claiming to observe that window.
///
/// Why a flag and not a retry counter: on this model no CAS EVER fails.
/// The head's only possible transitions are `(empty, 0) -> (0, 1)` (A's
/// push — nothing else can move an empty head, since `pop` returns `None`
/// at loop-top on an empty observation and B's re-push cannot exist
/// before B's successful pop) `-> (empty, 1)` (B's pop) `-> (0, 2)`
/// (B's re-push), each uncontested by construction, so
/// `PUSH_RETRY_COUNT`/`POP_RETRY_COUNT` stay at zero on every schedule
/// and the contention-style oracles used by `push_push_conservation`/
/// `pop_pop_conservation` would assert nothing here. The flag is a real
/// `std::sync::atomic` (deliberately NOT loom-modeled — like the retry
/// counters, it adds no schedules to explore and survives loom's
/// re-runs), written only by this test's thread B on its pop-success path
/// and read once after `model()` returns, so it needs no `MODEL_LOCK`
/// exclusivity: no other test touches it.
#[test]
fn pop_repush_after_publish_conserves() {
    model(|| {
        let stack = Arc::new(ArrayIndexStack::<16, N>::new());

        let stack_a = Arc::clone(&stack);
        let ta = thread::spawn(move || {
            // SAFETY: fresh stack (domain 0..2); index 0 is in-domain,
            // never pushed before, and this call's authority over it is
            // freshly minted and consumed by its own head CAS. B's later
            // push of the same index is backed by B's own successful pop —
            // a distinct, later epoch — so no epoch is ever duplicated
            // (push_index clause 3).
            unsafe { stack_a.push(0) }.expect("fresh head has tag budget");
        });

        let stack_b = Arc::clone(&stack);
        let tb = thread::spawn(move || {
            if let Some(idx) = stack_b.pop() {
                assert_eq!(idx, 0, "only index 0 is ever pushed onto this stack");
                POP_OBSERVED_PUBLISHED_INDEX.store(true, std::sync::atomic::Ordering::Relaxed);
                // SAFETY: `idx` came out of THIS thread's own successful
                // `pop()` — that pop's winning head CAS transferred
                // publish/recycle authority for it to this thread, a
                // fresh, singly-obtained epoch (only one popper can win
                // the CAS for a given published instance), which is
                // exactly what push_index clause 3 requires. A's push of
                // this index consumed its own separately-minted epoch at
                // its own CAS, so the two pushes never share one — this
                // is the republish the epoch contract explicitly permits,
                // whether or not A's push call has physically returned
                // yet (return timing is not distinguished here and does
                // not matter: A's authority ended at its own CAS).
                unsafe { stack_b.push(idx) }.expect("tiny loom model never nears TAG_MAX");
            }
        });

        ta.join().unwrap();
        tb.join().unwrap();

        let mut popped: Vec<u32> = Vec::new();
        while let Some(idx) = stack.pop() {
            popped.push(idx);
        }
        assert_eq!(
            popped,
            vec![0],
            "free-list corrupted (lost or duplicated index) after B's \
             pop-then-repush of A's just-published index: draining yielded \
             {popped:?}, expected exactly [0] — the permitted republish \
             must conserve the free-list"
        );
    });
    assert!(
        POP_OBSERVED_PUBLISHED_INDEX.load(std::sync::atomic::Ordering::Relaxed),
        "activation oracle: B's pop never returned A's just-published index \
         in any explored schedule — every explored schedule had B pop before \
         A's publish (returning None), so the publish -> pop -> repush \
         scenario this test exists to pin was never exercised and the \
         conservation assert above was vacuous",
    );
}

/// Two threads each do ONE real [`ArrayIndexStack::pop`] concurrently
/// against a stack pre-seeded with exactly 2 free indices. Proves pop‖pop
/// conservation for the 2-elements/2-poppers case.
///
/// **Derivation of the asserted outcome (traced from `pop`'s actual CAS
/// loop, not assumed):** `pop` never returns `None` from inside its retry
/// loop except at loop-top, when it observes the head as empty. A failed
/// CAS does not exit the loop — it retries against the CAS failure's `actual`
/// head, which is always Acquire-synchronized with whatever the other
/// popper's successful CAS just installed. With exactly 2 elements, exactly
/// 2 poppers, and no third actor: the first winner pops one index and
/// advances the head to the other; the loser's CAS fails once, retries
/// against the now-single-element head, and succeeds uncontested — so both
/// poppers return `Some` with the two DISTINCT seeded indices; the stack
/// cannot be empty until after both pops commit, so neither ever sees
/// `None`. This is stronger than the general "subset, no duplicates"
/// invariant used elsewhere in this file where a third concurrent actor
/// exists (e.g. `aba_repush_keeps_free_list_conservation`): here the
/// outcome space collapses to exactly one shape.
///
/// Also asserts the `POP_RETRY_COUNT` activation oracle advances across this
/// model's explored schedules: a green run with zero retries would only
/// prove two independent pops can succeed when they never collide, not that
/// the loser's uncontested retry actually recovers correctly — the property
/// the derivation above traces and this test exists to cover.
#[test]
fn pop_pop_conservation() {
    model_with_oracle(
        tagged_index_stack::pop_retry_count_for_test,
        || {
            let stack = both_free();

            let stack_a = Arc::clone(&stack);
            let ta = thread::spawn(move || stack_a.pop());

            let stack_b = Arc::clone(&stack);
            let tb = thread::spawn(move || stack_b.pop());

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
                stack.pop().is_none(),
                "stack not fully drained by the two concurrent pops: a third \
             index was available afterward, meaning fewer than 2 indices \
             were actually handed out despite both poppers reporting Some"
            );
        },
        |before, after| {
            assert!(
                after - before > 0,
                "activation oracle: `pop`'s CAS-retry branch was never reached in \
                 any explored schedule — this test is vacuously green, since its \
                 free-list conservation assertion cannot catch a stale-retry \
                 corruption if no retry ever executes"
            );
        },
    );
}

// ============================================================================
// (g) Empty-`actual` retry: the 1-element counterpart of `pop_pop_conservation`'s
// 2-element shape. Two real poppers race a stack pre-seeded with exactly ONE
// free index, so the loser's CAS fails against an EMPTY `actual` and pop's
// Err-branch backoff-skip (`is_empty(actual) == true`) is taken — no other
// shipped model or test reaches that arm.
// ============================================================================

/// Two threads each do ONE real [`ArrayIndexStack::pop`] concurrently
/// against a stack pre-seeded with exactly 1 free index. Proves pop‖pop
/// conservation for the 1-element/2-poppers case AND activates `pop`'s
/// empty-`actual` skip-backoff arm.
///
/// **Derivation of the asserted outcome (traced from `pop`'s actual CAS
/// loop, not assumed):** both poppers may read the same head snapshot
/// `(0, t)`; only ONE `compare_exchange` against `(0, t)` can succeed. The
/// winner installs `(empty, t)` — `pop` preserves the running tag across
/// the drain (H-2). The loser's CAS therefore fails with an EMPTY `actual`,
/// taking `pop`'s Err arm: the retry counter increments, then the
/// `is_empty(actual) == true` guard SKIPS the exponential-backoff spin
/// (spinning would be pure wasted latency before the loop-top `None`),
/// assigns `head = actual`, and the loop-top empty check returns `None`. A
/// popper scheduled entirely after the winner committed instead sees the
/// empty head at loop top and returns `None` without ever reaching the Err
/// arm — both reachable outcome shapes are covered by the same assertions.
/// No third actor exists, so the outcome space is exactly {A wins, B wins}:
/// precisely one `Some(0)`, precisely one `None`, and the stack drains to
/// empty.
///
/// The only head transition this model admits is `(0, t) -> (empty, t)`, so
/// every `POP_RETRY_COUNT` increment the oracle asserts is provably a
/// failed CAS against an empty actual — the delta doubles as a
/// path-activation oracle for `pop`'s `is_empty(actual) == true`
/// skip-backoff arm specifically, which no other shipped model or test
/// reaches. Note this is stronger than in models with >1 element or a
/// concurrent push (e.g. `pop_pop_conservation`,
/// `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`),
/// where an increment could also come from a non-empty actual.
#[test]
fn pop_pop_single_element_loser_sees_empty_actual() {
    model_with_oracle(
        tagged_index_stack::pop_retry_count_for_test,
        || {
            // Seed exactly ONE element: slot 0 on a fresh (lazy, empty)
            // stack via the REAL push — running tag ends at exactly 1.
            let stack = Arc::new(ArrayIndexStack::<16, 1>::new());
            // SAFETY: fresh stack (sole in-domain index 0); this is its first push.
            unsafe { stack.push(0) }.expect("fresh head has tag budget");

            let stack_a = Arc::clone(&stack);
            let ta = thread::spawn(move || stack_a.pop());

            let stack_b = Arc::clone(&stack);
            let tb = thread::spawn(move || stack_b.pop());

            let a_result = ta.join().unwrap();
            let b_result = tb.join().unwrap();

            let popped: Vec<u32> = [a_result, b_result].iter().filter_map(|r| *r).collect();
            assert_eq!(
                popped.len(),
                1,
                "expected exactly one of the two concurrent poppers to win \
                 the single seeded index: got {popped:?} — a duplicate would \
                 be free-list corruption, two Nones would mean the seeded \
                 index vanished"
            );
            assert_eq!(
                popped,
                [0],
                "free-list corrupted (lost or duplicated index) after two \
                 concurrent real pops on a 1-element stack: got {popped:?}, \
                 expected exactly [0]"
            );
            assert!(
                stack.pop().is_none(),
                "stack not fully drained by the two concurrent pops: the \
                 single seeded index was handed out twice or the loser \
                 fabricated an index"
            );
        },
        |before, after| {
            assert!(
                after - before > 0,
                "activation oracle: `pop`'s CAS-retry branch was never reached in \
                 any explored schedule — this test is vacuously green, since its \
                 loser-returns-None assertion cannot catch a broken skip-backoff \
                 arm if no retry ever executes"
            );
        },
    );
}

// ============================================================================
// (h) Tiny-tag seal. See the module doc's "(h)" entry for the
// full description. Seeded at the REAL tag width (never a TAG_BITS-reducing
// cfg) a handful of pushes short of TaggedIndex::TAG_MAX, so the schedule
// stays loom-tractable while every arithmetic operation exercised is the
// crate's actual production packing.
// ============================================================================

/// How many successful pushes short of [`TaggedIndex::TAG_MAX`] the seeded
/// EMPTY head starts: the two chain-building pushes below consume 2, one
/// real push/pop "churn" cycle (Q re-pushes the same index and pops it
/// again) consumes 1 more, landing exactly on
/// the ceiling before the final step — see [`run_tiny_tag_seal`]'s
/// walk-through comments for the exact arithmetic. Chosen small purely so
/// the two-thread interleaving space loom must explore stays a "handful of
/// ops" (this crate's speed convention), not because a larger margin would
/// be unsound — the fix holds at any seed.
const TINY_SEAL_MARGIN: u64 = 3;

/// Shared harness for the two `(h)` tests. `bypass_seal == false` is the
/// FIXED path: Q's final step goes through the real, sealing
/// [`ArrayIndexStack::push`], which must return `Err(TagExhausted)` instead
/// of wrapping — P's stale CAS, captured before Q's churn began, is then
/// asserted to fail (the tag has moved past — never back to — P's stale
/// snapshot). `bypass_seal == true` is the counterfactual: Q's final step
/// instead hand-inlines what the OLD (pre-fix) wrapping `push` would have
/// done, using [`ArrayIndexStack::store_next_for_test`] +
/// [`ArrayIndexStack::cas_head_for_test`] — bypassing the `TAG_MAX` check
/// entirely — to install the exact head word a COMPLETED
/// `2^TAG_BITS`-push wrap-around would produce: `(q_a, p_stale_tag)`, i.e.
/// EXACTLY the stale word P is still holding. This is deliberately NOT a
/// literal `(index, 0)` raw CAS: a single real tag bump past `TAG_MAX`
/// truncates to tag 0 (see `TaggedIndex::pack_truncating`'s doc),
/// but reaching `p_stale_tag` again through real pushes needs an entire
/// `2^TAG_BITS`-push lap — the actual arithmetic content of "wrap" is
/// "returns to the exact starting tag after one full cycle", which is what
/// this raw CAS installs directly, collapsing the infeasible-to-run lap
/// into its end state rather than replaying it.
fn run_tiny_tag_seal(bypass_seal: bool) {
    model(move || {
        // Seed the EMPTY head TINY_SEAL_MARGIN pushes short of the ceiling.
        // `with_tag_for_test` is INITIALISATION (fresh atomic storage), not
        // a live-head mutation — see its own doc: the release-sequence
        // invariant on `head` is untouched.
        let seed_tag = Tag::TAG_MAX - TINY_SEAL_MARGIN;
        let stack = Arc::new(ArrayIndexStack::<16, N>::with_tag_for_test(seed_tag));

        // Build the A -> B -> TAIL chain with two REAL pushes — exactly
        // `both_free()`'s shape, just starting from the seeded near-ceiling
        // tag instead of a fresh tag-0 head. Consumes 2 of the margin: tag
        // goes seed_tag -> seed_tag+1 (B=1 pushed) -> seed_tag+2 (A=0
        // pushed).
        // SAFETY: freshly-seeded empty head (domain 0..2); indices 1 and 0
        // are each in-domain and pushed exactly once.
        unsafe { stack.push(1) }.expect("2 pushes remain within the seeded margin"); // B
        unsafe { stack.push(0) }.expect("1 push remains within the seeded margin"); // A
        let p_stale_tag = seed_tag + 2;
        assert_eq!(
            stack.pushes_remaining(),
            TINY_SEAL_MARGIN - 2,
            "chain-building pushes must consume exactly 2 of the seeded margin"
        );

        let p_loaded = Arc::new(AtomicU32::new(0));
        let q_done = Arc::new(AtomicU32::new(0));

        // Thread Q: the adversarial churn shape — pop A, pop B (holding
        // B), one real push(A)/pop(A) churn cycle, then a final step
        // exactly at the tag ceiling — using the REAL push/pop entry
        // points throughout except the counterfactual's one bypassed final
        // step.
        let stack_q = Arc::clone(&stack);
        let p_loaded_q = Arc::clone(&p_loaded);
        let q_done_q = Arc::clone(&q_done);
        let tq = thread::spawn(move || {
            while p_loaded_q.load(Ordering::Acquire) == 0 {
                thread::yield_now();
            }
            let q_a = stack_q.pop().expect("A is on top after the chain build");
            let q_b = stack_q.pop().expect("B is chained beneath A");
            // SAFETY: q_a was just returned by pop, so it is not live; in-domain by construction.
            unsafe { stack_q.push(q_a) }.expect("the churn cycle stays within the seeded margin");
            let q_a_again = stack_q
                .pop()
                .expect("the churn cycle's own push just re-published q_a");
            assert_eq!(
                q_a_again, q_a,
                "the churn cycle re-pops the exact index it just re-pushed"
            );
            assert_eq!(
                stack_q.pushes_remaining(),
                0,
                "the seed margin is exhausted by exactly the chain build (2) \
                 plus the one churn cycle (1) — TINY_SEAL_MARGIN pushes total"
            );

            let final_result: Result<(), TagExhausted> = if bypass_seal {
                // Counterfactual bypass — see run_tiny_tag_seal's own doc
                // for why the installed tag is p_stale_tag, not literal 0.
                // RAD-1: write the link the way a real push into an EMPTY
                // stack would (next[q_a] = TAIL) before publishing.
                stack_q.store_next_for_test(q_a, TAIL);
                let current = stack_q.raw_head();
                let wrapped_head = tag_pack(q_a, p_stale_tag);
                stack_q
                    .cas_head_for_test(current, wrapped_head, Ordering::Release, Ordering::Relaxed)
                    .expect("no concurrent writer exists between Q's own sequential steps");
                Ok(())
            } else {
                // Fixed: the real, sealing push — expected to return
                // Err(TagExhausted), confirming the seal engages under
                // this exact adversarial schedule.
                // SAFETY: q_a was just returned by pop, so it is not live; in-domain by construction.
                unsafe { stack_q.push(q_a) }
            };

            q_done_q.store(1, Ordering::Release);
            (q_a, q_b, final_result)
        });

        // Thread "P" (inline on the model thread, mirroring run_h2's
        // thread A): observes the stale head BEFORE Q's churn, pauses,
        // then fires its single-shot CAS only after Q's full sequence
        // (including the final step) completes.
        let p_head = stack.raw_head();
        let (p_idx, p_tag) = Tag::unpack(p_head);
        assert_eq!(
            p_tag, p_stale_tag,
            "P must observe the exact post-chain-build tag"
        );
        let p_next = stack.load_next_for_test(p_idx);
        p_loaded.store(1, Ordering::Release);
        while q_done.load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        let p_new_head = if p_next == TAIL {
            tag_pack(Tag::empty_index(), p_tag)
        } else {
            tag_pack(p_next, p_tag)
        };
        let p_result = stack
            .cas_head_for_test(p_head, p_new_head, Ordering::Acquire, Ordering::Acquire)
            .map(|_| p_idx);

        let (q_a, q_b, final_result) = tq.join().unwrap();

        if bypass_seal {
            // Counterfactual: P's stale CAS is expected to SUCCEED (the bug
            // this seal closes) — q_a is back in circulation (Q gave it up
            // via the bypassed final step), so it is NOT added to the
            // held-index set here; only q_b (never re-published) is.
            let mut multiset: Vec<u32> = Vec::new();
            if let Ok(idx) = p_result {
                multiset.push(idx);
            }
            multiset.push(q_b);
            while let Some(idx) = stack.pop() {
                multiset.push(idx);
            }
            multiset.sort_unstable();
            assert_eq!(
                multiset,
                vec![0, 1],
                "free-list corrupted (lost or duplicated index) after Q's \
                 bypassed-seal final step let P's stale CAS succeed: {multiset:?}"
            );
        } else {
            assert!(
                final_result.is_err(),
                "the seal did not engage: Q's real final push at the tag \
                 ceiling returned Ok, expected Err(TagExhausted)"
            );
            assert!(
                p_result.is_err(),
                "P's stale CAS succeeded even though the tag never wrapped \
                 back to P's stale snapshot — the seal should make this \
                 structurally impossible"
            );
            // q_a is still Q's: the churn cycle's final push was refused,
            // so per push_index's `# Errors` the refused index remains the
            // caller's. q_b is still Q's: never re-published since the
            // very first pop.
            let mut multiset: Vec<u32> = Vec::new();
            if let Ok(idx) = p_result {
                multiset.push(idx);
            }
            multiset.push(q_a);
            multiset.push(q_b);
            while let Some(idx) = stack.pop() {
                multiset.push(idx);
            }
            multiset.sort_unstable();
            assert_eq!(
                multiset,
                vec![0, 1],
                "free-list conservation violated even though the seal \
                 correctly engaged: {multiset:?}"
            );
        }
    });
}

/// **Fixed:** confirms the seal engages under the exact adversarial
/// schedule replayed at the real tag width, and that P's stale CAS —
/// captured before Q's churn — is rejected.
#[test]
fn tiny_tag_seal_rejects_stale_cas_at_the_real_width() {
    run_tiny_tag_seal(false);
}

/// **Counterfactual (non-vacuousness):** bypassing the `TAG_MAX` check for
/// Q's one final step lets P's stale CAS succeed and the free-list
/// conservation check fail — proving the seal, not just the tag bump, is
/// what closes the stale-CAS double-issue hole.
#[test]
#[should_panic(expected = "corrupted")]
fn counterfactual_bypassed_seal_lets_stale_cas_double_issue() {
    run_tiny_tag_seal(true);
}
