//! loom model-check of the **A1 `deferred_large` push/drain Treiber stack**
//! protocol (task #141 — closing the loom-debt documented in
//! `src/registry/tagged_ptr.rs` and the #138 audit report).
//!
//! # Scope — what loom covers
//!
//! This harness models `push_large_deferred_free` / `drain_large_deferred_free`
//! (`src/alloc_core/deferred_large/push.rs` / `drain.rs`) in isolation using
//! `loom::sync::atomic` (NOT the real functions, which use
//! `core::sync::atomic` and operate on live `SegmentHeader`s). It reproduces
//! the EXACT protocol shape:
//!
//! - `head: AtomicPtr<u8>` (the owner's stack head), intrusive per-node
//!   `deferred_next: AtomicU64` link (repurposed as the stack link word,
//!   exactly as the real code does).
//! - Two sentinels: `ABANDONED_TAIL` (`u64::MAX`, "not on any stack" — the
//!   node's rest state) and `DEFERRED_LARGE_TAIL` (`u64::MAX - 1`, "on THIS
//!   stack, no next" — the bottom-of-stack marker), matching
//!   `segment_header.rs` / `deferred_large/tail.rs`.
//! - The **double-push guard**: `push` claims the node's link word via
//!   `compare_exchange(ABANDONED_TAIL → next_link)` BEFORE contesting `head`.
//!   A losing claimant (same `base` already linked) is a no-op. This is the
//!   post-A1 hardening that fixes the UAF/double-unmap regression
//!   (`regression_xthread_large_free_no_leak`) on the real allocator.
//! - `push`'s retry-on-lost-head-CAS: a plain `store` (not a fresh
//!   claim-CAS) retargets the already-owned link word to the new head.
//! - `drain`'s single-consumer pop loop: CAS `head` from `cur` to the node's
//!   `next` (translated back from `DEFERRED_LARGE_TAIL` to "null"), retrying
//!   on a concurrent pusher.
//!
//! # Properties asserted
//!
//! (a) **No lost nodes** — every DISTINCT-base node successfully pushed is
//!     extracted by the drain EXACTLY once.
//! (b) **Double-push guard holds** — pushing the SAME base twice (racing
//!     "double free") does NOT cause that base to be extracted twice; the
//!     second push is a no-op (mirrors the real double-free-degrades-safely
//!     contract).
//! (c) **No panic / no deadlock** in the model.
//!
//! # This harness FOUND a real production leak (task #141 → fixed in #143)
//!
//! While writing this harness (faithfully transcribing `push.rs`'s retry
//! loop), loom found — and a 2M-trial plain-`std::thread` reproduction on
//! real `core::sync::atomic` CONFIRMED — that the ORIGINAL
//! `push_large_deferred_free` had the double-push claim-CAS INSIDE the
//! `head`-CAS retry loop. On the second iteration (after losing the `head`
//! CAS to a concurrent pusher of a DIFFERENT base), `next_atomic` no longer
//! read `ABANDONED_TAIL` (the first iteration's claim already moved it), so
//! the claim CAS always failed and the function `return`ed early via the
//! guard bail-out — WITHOUT ever winning `head`: the node was silently
//! dropped from the stack (an A1-class permanent leak), whenever >=2
//! concurrent pushers of DISTINCT bases raced on `head` more than once. This
//! was not caught before because `tests/regression_xthread_large_free_no_leak.rs`
//! uses a single remote thread pushing sequentially, never real concurrent
//! contention on `head`.
//!
//! The fix (task #143) hoists the claim CAS to run EXACTLY ONCE, before the
//! `head`-CAS loop; the loop then only re-attempts the `head` CAS with a
//! plain link-store retarget. This harness models the FIXED protocol, and
//! `distinct_pushes_all_drained_exactly_once` below now asserts the
//! no-lost-node invariant as a normal green test — if it ever starts failing,
//! either the #143 fix regressed in `push.rs` or this harness drifted from
//! it.
//!
//! We do NOT model the OS-level reclaim (unmap/recycle) — only the
//! stack-protocol push/drain extraction, per the task's scope.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features "alloc-core alloc-xthread" --test loom_deferred_large
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;
use std::ptr::null_mut;

/// Mirrors `segment_header::ABANDONED_TAIL` — "not currently linked into any
/// stack" (a node's rest state).
const ABANDONED_TAIL: u64 = u64::MAX;
/// Mirrors `deferred_large::tail::DEFERRED_LARGE_TAIL` — "linked into THIS
/// stack, no next" (bottom-of-stack marker), deliberately distinct from
/// `ABANDONED_TAIL`.
const DEFERRED_LARGE_TAIL: u64 = u64::MAX - 1;

/// A model "segment" node: the intrusive Treiber-stack link. In the real code
/// this is `SegmentHeader::deferred_next`; loom needs its own tiny
/// heap-allocated node since loom's model doesn't have real segment memory.
struct Node {
    deferred_next: AtomicU64,
    /// Stable identity for the node (mirrors the segment `base` pointer's
    /// role as identity — loom nodes are heap boxes, so we tag them with an
    /// id instead of comparing addresses across `expose_provenance` cycles,
    /// keeping the model's identity check independent of loom's allocator).
    id: u64,
}

/// The model stack: `head` mirrors `&AtomicPtr<u8>`. We store `*mut Node`
/// directly (loom's own allocator tracks these boxes; each `Node` is
/// `Box::leak`ed for the duration of one `model.check` iteration and
/// reclaimed at the end via `drop_all`, mirroring the real code's "the OWNER
/// reclaims once drained" discipline without modelling the OS unmap itself).
struct Stack {
    head: AtomicPtr<Node>,
}

impl Stack {
    fn new() -> Arc<Self> {
        Arc::new(Stack {
            head: AtomicPtr::new(null_mut()),
        })
    }

    /// Mirrors `push_large_deferred_free`: claim-CAS guard on the node's own
    /// link word, then contest `head`, retrying with a plain store on a lost
    /// head CAS (the claim already secured exclusive ownership of the link
    /// word for the rest of this call).
    fn push(&self, node: *mut Node) {
        // SAFETY (model): `node` is a live loom-allocated box for the
        // duration of the check; only ever dereferenced by this stack's
        // single consumer (drain) or by a push racing on the SAME node's own
        // link word (guarded below).
        let next_atomic = unsafe { &(*node).deferred_next };
        let mut cur = self.head.load(Ordering::Acquire);
        let next_link = if cur.is_null() {
            DEFERRED_LARGE_TAIL
        } else {
            cur as u64
        };
        // Double-push guard: claim the link word from ABANDONED_TAIL EXACTLY
        // ONCE, before the head-CAS loop (task #143 fix — mirrors the
        // corrected `push_large_deferred_free`). Re-running the claim inside
        // the loop would always fail on retry and silently drop the push.
        if next_atomic
            .compare_exchange(
                ABANDONED_TAIL,
                next_link,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_err()
        {
            // Already linked (a concurrent/earlier push of the SAME node won
            // the claim) — sound no-op, matches the real guard.
            return;
        }
        loop {
            match self
                .head
                .compare_exchange(cur, node, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(actual) => {
                    // Lost the head CAS to a different node. We exclusively
                    // own this node's link word (claim CAS above succeeded),
                    // so a plain store retargets it — mirrors the real code.
                    // We do NOT re-run the claim CAS (the #143 leak).
                    let retry_link = if actual.is_null() {
                        DEFERRED_LARGE_TAIL
                    } else {
                        actual as u64
                    };
                    next_atomic.store(retry_link, Ordering::Release);
                    cur = actual;
                }
            }
        }
    }

    /// Mirrors `drain_large_deferred_free`: single-consumer pop loop, CAS
    /// `head` to the popped node's `next` (translating `DEFERRED_LARGE_TAIL`
    /// back to null), retrying on a concurrent pusher.
    fn drain<F: FnMut(*mut Node)>(&self, mut reclaim: F) {
        loop {
            let cur = self.head.load(Ordering::Acquire);
            if cur.is_null() {
                return;
            }
            // SAFETY (model): single consumer; `cur` is still linked (not yet
            // popped), so its link word is stable until our CAS below.
            let next_link = unsafe { (*cur).deferred_next.load(Ordering::Acquire) };
            let next = if next_link == DEFERRED_LARGE_TAIL {
                null_mut()
            } else {
                next_link as *mut Node
            };
            match self
                .head
                .compare_exchange(cur, next, Ordering::Acquire, Ordering::Relaxed)
            {
                Ok(_) => reclaim(cur),
                Err(_) => continue, // a concurrent push raced us — retry
            }
        }
    }
}

// ============================================================================
// (a) No lost nodes: 2 producers push DISTINCT bases; 1 drain after join.
// This models the FIXED protocol (task #143): the claim CAS runs once,
// before the head-CAS loop, so a lost head CAS retries WITHOUT dropping the
// push. Asserts every distinct-base push survives to be drained exactly once
// (the invariant the original in-loop claim CAS violated — see the module doc).
// ============================================================================

#[test]
fn distinct_pushes_all_drained_exactly_once() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let stack = Stack::new();

        let node_a = Box::into_raw(Box::new(Node {
            deferred_next: AtomicU64::new(ABANDONED_TAIL),
            id: 1,
        }));
        let node_b = Box::into_raw(Box::new(Node {
            deferred_next: AtomicU64::new(ABANDONED_TAIL),
            id: 2,
        }));

        let s1 = Arc::clone(&stack);
        let na = node_a as usize;
        let t1 = thread::spawn(move || s1.push(na as *mut Node));

        let s2 = Arc::clone(&stack);
        let nb = node_b as usize;
        let t2 = thread::spawn(move || s2.push(nb as *mut Node));

        t1.join().unwrap();
        t2.join().unwrap();

        let mut seen_a = 0u32;
        let mut seen_b = 0u32;
        stack.drain(|n| {
            // SAFETY: node came from this stack's own drain; reclaimed
            // exactly once per successful pop, matching the real
            // "owner reclaims after CAS-pop" discipline.
            let id = unsafe { (*n).id };
            if id == 1 {
                seen_a += 1;
            } else if id == 2 {
                seen_b += 1;
            }
            unsafe { drop(Box::from_raw(n)) };
        });

        assert_eq!(seen_a, 1, "node A lost or duplicated: seen {seen_a} times");
        assert_eq!(seen_b, 1, "node B lost or duplicated: seen {seen_b} times");
    });
}

// ============================================================================
// (b) Double-push guard: 2 threads push the SAME base ("double free" race).
// ============================================================================

#[test]
fn double_push_same_base_extracted_once() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let stack = Stack::new();

        let node = Box::into_raw(Box::new(Node {
            deferred_next: AtomicU64::new(ABANDONED_TAIL),
            id: 42,
        }));
        let n_usize = node as usize;

        // Two racing "double free" pushes of the SAME node.
        let s1 = Arc::clone(&stack);
        let t1 = thread::spawn(move || s1.push(n_usize as *mut Node));

        let s2 = Arc::clone(&stack);
        let t2 = thread::spawn(move || s2.push(n_usize as *mut Node));

        t1.join().unwrap();
        t2.join().unwrap();

        let mut seen = 0u32;
        stack.drain(|n| {
            let id = unsafe { (*n).id };
            if id == 42 {
                seen += 1;
            }
            unsafe { drop(Box::from_raw(n)) };
        });

        // GUARD: exactly one of the two racing pushes wins the claim CAS; the
        // other is a no-op. The node is extracted exactly once — never twice
        // (which would be a double-unmap on the real allocator) and never
        // zero (the guard must not silently drop the ONLY push either, in the
        // case where the two pushes don't race — but here both DO attempt to
        // push, so at least one must win).
        assert_eq!(
            seen, 1,
            "double-push guard violated: base extracted {seen} times (want exactly 1)"
        );
    });
}

// ============================================================================
// (c) Counterfactual: WITHOUT the double-push guard, the same base can be
// linked twice (self-loop / duplicate extraction) — proves the harness is
// non-vacuous.
// ============================================================================

/// The BROKEN push: identical to `Stack::push` but WITHOUT the claim-CAS
/// guard — it always proceeds to link the node, regardless of whether it is
/// already linked. Mirrors the pre-hardening bug described in `push.rs`'s
/// doc comment (a self-loop / double-link that a drain can traverse twice).
fn push_broken_no_guard(stack: &Stack, node: *mut Node) {
    let next_atomic = unsafe { &(*node).deferred_next };
    let mut cur = stack.head.load(Ordering::Acquire);
    loop {
        let next_link = if cur.is_null() {
            DEFERRED_LARGE_TAIL
        } else {
            cur as u64
        };
        // BUG: no claim-CAS — unconditionally overwrite the link word even
        // if this node is already linked into the stack (e.g. by a
        // concurrent racing push of the same node).
        next_atomic.store(next_link, Ordering::Release);
        match stack
            .head
            .compare_exchange(cur, node, Ordering::Release, Ordering::Relaxed)
        {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
}

#[test]
#[should_panic(expected = "want exactly 1")]
fn counterfactual_no_guard_double_extracts_or_corrupts() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let stack = Stack::new();

        let node = Box::into_raw(Box::new(Node {
            deferred_next: AtomicU64::new(ABANDONED_TAIL),
            id: 42,
        }));
        let n_usize = node as usize;

        let stack1 = Arc::clone(&stack);
        let t1 = thread::spawn(move || push_broken_no_guard(&stack1, n_usize as *mut Node));

        let stack2 = Arc::clone(&stack);
        let t2 = thread::spawn(move || push_broken_no_guard(&stack2, n_usize as *mut Node));

        t1.join().unwrap();
        t2.join().unwrap();

        // A racing double-push without the guard can self-loop the node
        // (head -> node -> node -> ...) or corrupt the chain such that drain
        // never terminates within a bounded number of pops, OR extracts the
        // node more than once if the two link-word stores race such that
        // head briefly points at the node twice via two different paths in
        // some interleavings modeled with a bounded pop budget below.
        let mut seen = 0u32;
        let mut pops = 0u32;
        loop {
            let cur = stack.head.load(Ordering::Acquire);
            if cur.is_null() || pops >= 4 {
                break;
            }
            pops += 1;
            let next_link = unsafe { (*cur).deferred_next.load(Ordering::Acquire) };
            let next = if next_link == DEFERRED_LARGE_TAIL {
                null_mut()
            } else {
                next_link as *mut Node
            };
            if stack
                .head
                .compare_exchange(cur, next, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                let id = unsafe { (*cur).id };
                if id == 42 {
                    seen += 1;
                }
            }
        }
        // With the guard this would be exactly 1; the broken version lets
        // loom find an interleaving producing a self-loop, which this bounded
        // drain (pops capped at 4) surfaces as `seen != 1` (either the
        // self-loop makes the SAME node poppable more than once within the
        // 4-pop budget, or the corrupted chain makes it effectively
        // unreachable/lost). Either way the invariant fails.
        assert_eq!(seen, 1, "want exactly 1");

        // Clean up remaining model allocation (best-effort; loom's leak
        // checker only tracks loom-native allocations, not these Box::leak
        // model nodes across `#[should_panic]`, since the panic aborts the
        // closure before cleanup — acceptable for a should_panic model test).
    });
}
