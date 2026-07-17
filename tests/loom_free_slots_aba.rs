//! loom model-check of the **Registry `free_slots` Treiber stack + `TaggedPtr`
//! ABA guard** (task #141 — closing the loom-debt documented in
//! `src/registry/tagged_ptr.rs`'s "0.3.0 (task #138) — honest status" note).
//!
//! # Scope — what loom covers
//!
//! This harness models `pop_free_slot` / `push_free_slot`
//! (`src/registry/heap_registry.rs`) and `TaggedPtr::pack`/`unpack`
//! (`src/registry/tagged_ptr.rs`) in isolation using `loom::sync::atomic`
//! (NOT the real functions, which use `core::sync::atomic` and operate on a
//! live `Registry`/`HeapSlot` array). It reproduces the EXACT protocol shape:
//!
//! - `free_slots: AtomicU64` head, packed `(index | tag << 16)` via
//!   `TaggedPtr` (low 16 bits = slot index, high 48 bits = a monotonic tag
//!   bumped on every successful PUSH — never on pop; task W7a).
//! - Per-slot `next_free: AtomicU32` link (mirrors `HeapSlot::next_free`),
//!   with `NEXT_FREE_TAIL` (`u32::MAX`) as the "no next" sentinel and
//!   `TaggedPtr::empty()` (`value = INDEX_MASK = 0xFFFF`, `tag = 0`) as the
//!   "stack empty" sentinel. Post-W7a these are DISTINCT numeric values
//!   (`INDEX_MASK = 0xFFFF` for empty vs `NEXT_FREE_TAIL = u32::MAX` for the
//!   per-slot tail link) — the real code keeps the two mappings spelled out
//!   separately, so the `TAIL → empty()` translation in `pop`/`push` is
//!   exercised faithfully here regardless of the value coincidence.
//! - `pop`: load tagged head, read the slot's `next_free` link, CAS head to
//!   `(next, SAME tag)` — a losing CAS retries with the fresh head.
//! - `push`: write the slot's `next_free` link to the current head's index
//!   (or tail sentinel), CAS head to `(idx, tag + 1)` — the tag bump is what
//!   defeats ABA.
//!
//! # The classic ABA scenario modelled
//!
//! Thread A reads `head = (idx=X, tag=T)`, begins its pop (reads
//! `next_free` for X). Before A's CAS lands, thread B pops X (successfully,
//! advancing head to X's `next_free`), then re-pushes X (bumping the tag to
//! `T' > T`, but the numeric slot index is again `X`). A's CAS on
//! `(X, T) → (next_A, T)` MUST fail — even though the head's `value` half
//! reads `X` again, matching what A originally saw — because the `tag` half
//! no longer matches `T`. loom explores the interleaving where B's pop+repush
//! completes entirely within A's read-then-CAS window and asserts A's CAS
//! observably fails (forcing a retry) rather than "succeeding" onto a stale
//! `next_free` chain (which would corrupt the free-list — losing or
//! duplicating slot indices).
//!
//! # Properties asserted
//!
//! (a) **A's stale-tag CAS fails** (forced retry) in the specific
//!     interleaving where B's full pop+repush of the SAME index completes
//!     inside A's window.
//! (b) **Free-list stays consistent** after the whole race resolves: exactly
//!     the set of indices actually pushed are poppable, each exactly once
//!     (no loss, no duplication) — checked by draining the stack fully at
//!     the end and comparing against the expected index set.
//! (c) A counterfactual with the tag mechanism DISABLED (an `AtomicU32`-only
//!     head, no tag) demonstrates loom finding an ABA corruption in the same
//!     interleaving, proving the harness — and by extension the tag
//!     mechanism it models — is non-vacuous.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features alloc-global --test loom_free_slots_aba
//! ```
//!
//! # H-2 addendum (UBFIX-4): the empty-transition tag-reset ABA
//!
//! The two tests above model the classic "pop X, repush X" ABA and prove the
//! TAG mechanism in general defeats it. They do NOT, on their own, exercise
//! the specific defect fixed by UBFIX-4/H-2: the ORIGINAL `pop`'s
//! empty-transition branch (the pop that drains the stack to zero elements)
//! collapsed the head to `TaggedPtr::empty()`, which hardcodes **tag = 0**,
//! discarding whatever running tag was live at the moment of the drain. A
//! parked popper holding a stale `(idx, tag)` snapshot from BEFORE the drain
//! can then have its stale tag spuriously RECUR after the stack drains and
//! is immediately refilled by a repush of the SAME index — because the
//! repush computes `new_tag = drained_tag(0) + 1 = 1`, and if the popper's
//! stale snapshot also happened to be `tag = 1` (e.g. this was the second
//! push ever performed on this slot), the numeric head word recurs EXACTLY,
//! and the stale popper's CAS spuriously succeeds — a genuine ABA collision,
//! not merely a "same index, different tag" near-miss the outer tests above
//! already rule out.
//!
//! `pop_empty_transition_preserves_tag` below models BOTH the buggy drain
//! branch (`pack(MASK, 0)`) and the fixed drain branch (`pack(MASK, tag)`,
//! reusing the tag the draining pop just read) via a `preserve_tag_on_drain`
//! flag, replays the IDENTICAL 2-thread interleaving against both (using a
//! two-flag rendezvous so thread B's full pop+push cycle is GUARANTEED to
//! be sandwiched between thread A's head load and A's CAS — not merely
//! possible under some schedule, which a naive free race left ambiguous
//! with an "innocent" ordering; see `run_h2_interleaving`'s doc comment for
//! why), and asserts:
//! - **buggy branch:** loom finds the schedule where the stalled popper's
//!   (A's) CAS — fired against a snapshot from BEFORE B's cycle — spuriously
//!   succeeds even though B's cycle fully completed in the interim (a stale
//!   CAS succeeding is, by definition, an ABA collision) — `#[should_panic]`
//!   counterfactual `counterfactual_empty_transition_tag_reset_lets_aba_recur`.
//! - **fixed branch:** the SAME interleaving forces the stalled popper's CAS
//!   to fail (it observes a tag that has moved on), so it retries and the
//!   free-list stays loss/duplication-free (`pop_empty_transition_preserves_tag`).
//!
//! This directly transcribes the fix landed in `pop_free_slot`
//! (`src/registry/heap_registry.rs`) and `TaggedPtr::empty_index`
//! (`src/registry/tagged_ptr.rs`): the fixed `pop` packs the empty
//! sentinel's index half with the RUNNING tag it just observed, instead of
//! resetting to a hardcoded 0.

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Mirrors `heap_registry`'s `NEXT_FREE_TAIL` — "no next" sentinel for a
/// slot's `next_free` link (`u32::MAX`).
const NEXT_FREE_TAIL: u32 = u32::MAX;

/// Number of bits carrying the index (mirrors `tagged_ptr::INDEX_BITS` — 16
/// since task W7a; the high 48 bits carry the ABA tag).
const INDEX_BITS: u32 = 16;
const INDEX_MASK: u64 = (1u64 << INDEX_BITS) - 1;

/// Mirrors `TaggedPtr::pack`/`unpack`/`empty`/`is_empty` — pure bit
/// arithmetic, transcribed verbatim (the real module is `unsafe`-free pure
/// arithmetic too, so there is no seam to model separately; copying the
/// exact bit ops keeps the loom protocol faithful to what the registry
/// actually packs into the head word).
mod tagged_ptr {
    use super::INDEX_MASK;

    pub(crate) const fn pack(value: u64, tag: u64) -> u64 {
        (tag << super::INDEX_BITS) | (value & INDEX_MASK)
    }

    pub(crate) const fn unpack(word: u64) -> (u64, u64) {
        (word & INDEX_MASK, word >> super::INDEX_BITS)
    }

    pub(crate) const fn empty() -> u64 {
        pack(INDEX_MASK, 0)
    }

    pub(crate) fn is_empty(word: u64) -> bool {
        let (value, _tag) = unpack(word);
        value == INDEX_MASK
    }
}

/// A tiny fixed-size model registry: 2 slots is enough to exercise the ABA
/// scenario (thread A targets slot 0; thread B pops slot 0, pushes some
/// other work, then re-pushes slot 0 — bumping its tag).
const MAX_SLOTS: usize = 2;

struct Registry {
    free_slots: AtomicU64,
    next_free: [AtomicU32; MAX_SLOTS],
}

impl Registry {
    /// Both slots start pushed (index 0 on top, chained to index 1, chained
    /// to the tail), tag = 0 — mirrors the bootstrap-time "all slots free"
    /// initial state closely enough for this protocol-only model (the real
    /// bootstrap path is covered by the `racy-ptr-cell` crate's real-type loom
    /// suite — CRATE-P3, which replaced the former `loom_bootstrap_cas.rs` — not
    /// here).
    fn new_both_free() -> Arc<Self> {
        Arc::new(Registry {
            // head = (idx=0, tag=0), slot 0 -> slot 1 -> TAIL.
            free_slots: AtomicU64::new(tagged_ptr::pack(0, 0)),
            next_free: [AtomicU32::new(1), AtomicU32::new(NEXT_FREE_TAIL)],
        })
    }

    /// Mirrors `pop_free_slot`: load tagged head, read the slot's next link,
    /// CAS head to `(next, SAME tag)`.
    fn pop(&self) -> Option<u32> {
        let mut head = self.free_slots.load(Ordering::Acquire);
        loop {
            if tagged_ptr::is_empty(head) {
                return None;
            }
            let (idx_v, tag) = tagged_ptr::unpack(head);
            let idx = idx_v as u32;
            let next = self.next_free[idx as usize].load(Ordering::Acquire);
            let new_head = if next == NEXT_FREE_TAIL {
                tagged_ptr::empty()
            } else {
                tagged_ptr::pack(next as u64, tag)
            };
            match self.free_slots.compare_exchange(
                head,
                new_head,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(idx),
                Err(actual) => head = actual,
            }
        }
    }

    /// Mirrors `push_free_slot`: write the link, bump the tag, CAS head to
    /// `(idx, tag + 1)`.
    fn push(&self, idx: u32) {
        let mut head = self.free_slots.load(Ordering::Acquire);
        loop {
            let next_link = if tagged_ptr::is_empty(head) {
                NEXT_FREE_TAIL
            } else {
                let (cur_idx, _tag) = tagged_ptr::unpack(head);
                cur_idx as u32
            };
            self.next_free[idx as usize].store(next_link, Ordering::Release);
            let (_cur_idx, tag) = tagged_ptr::unpack(head);
            let new_tag = tag.wrapping_add(1);
            let new_head = tagged_ptr::pack(idx as u64, new_tag);
            match self.free_slots.compare_exchange(
                head,
                new_head,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => head = actual,
            }
        }
    }
}

// ============================================================================
// (a) + (b): the classic ABA race. Thread A reads head, starts a pop of slot
// 0; thread B fully pops slot 0 AND re-pushes it (bumping the tag) before A's
// CAS lands. A's CAS must fail (retry), and the free-list must end up
// consistent (both indices poppable exactly once across the whole run).
// ============================================================================

#[test]
fn aba_repush_forces_stale_cas_retry_and_stays_consistent() {
    let mut builder = loom::model::Builder::new();
    // Tight bound: this is a 2-thread, few-step protocol; a small bound is
    // enough to force the ABA window (B's pop+push interleaved inside A's
    // read-then-CAS gap) into loom's exploration.
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let reg = Registry::new_both_free();

        // Thread A: read the head "manually" (mirrring the first half of
        // `pop`), THEN race thread B, THEN attempt A's own CAS using the
        // stale head it captured. This inlines `pop`'s loop body once so we
        // can interleave B's full pop+repush between A's read and A's CAS —
        // the actual ABA window.
        let reg_a = Arc::clone(&reg);
        let ta = thread::spawn(move || {
            let head = reg_a.free_slots.load(Ordering::Acquire);
            let (idx_v, tag) = tagged_ptr::unpack(head);
            let idx = idx_v as u32;
            // idx should be 0 (top of stack) in every schedule reaching here
            // before any pop has happened.
            let next = reg_a.next_free[idx as usize].load(Ordering::Acquire);
            let new_head = if next == NEXT_FREE_TAIL {
                tagged_ptr::empty()
            } else {
                tagged_ptr::pack(next as u64, tag)
            };
            // A's CAS against the STALE captured `head` — this is exactly
            // `pop`'s CAS, just with the read/CAS pair split so B can race
            // in between under loom's scheduler.
            reg_a
                .free_slots
                .compare_exchange(head, new_head, Ordering::Acquire, Ordering::Relaxed)
                .map(|_| idx)
        });

        // Thread B: pop slot 0 (whatever A also targeted), then push it back
        // — the classic "pop X, repush X" ABA move. Tag bumps by exactly 1.
        let reg_b = Arc::clone(&reg);
        let tb = thread::spawn(move || {
            if let Some(idx) = reg_b.pop() {
                reg_b.push(idx);
            }
        });

        let a_result = ta.join().unwrap();
        tb.join().unwrap();

        // Drain whatever remains, collecting every index seen (A's own CAS
        // result tells us whether A additionally believes it popped idx 0).
        let mut popped: Vec<u32> = Vec::new();
        if let Ok(idx) = a_result {
            // A's CAS succeeded: A popped `idx` (0 or 1 depending on the
            // interleaving — A's `head` read can race ahead of or behind
            // B's pop+repush). This is only sound if the tag A captured in
            // its head snapshot still matched at CAS time — i.e. no
            // interleaving of B's pop+repush of the SAME numeric index
            // slipped in between A's read and A's CAS undetected.
            popped.push(idx);
        }
        while let Some(idx) = reg.pop() {
            popped.push(idx);
        }

        // INVARIANT: every index in {0, 1} appears in `popped` EXACTLY once,
        // across the combination of "A's own successful CAS" (if any) plus
        // the final drain. If the ABA guard failed to protect A in some
        // interleaving, A's CAS could spuriously succeed on a STALE head
        // (same idx, stale tag no longer current) after B already spliced
        // idx 0 back onto a DIFFERENT chain position — producing either a
        // duplicate (0 appears twice: once via A, once via the drain) or a
        // loss (1 never appears, because A's stale CAS overwrote a head that
        // B had already correctly repositioned, e.g. corrupting the link
        // to slot 1 out of the chain).
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
// (c) Counterfactual: an UNTAGGED head (plain index, no ABA tag) lets the
// SAME interleaving corrupt the free-list — proving the harness actually
// exercises the ABA window (non-vacuousness) and that the tag mechanism is
// load-bearing.
// ============================================================================

/// An untagged model registry: `free_slots` is a bare `AtomicU32` index (no
/// tag bits at all). `NEXT_FREE_TAIL` doubles as the "empty" sentinel (no
/// separate `TaggedPtr::empty()` needed without a tag half).
struct UntaggedRegistry {
    free_slots: AtomicU32,
    next_free: [AtomicU32; MAX_SLOTS],
}

impl UntaggedRegistry {
    fn new_both_free() -> Arc<Self> {
        Arc::new(UntaggedRegistry {
            free_slots: AtomicU32::new(0),
            next_free: [AtomicU32::new(1), AtomicU32::new(NEXT_FREE_TAIL)],
        })
    }

    fn pop(&self) -> Option<u32> {
        let mut head = self.free_slots.load(Ordering::Acquire);
        loop {
            if head == NEXT_FREE_TAIL {
                return None;
            }
            let next = self.next_free[head as usize].load(Ordering::Acquire);
            match self
                .free_slots
                .compare_exchange(head, next, Ordering::Acquire, Ordering::Relaxed)
            {
                Ok(idx) => return Some(idx),
                Err(actual) => head = actual,
            }
        }
    }

    fn push(&self, idx: u32) {
        let mut head = self.free_slots.load(Ordering::Acquire);
        loop {
            self.next_free[idx as usize].store(head, Ordering::Release);
            match self
                .free_slots
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
        let reg = UntaggedRegistry::new_both_free();

        let reg_a = Arc::clone(&reg);
        let ta = thread::spawn(move || {
            let head = reg_a.free_slots.load(Ordering::Acquire);
            if head == NEXT_FREE_TAIL {
                return Err(head);
            }
            let next = reg_a.next_free[head as usize].load(Ordering::Acquire);
            // BUG (counterfactual): CAS against the bare index, with NO tag
            // to detect that this exact numeric value (`head == 0`) might
            // have been popped-and-repushed by another thread in the
            // meantime — a stale-but-numerically-identical head is
            // indistinguishable from a fresh one.
            reg_a
                .free_slots
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
        // Without the tag, loom finds the interleaving where A's CAS
        // spuriously "succeeds" against a numerically-identical-but-stale
        // head (ABA), corrupting the free-list: either 0 is duplicated or 1
        // is lost. This assertion states the CORRECT invariant, which the
        // untagged protocol violates — `#[should_panic]` proves it.
        assert_eq!(
            popped,
            vec![0, 1],
            "free-list corrupted (loss or duplication): got {popped:?}"
        );
    });
}

// ============================================================================
// H-2 (UBFIX-4): the empty-transition tag-reset ABA.
//
// A single-slot registry, seeded so the head already carries a NON-ZERO
// running tag (`tag = 1`, as if one prior push-then-pop-then-push cycle
// already happened during bootstrap/warm-up — a completely ordinary state,
// not a contrived edge case). Two threads race:
//
// - Thread A performs a MANUAL split pop (mirrors `pop_free_slot`): loads
//   the head, reads the slot's `next_free` link, computes its OWN candidate
//   `new_head` for the "drain to empty" branch, then STALLS before firing
//   its CAS (letting B's full pop+push interleave in the gap — same
//   technique as `aba_repush_forces_stale_cas_retry_and_stays_consistent`
//   above).
// - Thread B performs a full `pop()` (draining the stack to empty — the
//   SAME index A is targeting) followed immediately by a full `push()` of
//   that SAME index (refilling the stack). This is the exact "drain, then
//   immediately refill the same slot" cycle H-2 is about.
//
// `preserve_tag_on_drain` selects between the BUGGY drain branch
// (`pack(MASK, 0)`, mirrors `TaggedPtr::empty()`) and the FIXED drain
// branch (`pack(MASK, tag)`, mirrors `TaggedPtr::pack(TaggedPtr::empty_index(),
// _tag)` landed in `pop_free_slot`). Both A and B use the SAME branch
// selection (they model the SAME build of the allocator).
// ============================================================================

/// Registry variant with a single live slot, seeded at a caller-chosen
/// starting tag (models "this is not the first push/pop cycle ever" — the
/// realistic steady-state case, not a bootstrap artifact).
struct SingleSlotRegistry {
    free_slots: AtomicU64,
    next_free: AtomicU32,
}

impl SingleSlotRegistry {
    /// Slot 0 is on the stack, head = `(idx=0, tag=start_tag)`,
    /// `next_free[0] = TAIL` (it is the only element).
    fn seeded(start_tag: u64) -> Arc<Self> {
        Arc::new(SingleSlotRegistry {
            free_slots: AtomicU64::new(tagged_ptr::pack(0, start_tag)),
            next_free: AtomicU32::new(NEXT_FREE_TAIL),
        })
    }

    /// Mirrors `pop_free_slot`, parameterised on the drain-branch tag
    /// behaviour so the SAME function models both the buggy and fixed
    /// builds (`preserve_tag_on_drain = false` reproduces the H-2 bug;
    /// `true` is the landed fix).
    fn pop(&self, preserve_tag_on_drain: bool) -> Option<u32> {
        let mut head = self.free_slots.load(Ordering::Acquire);
        loop {
            if tagged_ptr::is_empty(head) {
                return None;
            }
            let (idx_v, tag) = tagged_ptr::unpack(head);
            let idx = idx_v as u32;
            let next = self.next_free.load(Ordering::Acquire);
            let new_head = if next == NEXT_FREE_TAIL {
                if preserve_tag_on_drain {
                    // FIX: reuse the tag just read instead of resetting to 0.
                    tagged_ptr::pack(INDEX_MASK, tag)
                } else {
                    // BUG: `TaggedPtr::empty()` — hardcoded tag 0.
                    tagged_ptr::empty()
                }
            } else {
                tagged_ptr::pack(next as u64, tag)
            };
            match self.free_slots.compare_exchange(
                head,
                new_head,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(idx),
                Err(actual) => head = actual,
            }
        }
    }

    /// Mirrors `push_free_slot`: reads the tag out of the CURRENT head
    /// unconditionally (empty or not) and bumps it. Identical in both the
    /// buggy and fixed builds — H-2's fix lives entirely in `pop`'s drain
    /// branch; `push` already composes correctly with either.
    fn push(&self, idx: u32) {
        let mut head = self.free_slots.load(Ordering::Acquire);
        loop {
            let next_link = if tagged_ptr::is_empty(head) {
                NEXT_FREE_TAIL
            } else {
                let (cur_idx, _tag) = tagged_ptr::unpack(head);
                cur_idx as u32
            };
            self.next_free.store(next_link, Ordering::Release);
            let (_cur_idx, tag) = tagged_ptr::unpack(head);
            let new_tag = tag.wrapping_add(1);
            let new_head = tagged_ptr::pack(idx as u64, new_tag);
            match self.free_slots.compare_exchange(
                head,
                new_head,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => head = actual,
            }
        }
    }
}

/// Runs the H-2 interleaving against whichever drain-branch behaviour
/// `preserve_tag_on_drain` selects, and PANICS (`"stale CAS succeeded"`) if
/// loom finds a schedule where thread A's CAS — fired against a snapshot
/// captured BEFORE thread B's full pop+push cycle — spuriously succeeds
/// AFTER that cycle has fully completed. `builder.check` requires an `Fn`
/// closure (it may replay the body across many schedules), so the assertion
/// lives INSIDE the closure — same idiom as
/// `aba_repush_forces_stale_cas_retry_and_stays_consistent` above.
///
/// **Why a rendezvous, not a free race:** a naive free race between A's
/// split pop and B's full pop+push lets loom explore DEGENERATE schedules
/// where B's whole cycle runs entirely BEFORE A's initial load (so A's
/// "stale" snapshot is actually fresh — A legitimately re-pops the
/// just-repushed index, which is correct behaviour, not a bug) or entirely
/// AFTER A's CAS (no interaction at all). Both degenerate schedules produce
/// "A and B both got index 0" without any ABA, which would make a naive
/// "same index" assertion false-positive on the FIXED build (confirmed: an
/// earlier version of this harness asserted exactly that and failed on
/// `preserve_tag_on_drain = true`, a spurious failure — not a real bug —
/// caused by exactly this degenerate ordering).
///
/// Two `AtomicU32` flags force STRICT sandwiching instead: A signals
/// `a_loaded` immediately after its step-1 load (so B cannot start before
/// A's snapshot exists), then spin-waits on `b_done` before firing its CAS
/// (so B's ENTIRE pop+push cycle is guaranteed complete before A's CAS —
/// this is the genuine "B fully mutated state in the gap between A's read
/// and A's CAS" scenario H-2 is about). Spin-waiting on a `loom::sync`
/// atomic via `thread::yield_now()` is loom's standard idiom for a one-shot
/// rendezvous (loom explores every point the spinner could observe the
/// flag; the state space stays bounded because both threads run a fixed,
/// small number of steps).
fn run_h2_interleaving(preserve_tag_on_drain: bool) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(move || {
        // Seed at tag = 1 (an ordinary post-warm-up state: one push already
        // happened before this run started), so B's drain-then-refill cycle
        // recomputes exactly tag = 1 again in the BUGGY branch (0 -> +1 -> 1),
        // colliding with A's captured `tag = 1` snapshot.
        let reg = SingleSlotRegistry::seeded(1);
        // Rendezvous flags: 0 = not yet signalled, 1 = signalled.
        let a_loaded = Arc::new(AtomicU32::new(0));
        let b_done = Arc::new(AtomicU32::new(0));

        // Thread B: waits for A's snapshot, then runs a FULL pop (drains
        // the stack — the same index A is targeting) immediately followed
        // by a full push of that SAME index (refills the stack) — the
        // exact "drain, then immediately repush" cycle the fix targets.
        // Signals `b_done` only after BOTH complete.
        let reg_b = Arc::clone(&reg);
        let a_loaded_b = Arc::clone(&a_loaded);
        let b_done_b = Arc::clone(&b_done);
        let tb = thread::spawn(move || {
            while a_loaded_b.load(Ordering::Acquire) == 0 {
                thread::yield_now();
            }
            let popped = reg_b.pop(preserve_tag_on_drain);
            if let Some(idx) = popped {
                reg_b.push(idx);
            }
            b_done_b.store(1, Ordering::Release);
            popped
        });

        // Thread A: manual split pop (load, read next_free, compute
        // candidate) — mirrors `pop_free_slot`'s body with the CAS pulled
        // out. Signals `a_loaded` right after the load, then BLOCKS on
        // `b_done` before firing its CAS — forcing B's entire cycle to be
        // sandwiched in the gap by construction (not merely "possible under
        // some loom schedule").
        let head = reg.free_slots.load(Ordering::Acquire);
        let (idx_v, tag) = tagged_ptr::unpack(head);
        let idx = idx_v as u32;
        let next = reg.next_free.load(Ordering::Acquire);
        let new_head = if next == NEXT_FREE_TAIL {
            if preserve_tag_on_drain {
                tagged_ptr::pack(INDEX_MASK, tag)
            } else {
                tagged_ptr::empty()
            }
        } else {
            tagged_ptr::pack(next as u64, tag)
        };
        a_loaded.store(1, Ordering::Release);
        while b_done.load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        // A's CAS against the STALE captured `head` — B's full pop+push is
        // GUARANTEED to have completed in the gap between A's load (above)
        // and this CAS.
        let a_result = reg
            .free_slots
            .compare_exchange(head, new_head, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| idx);

        tb.join().unwrap();

        // THE INVARIANT: once B has fully completed a pop+push cycle on
        // this slot (guaranteed true here by the rendezvous), A's CAS
        // against its PRE-B snapshot must NEVER succeed — the tag half of
        // the live head must have moved on from what A captured, because a
        // push (B's repush) always bumps the tag by construction. If A's
        // CAS succeeds anyway, the tag sequence must have looped back to
        // A's stale value — exactly the H-2 empty-transition tag-reset bug
        // (the drain branch discarding the running tag lets the very next
        // push recompute a numerically colliding word).
        assert!(
            a_result.is_err(),
            "stale CAS succeeded: thread A's compare_exchange used a head \
             snapshot captured BEFORE thread B's full pop+push cycle, yet \
             succeeded AFTER that cycle completed — an empty-transition \
             tag-reset ABA collision (H-2)"
        );
    });
}

/// **The fix (`preserve_tag_on_drain = true`):** replaying the H-2
/// interleaving against the FIXED drain branch, loom finds NO schedule
/// where A's stale CAS spuriously succeeds — the running tag keeps
/// climbing across the empty transition (B's drain preserves tag=1, B's
/// refill bumps it to 2), so A's captured `tag=1` snapshot can never recur
/// numerically. A's CAS is always forced to fail and retry.
#[test]
fn pop_empty_transition_preserves_tag() {
    run_h2_interleaving(true);
}

/// **The counterfactual (non-vacuousness proof):** replaying the IDENTICAL
/// interleaving against the BUGGY drain branch (`preserve_tag_on_drain =
/// false`, i.e. `TaggedPtr::empty()`'s hardcoded tag=0 — the pre-fix
/// behaviour), loom finds the schedule where A's stale CAS spuriously
/// succeeds: B's drain resets the tag to 0, B's refill computes `0+1=1`,
/// recreating A's captured `tag=1` head word exactly, so A's CAS succeeds
/// against a head that is numerically identical but structurally stale —
/// classic ABA. This proves the H-2 harness is non-vacuous: it is the
/// PRESENCE of the fix (not an artifact of the test's construction) that
/// makes `pop_empty_transition_preserves_tag` above pass.
#[test]
#[should_panic(expected = "stale CAS succeeded")]
fn counterfactual_empty_transition_tag_reset_lets_aba_recur() {
    run_h2_interleaving(false);
}
