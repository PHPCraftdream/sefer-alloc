//! R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL) — loom model-check of
//! the per-(segment, class) dirty-bit protocol
//! (`alloc_core::dirty_by_class::PerClassDirty`).
//!
//! # Scope
//!
//! This harness models the NEW protocol surface `class-aware-dirty` adds, in
//! isolation, using `loom::sync::atomic` (NOT the real
//! `HeapSlotRemote::dirty_by_class` / `AllocCore::drain_dirty_segments`).
//! Mirrors `tests/loom_dirty_publish.rs` / `tests/loom_dirty_multi_segment.rs`'s
//! established modelling discipline for this project's dirty-bitmap
//! producer/consumer race.
//!
//! **What is deliberately NOT re-modelled here:** the pointer-materialisation
//! race for the lazily-published `PerClassDirty` sidecar itself
//! (`RacyPtrCell<PerClassDirty>`'s `UNINIT -> INITIALIZING -> READY` CAS
//! protocol) — that primitive is `racy-ptr-cell`'s own crate, with its own
//! independent loom suite (`crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs`).
//! Re-verifying it here would be redundant re-verification of an
//! already-proven primitive (see `alloc_core::dirty_by_class`'s module doc,
//! "Sizing and lazy materialisation"). This harness assumes the sidecar is
//! already materialised (a plain `AtomicU64` array, no `RacyPtrCell`) and
//! focuses purely on the NEW bit-set/scan semantics layered on top: TWO
//! bitmaps set by ONE producer push (the existing per-segment bit, now
//! ADDITIONALLY a per-class bit), and a consumer whose VISIT decision reads
//! only the per-class bitmap while its DRAIN body still processes the whole
//! ring (both classes' entries).
//!
//! # The property under test
//!
//! > A producer publishes a ring entry of class C into a segment's ring, then
//! > sets BOTH the per-segment dirty bit AND the per-(segment, class=C) dirty
//! > bit (Release). A consumer searching for class C scans ONLY the
//! > per-class-C bitmap to decide whether to visit the segment; if it visits,
//! > it drains the WHOLE ring (recovering entries of every class present, not
//! > just C). INVARIANT: every entry published to the ring — regardless of
//! > which class it belongs to, and regardless of which class's search
//! > triggered the visit that drained it — is eventually recovered. No entry
//! > is permanently invisible.
//!
//! This is the model-level counterpart to
//! `tests/class_aware_dirty_routing.rs::class_a_refill_reclaims_class_b_entries_in_the_same_pass`
//! (the real-code integration test) — this file explores loom's full
//! interleaving space (bounded by `preemption_bound`) instead of ONE
//! concrete thread schedule.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release \
//!     --features alloc-core,alloc-xthread,class-aware-dirty \
//!     --test loom_class_aware_dirty
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel — matches `RING_SLOT_EMPTY`.
const RING_SLOT_EMPTY: u32 = u32::MAX;

/// Ring capacity for [`ClassAwareDirtyModel`]. `RemoteFreeRing::push`'s own
/// production full-check (`t.wrapping_sub(h) >= RING_CAP`, `remote_free_ring.rs`)
/// reads `tail` `Relaxed` and `head` `Acquire` — the SAME pairing this model
/// uses — which can (regardless of `RING_CAP`'s value: `t.wrapping_sub(h)`
/// wraps to a huge value whenever a reader observes `h` "ahead of" its own
/// stale `t` snapshot, a legal outcome under the memory model when nothing
/// separately synchronises that reader with the tail's own writer) report a
/// TRANSIENT false "full" under a genuinely concurrent drain. This is an
/// ACCEPTED, already-relied-upon property of the real ring's bounded-leak
/// contract (see `push`'s "Ring full: bounded leak" branch) — mirrored by
/// `tests/loom_remote_ring.rs`'s own push call sites, which retry in a loop
/// on `Err` rather than asserting single-attempt success (see this file's
/// `push_and_mark` call sites, which do the same). `CAP=4` (rather than a
/// razor-thin 2) is still used here — comfortably above the 2 real entries
/// this file ever pushes — purely so a genuinely exhausted ring (as opposed
/// to the transient false-positive above) never enters this model's search
/// space at all, keeping capacity questions fully out of scope: this file
/// is about the per-class dirty-bit protocol, not the ring's own push
/// contention shape (separately, already loom-verified — see
/// `crates/ring-mpsc`/`loom_remote_ring.rs`).
const RING_CAP: u32 = 4;

/// Model: ONE segment's ring (`RING_CAP` slots, comfortably holding the 2
/// entries this file's tests push), the EXISTING per-segment dirty bit, and
/// the NEW per-class dirty bitmap (one bit per class covering this one
/// modelled segment — bit 0 = "this segment dirty for class A", bit 1 =
/// "... for class B").
///
/// This mirrors the real production layout's RELATIONSHIP (per-segment bit
/// unconditionally set + per-class bit additionally set by the SAME push),
/// simplified to one segment / two classes — the same simplification
/// `MultiSegDirtyModel` in `loom_dirty_multi_segment.rs` uses for its own
/// 2-segment model.
struct ClassAwareDirtyModel {
    // Ring state (FIFO via wrapping head/tail cursors — from
    // `loom_remote_ring.rs` / `loom_dirty_publish.rs`'s established shape).
    ring_head: AtomicU32,
    ring_tail: AtomicU32,
    ring_slots: [AtomicU32; RING_CAP as usize],
    // Which class each occupied ring slot holds (index-parallel to
    // `ring_slots`; only meaningful while that slot's offset is published).
    ring_slot_class: [AtomicU32; RING_CAP as usize],
    // EXISTING per-segment dirty bit (bit 0 = this segment). Set
    // unconditionally by every push, regardless of class — unchanged by
    // this task.
    dirty_segment: AtomicU64,
    // NEW per-class dirty bitmap: bit 0 = class A dirty on this segment,
    // bit 1 = class B dirty on this segment. Set ADDITIONALLY (alongside
    // `dirty_segment`) by a push, keyed by that push's class.
    dirty_by_class: AtomicU64,
}

impl ClassAwareDirtyModel {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            ring_head: AtomicU32::new(0),
            ring_tail: AtomicU32::new(0),
            // `loom::sync::atomic::AtomicU32::new` is not `const` (unlike
            // `core::sync::atomic::AtomicU32::new`), so the `[const { .. };
            // N]` repeat-expression form used elsewhere in this crate for
            // real (non-loom) atomics is unavailable here — build the arrays
            // via `core::array::from_fn` instead.
            ring_slots: core::array::from_fn(|_| AtomicU32::new(RING_SLOT_EMPTY)),
            ring_slot_class: core::array::from_fn(|_| AtomicU32::new(0)),
            dirty_segment: AtomicU64::new(0),
            dirty_by_class: AtomicU64::new(0),
        })
    }

    /// Producer: push `(offset, class)` into the ring, THEN set BOTH dirty
    /// signals — mirrors `set_dirty_bit_for_segment`'s real production
    /// ordering (ring publish happens-before the dirty-bit `fetch_or`s).
    /// `class` is 0 or 1 (class A / class B in this 2-class model).
    fn push_and_mark(&self, offset: u32, class: u32) -> bool {
        loop {
            let t = self.ring_tail.load(Ordering::Relaxed);
            let h = self.ring_head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= RING_CAP {
                return false; // Ring full.
            }
            match self.ring_tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let idx = (t as usize) % RING_CAP as usize;
                    // Publish the offset (Release) AND its class (Relaxed —
                    // the offset's own Release store is what the consumer's
                    // Acquire load synchronises with; the class word is
                    // published as part of the same "before the dirty bits"
                    // program order, matching the real ring entry's packed
                    // (offset, class) word being a SINGLE atomic write in
                    // production — this model splits it into two words only
                    // for clarity, not because production has two racy
                    // writes).
                    self.ring_slot_class[idx].store(class, Ordering::Relaxed);
                    self.ring_slots[idx].store(offset, Ordering::Release);
                    // Existing per-segment bit: Release, unconditional.
                    self.dirty_segment.fetch_or(1, Ordering::Release);
                    // NEW per-class bit: Release, keyed by `class`.
                    self.dirty_by_class
                        .fetch_or(1u64 << class, Ordering::Release);
                    return true;
                }
                Err(_) => continue,
            }
        }
    }

    /// Consumer: search for `sought_class`. VISIT DECISION reads ONLY the
    /// per-class bit for `sought_class` (`swap` that ONE bit to 0, Acquire)
    /// — mirrors `drain_dirty_segments`'s class-scoped scan-source under
    /// `class-aware-dirty`. If the bit was set, DRAIN THE WHOLE RING
    /// (both classes' entries) — mirrors the real drain body being
    /// byte-for-byte unchanged regardless of which bitmap triggered the
    /// visit. Returns the offsets reclaimed (both classes, if present).
    ///
    /// Per this task's design (see `alloc_core::dirty_by_class`'s module
    /// doc): the per-segment `dirty_segment` bit is intentionally NOT
    /// consulted or cleared here — it remains the FALLBACK signal for
    /// non-class-aware callers/paths and is orthogonal to this scan. Real
    /// production code has separate call sites for the two scan sources
    /// (see `drain_dirty_segments`'s `scan_source` selection); this model
    /// isolates the class-aware path specifically.
    fn class_scoped_visit_and_drain(&self, sought_class: u32) -> Vec<(u32, u32)> {
        let bit = 1u64 << sought_class;
        let was_dirty = self.dirty_by_class.fetch_and(!bit, Ordering::Acquire) & bit != 0;
        if !was_dirty {
            return Vec::new();
        }
        // Full-ring drain — UNCHANGED regardless of which class triggered
        // the visit. This is the load-bearing design property: the per-class
        // bit is a VISIT HINT only, never a partial-drain filter.
        let t = self.ring_tail.load(Ordering::Acquire);
        let mut h = self.ring_head.load(Ordering::Relaxed);
        let mut reclaimed = Vec::new();
        while h != t {
            let idx = (h as usize) % RING_CAP as usize;
            let slot = &self.ring_slots[idx];
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break; // Not yet published — stop (mirrors the real ring's contract).
            }
            let class = self.ring_slot_class[idx].load(Ordering::Relaxed);
            reclaimed.push((class, off));
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.ring_head.store(h, Ordering::Release);
        reclaimed
    }

    /// COUNTERFACTUAL: the REJECTED "genuinely partial" drain — stops after
    /// reclaiming the FIRST entry matching `sought_class`, leaving any other
    /// classes' entries (even ones already published and visible) undrained.
    /// Used only by `counterfactual_partial_drain_loses_other_class_entry`
    /// below, to prove the harness is non-vacuous (a design that DID filter
    /// the drain body itself, not just the visit decision, WOULD lose
    /// entries).
    fn class_scoped_visit_and_partial_drain(&self, sought_class: u32) -> Vec<(u32, u32)> {
        let bit = 1u64 << sought_class;
        let was_dirty = self.dirty_by_class.fetch_and(!bit, Ordering::Acquire) & bit != 0;
        if !was_dirty {
            return Vec::new();
        }
        let t = self.ring_tail.load(Ordering::Acquire);
        let mut h = self.ring_head.load(Ordering::Relaxed);
        let mut reclaimed = Vec::new();
        while h != t {
            let idx = (h as usize) % RING_CAP as usize;
            let slot = &self.ring_slots[idx];
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break;
            }
            let class = self.ring_slot_class[idx].load(Ordering::Relaxed);
            // BUG: only reclaim entries matching `sought_class`; STOP at the
            // first non-matching entry instead of continuing past it (a
            // "genuinely partial" per-class drain that never re-visits the
            // skipped entry, since the cursor still advances past it below
            // in a real ring implementation -- here we model the even MORE
            // naive variant that just stops, matching this file's sibling
            // `class_aware_dirty_routing.rs::NaivePartialDrainModel`).
            if class != sought_class {
                break;
            }
            reclaimed.push((class, off));
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.ring_head.store(h, Ordering::Release);
        reclaimed
    }
}

// =========================================================================
// Positive tests: the real (full-ring-drain) protocol never loses an entry,
// regardless of which class's search triggers the visit.
// =========================================================================

/// Two producers push DIFFERENT classes (A pushes class 0, B pushes class 1)
/// into the SAME segment's ring, then join. A single class-A-triggered visit
/// must recover BOTH entries (the full-ring drain reclaims class B's entry
/// too, even though only class A's bit drove the visit decision).
#[test]
fn class_a_triggered_visit_recovers_class_b_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = ClassAwareDirtyModel::new();

        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_and_mark(10, 0 /* class A */);
        });
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_and_mark(20, 1 /* class B */);
        });
        ta.join().unwrap();
        tb.join().unwrap();

        // A search for class A triggers the ONLY visit in this test.
        let reclaimed = model.class_scoped_visit_and_drain(0);

        let found_a = reclaimed.iter().any(|&(c, o)| c == 0 && o == 10);
        let found_b = reclaimed.iter().any(|&(c, o)| c == 1 && o == 20);
        assert!(
            found_a && found_b,
            "class-A-triggered visit did not recover both entries: {reclaimed:?} \
             (want class A offset 10 AND class B offset 20 -- the drain body must \
             be full-ring regardless of which class's bit triggered the visit)"
        );
    });
}

/// Concurrent producer-vs-consumer variant (CONC-1-style, mirroring
/// `loom_dirty_multi_segment.rs`'s `concurrent_producer_consumer_eventual_
/// visibility`): the class-A-triggered visit races BOTH producers (not
/// joined first). Per the "at-least-once, bounded deferral" contract, the
/// racy concurrent visit alone may miss an entry whose push lands after the
/// visit's bit-clear; the correctness argument is that ANY missed bit
/// remains set (or gets re-set) for a LATER visit. Assert the total across
/// (concurrent visit + one guaranteed final visit of EACH class) recovers
/// both entries.
#[test]
fn concurrent_producer_consumer_eventual_visibility() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = ClassAwareDirtyModel::new();

        // `push_and_mark`'s "full" outcome mirrors `RemoteFreeRing::push`'s
        // real `Err(PushOverflow)` contract (see `RING_CAP`'s doc comment):
        // a transient false-"full" observation (stale-Relaxed-tail vs.
        // fresh-Acquire-head) is an ACCEPTED, already-modelled outcome
        // (`tests/loom_remote_ring.rs` retries in a loop on `Err`, never
        // asserts single-attempt success) -- retry here for the same reason,
        // so this test's failures are attributable to the dirty-bit protocol
        // under study, not to a single-attempt push racing a concurrent
        // drain.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || while !m_a.push_and_mark(10, 0) {});
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || while !m_b.push_and_mark(20, 1) {});

        // Consumer races both producers, searching for class A.
        let m_c = Arc::clone(&model);
        let tc = thread::spawn(move || m_c.class_scoped_visit_and_drain(0));

        let concurrent = tc.join().unwrap();
        ta.join().unwrap();
        tb.join().unwrap();

        // Guaranteed final visits (one per class) -- models the next drain
        // cycle relying on the "bit remains set until consumed" contract.
        let final_a = model.class_scoped_visit_and_drain(0);
        let final_b = model.class_scoped_visit_and_drain(1);

        let mut all = concurrent;
        all.extend(final_a);
        all.extend(final_b);

        let found_a = all.iter().any(|&(c, o)| c == 0 && o == 10);
        let found_b = all.iter().any(|&(c, o)| c == 1 && o == 20);
        assert!(
            found_a && found_b,
            "concurrent producer-vs-consumer: entries not fully recovered across \
             concurrent + final visits: {all:?} (want class A offset 10 AND \
             class B offset 20)"
        );
    });
}

/// A producer pushing class B AFTER a class-A-triggered visit has already
/// cleared class A's bit (but the ring still had room) must have its entry
/// survive to a LATER class-B-triggered visit — the lost-wakeup property,
/// applied specifically to the per-class bitmap's own bit (not just the
/// shared per-segment bit `loom_dirty_publish.rs` already covers).
#[test]
fn per_class_bit_survives_across_visits() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = ClassAwareDirtyModel::new();

        // Producer A pushes and joins first.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_and_mark(10, 0);
        });
        ta.join().unwrap();

        // First visit: class A search recovers class A's entry.
        let first = model.class_scoped_visit_and_drain(0);

        // Producer B pushes class B AFTER the first visit.
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_and_mark(20, 1);
        });
        tb.join().unwrap();

        // Second visit: class B search recovers class B's entry (its bit
        // survived independently of class A's bit having already been
        // cleared by the first visit).
        let second = model.class_scoped_visit_and_drain(1);

        let mut all = first;
        all.extend(second);
        let found_a = all.iter().any(|&(c, o)| c == 0 && o == 10);
        let found_b = all.iter().any(|&(c, o)| c == 1 && o == 20);
        assert!(
            found_a && found_b,
            "per-class bit did not survive across visits: {all:?} (want class A \
             offset 10 from the first visit AND class B offset 20 from the second)"
        );
    });
}

// =========================================================================
// Counterfactual — a GENUINELY partial per-class drain (the rejected
// alternative design) loses an entry of a DIFFERENT class than the one that
// triggered the visit.
// =========================================================================

/// Proves the harness is non-vacuous: if the drain body ITSELF were filtered
/// by class (the rejected design this task's `dirty_by_class` module doc
/// explicitly argues against), a class-A-triggered visit would lose class
/// B's entry — because the naive partial drain stops at the first
/// non-matching class instead of continuing past it (mirroring
/// `class_aware_dirty_routing.rs::NaivePartialDrainModel`'s real-code
/// counterfactual, but here exercised across loom's full interleaving space
/// instead of one fixed thread schedule).
///
/// `#[should_panic]` because loom finds the interleaving where class B is
/// pushed into slot 0 (published FIRST, ahead of class A in FIFO order) and
/// the naive partial drain, triggered by a class-A search, stops at that
/// first non-matching entry — leaving class A's OWN entry (in slot 1)
/// unreached in the SAME pass, and permanently losing visibility into class
/// B's entry from class A's search (class B's bit was already cleared by
/// this visit, so a LATER class-B search would find the bit already
/// clear -- unless something re-sets it, which nothing does here).
#[test]
#[should_panic(expected = "partial drain")]
fn counterfactual_partial_drain_loses_other_class_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = ClassAwareDirtyModel::new();

        // Producer B pushes FIRST (lands in slot 0, FIFO head).
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_and_mark(20, 1 /* class B */);
        });
        tb.join().unwrap();

        // Producer A pushes SECOND (lands in slot 1).
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_and_mark(10, 0 /* class A */);
        });
        ta.join().unwrap();

        // A class-A-triggered visit using the NAIVE partial drain: it clears
        // class A's bit (the visit decision), then walks the ring from the
        // head (slot 0 = class B) and stops immediately because slot 0's
        // class does not match the sought class (0).
        let reclaimed = model.class_scoped_visit_and_partial_drain(0);

        let found_a = reclaimed.iter().any(|&(c, o)| c == 0 && o == 10);
        let found_b = reclaimed.iter().any(|&(c, o)| c == 1 && o == 20);
        assert!(
            found_a && found_b,
            "partial drain lost an entry: reclaimed only {reclaimed:?} from a \
             class-A-triggered visit (want class A offset 10 AND class B offset \
             20) -- this is exactly the lost-wakeup hazard this implementation's \
             full-ring-drain design avoids by construction"
        );
    });
}

// =========================================================================
// R13-1 (task #271, P0 fix): the coarse-only latch closes the "sidecar OOM
// window -> later successful materialisation" visibility gap.
//
// Scenario this section models (the bug report's exact shape): producer A's
// push hits sidecar OOM (`ensure_per_class_dirty` returns `None` for THAT
// push) -- it sets ONLY the coarse per-segment bit for its entry, no
// per-class bit, and trips the coarse-only latch. Producer B pushes LATER
// and successfully materialises/uses the sidecar (models "the transient OOM
// condition cleared") -- it sets BOTH the coarse bit and its own per-class
// bit. A consumer searching for A's class must see BOTH entries in one pass
// once the latch is observed, rather than only ever finding B's entry via
// the per-class scan while A's stays invisible until a periodic full scan
// (which this isolated model does not implement at all -- proving the
// latch, not a fallback timer, is what recovers A's entry).
// =========================================================================

/// Model: a SINGLE segment's ring (mirrors `ClassAwareDirtyModel`), the
/// existing coarse per-segment dirty bit, the per-class dirty bitmap, AND the
/// NEW coarse-only latch. `push_and_mark_with_latch(class, sidecar_ok)` lets
/// a caller choose, per push, whether "the sidecar was available" (mirrors
/// `ensure_per_class_dirty` returning `Some`/`None` for that specific call) —
/// modelling the real production fact that sidecar availability is a
/// property of the ATTEMPT, not a fixed per-heap constant across the whole
/// test (the whole point of the bug being fixed is that it CAN flip from
/// unavailable to available between two pushes on the same heap).
struct LatchedDirtyModel {
    ring_head: AtomicU32,
    ring_tail: AtomicU32,
    ring_slots: [AtomicU32; RING_CAP as usize],
    ring_slot_class: [AtomicU32; RING_CAP as usize],
    dirty_segment: AtomicU64,
    dirty_by_class: AtomicU64,
    // NEW (R13-1): coarse-only latch. `0` = per-class path trusted (subject
    // to the existing materialisation check), `1` = PERMANENTLY coarse-only
    // for the rest of this model's lifetime. Modelled as `AtomicU32` (loom
    // has no `AtomicBool` in every version used across this crate's other
    // loom files; `AtomicU32` with 0/1 mirrors `loom_dirty_publish.rs`'s own
    // convention for boolean-shaped atomics) rather than reusing
    // `core::sync::atomic::AtomicBool`, since this file already builds every
    // atomic from `loom::sync::atomic` -- see the module doc's "What is
    // deliberately NOT re-modelled" section for why this file uses its own
    // small hand-rolled model rather than the real `HeapSlotRemote` type.
    coarse_only_latch: AtomicU32,
}

impl LatchedDirtyModel {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            ring_head: AtomicU32::new(0),
            ring_tail: AtomicU32::new(0),
            ring_slots: core::array::from_fn(|_| AtomicU32::new(RING_SLOT_EMPTY)),
            ring_slot_class: core::array::from_fn(|_| AtomicU32::new(0)),
            dirty_segment: AtomicU64::new(0),
            dirty_by_class: AtomicU64::new(0),
            coarse_only_latch: AtomicU32::new(0),
        })
    }

    /// Producer: push `(offset, class)`, THEN set the coarse bit
    /// unconditionally. If `sidecar_ok` is `true` (models a successful
    /// `ensure_per_class_dirty` for THIS push), additionally set the
    /// per-class bit. If `false` (models sidecar OOM for THIS push),
    /// trip the coarse-only latch instead — mirrors
    /// `set_dirty_bit_for_segment`'s `None` branch
    /// (`registry::heap_core_xthread`) storing `true` into
    /// `sidecar_oom_latch` with `Release`.
    fn push_and_mark_with_latch(&self, offset: u32, class: u32, sidecar_ok: bool) -> bool {
        loop {
            let t = self.ring_tail.load(Ordering::Relaxed);
            let h = self.ring_head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= RING_CAP {
                return false;
            }
            match self.ring_tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let idx = (t as usize) % RING_CAP as usize;
                    self.ring_slot_class[idx].store(class, Ordering::Relaxed);
                    self.ring_slots[idx].store(offset, Ordering::Release);
                    // Existing per-segment bit: Release, unconditional --
                    // matches `set_dirty_bit_for_segment`'s `dirty_segments`
                    // fetch_or, which runs regardless of sidecar outcome.
                    self.dirty_segment.fetch_or(1, Ordering::Release);
                    if sidecar_ok {
                        self.dirty_by_class
                            .fetch_or(1u64 << class, Ordering::Release);
                    } else {
                        // R13-1: sidecar OOM for this push -- trip the latch.
                        // Release: pairs with the consumer's Acquire read in
                        // `latched_visit_and_drain` below -- matches the real
                        // production producer's `Ordering::Release` store
                        // (`set_dirty_bit_for_segment`,
                        // `registry::heap_core_xthread`) byte-for-byte. R14-2
                        // (task #287): this model's Acquire consumer read was
                        // ALSO, until this task, the ONLY place the intended
                        // Acquire/Release pairing actually existed --
                        // production's `drain_dirty_segments` read this latch
                        // `Relaxed` (three independent Round 13 reviews found
                        // the divergence). A loom pass under a STRICTER
                        // ordering than production ships does not prove
                        // anything about the weaker ordering production
                        // actually used -- this model is now a byte-for-byte
                        // copy of the (fixed) production orderings, not
                        // merely an "equivalent, never weaker" approximation.
                        self.coarse_only_latch.store(1, Ordering::Release);
                    }
                    return true;
                }
                Err(_) => continue,
            }
        }
    }

    /// Consumer: mirrors `drain_dirty_segments`'s R13-1 scan-source
    /// selection. Reads the latch FIRST; if tripped, ALWAYS scans+clears the
    /// coarse bit (ignoring the per-class bitmap entirely, even if it
    /// happens to be set) and full-ring-drains on a coarse hit. If the latch
    /// is NOT tripped, falls back to the ORIGINAL class-scoped behaviour
    /// (`class_scoped_visit_and_drain`'s logic, inlined here for a
    /// self-contained model): scan the per-class bit for `sought_class` only.
    fn latched_visit_and_drain(&self, sought_class: u32) -> Vec<(u32, u32)> {
        // R14-2 (task #287): Acquire -- matches production
        // `AllocCore::drain_dirty_segments`'s (`alloc_core_small.rs`) latch
        // read exactly (promoted from `Relaxed` this same task; see
        // `push_and_mark_with_latch`'s `store` comment above for the full
        // divergence history this model now closes).
        let latched = self.coarse_only_latch.load(Ordering::Acquire) != 0;
        let was_dirty = if latched {
            // R13-1: coarse-only path -- ignore per-class bitmap entirely.
            self.dirty_segment.swap(0, Ordering::Acquire) & 1 != 0
        } else {
            let bit = 1u64 << sought_class;
            self.dirty_by_class.fetch_and(!bit, Ordering::Acquire) & bit != 0
        };
        if !was_dirty {
            return Vec::new();
        }
        // Full-ring drain, unchanged regardless of scan source (same
        // property `ClassAwareDirtyModel::class_scoped_visit_and_drain`
        // upholds).
        let t = self.ring_tail.load(Ordering::Acquire);
        let mut h = self.ring_head.load(Ordering::Relaxed);
        let mut reclaimed = Vec::new();
        while h != t {
            let idx = (h as usize) % RING_CAP as usize;
            let slot = &self.ring_slots[idx];
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break;
            }
            let class = self.ring_slot_class[idx].load(Ordering::Relaxed);
            reclaimed.push((class, off));
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.ring_head.store(h, Ordering::Release);
        reclaimed
    }

    /// COUNTERFACTUAL twin of `latched_visit_and_drain`: the REJECTED
    /// pre-R13-1 behaviour with NO latch at all -- the scan source is chosen
    /// PURELY on "is the per-class bit set for `sought_class` right now",
    /// with no memory of any past sidecar-OOM push. Used only by
    /// `counterfactual_no_latch_loses_visibility_of_oom_window_entry` below,
    /// to prove the harness is non-vacuous (the latch is doing real work,
    /// not a redundant safety margin).
    fn no_latch_visit_and_drain(&self, sought_class: u32) -> Vec<(u32, u32)> {
        let bit = 1u64 << sought_class;
        let was_dirty = self.dirty_by_class.fetch_and(!bit, Ordering::Acquire) & bit != 0;
        if !was_dirty {
            return Vec::new();
        }
        let t = self.ring_tail.load(Ordering::Acquire);
        let mut h = self.ring_head.load(Ordering::Relaxed);
        let mut reclaimed = Vec::new();
        while h != t {
            let idx = (h as usize) % RING_CAP as usize;
            let slot = &self.ring_slots[idx];
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break;
            }
            let class = self.ring_slot_class[idx].load(Ordering::Relaxed);
            reclaimed.push((class, off));
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.ring_head.store(h, Ordering::Release);
        reclaimed
    }
}

/// The central R13-1 property: producer A publishes class A's entry DURING a
/// sidecar-OOM window (coarse bit only, latch tripped), joins; producer B
/// THEN publishes class B's entry with a successful sidecar (both bits set),
/// joins. A SINGLE class-A-triggered visit (`latched_visit_and_drain`) must
/// recover BOTH entries in that ONE pass -- proving the latch redirects the
/// consumer to the coarse path (which sees everything) rather than the
/// per-class path (which would only ever see B's entry, since A's per-class
/// bit was never set).
#[test]
fn latch_makes_oom_window_entry_visible_in_one_pass() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = LatchedDirtyModel::new();

        // Producer A: sidecar OOM for this push (sidecar_ok = false) --
        // sets ONLY the coarse bit, trips the latch.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_and_mark_with_latch(10, 0 /* class A */, false);
        });
        ta.join().unwrap();

        // Producer B: sidecar available for this push (sidecar_ok = true) --
        // sets BOTH the coarse bit and its own per-class bit. Happens AFTER
        // A joins, modelling "a later producer on the same heap successfully
        // materialises the sidecar".
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_and_mark_with_latch(20, 1 /* class B */, true);
        });
        tb.join().unwrap();

        // ONE class-A-triggered visit. Per R13-1, the latch (tripped by A's
        // push) makes this visit use the coarse path regardless of B's
        // successful per-class bit -- so it must see BOTH entries now, not
        // just B's (which is all the OLD per-class-only scan would ever
        // find, since A's per-class bit was never set).
        let reclaimed = model.latched_visit_and_drain(0);

        let found_a = reclaimed.iter().any(|&(c, o)| c == 0 && o == 10);
        let found_b = reclaimed.iter().any(|&(c, o)| c == 1 && o == 20);
        assert!(
            found_a && found_b,
            "latched visit did not recover both entries in one pass: {reclaimed:?} \
             (want class A offset 10 -- published during the sidecar-OOM window -- \
             AND class B offset 20 -- published after successful materialisation) \
             -- the coarse-only latch must make the OOM-window entry visible \
             immediately, not just eventually via a periodic fallback"
        );
    });
}

/// Concurrent variant (CONC-1-style, mirroring
/// `concurrent_producer_consumer_eventual_visibility` above): producer A's
/// OOM-window push and producer B's later successful push race a concurrent
/// consumer visit, none joined first. Per the "at-least-once, bounded
/// deferral" contract, the racy concurrent visit alone may miss an entry
/// whose push (or whose latch trip) lands after the visit's read. Assert the
/// total across (concurrent visit + one guaranteed final visit) recovers
/// both entries -- proving the latch's `Acquire`/`Release` pairing is sound
/// under genuine interleaving, not just under the sequential join order the
/// test above uses.
#[test]
fn latch_concurrent_producer_consumer_eventual_visibility() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = LatchedDirtyModel::new();

        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || while !m_a.push_and_mark_with_latch(10, 0, false) {});
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || while !m_b.push_and_mark_with_latch(20, 1, true) {});

        let m_c = Arc::clone(&model);
        let tc = thread::spawn(move || m_c.latched_visit_and_drain(0));

        let concurrent = tc.join().unwrap();
        ta.join().unwrap();
        tb.join().unwrap();

        // Guaranteed final visit, now that both producers have joined and
        // the latch (if tripped) is stably visible.
        let final_visit = model.latched_visit_and_drain(1);

        let mut all = concurrent;
        all.extend(final_visit);

        let found_a = all.iter().any(|&(c, o)| c == 0 && o == 10);
        let found_b = all.iter().any(|&(c, o)| c == 1 && o == 20);
        assert!(
            found_a && found_b,
            "concurrent producer-vs-consumer under the latch: entries not fully \
             recovered across concurrent + final visits: {all:?} (want class A \
             offset 10 AND class B offset 20)"
        );
    });
}

/// COUNTERFACTUAL (`#[should_panic]`): proves this harness is non-vacuous by
/// showing the REJECTED pre-R13-1 behaviour (no latch, scan source picked
/// purely by "is the per-class bit set right now") genuinely loses
/// visibility of the OOM-window entry within one pass. Same producer
/// sequence as `latch_makes_oom_window_entry_visible_in_one_pass` (A's push
/// during the OOM window, then B's successful push), but the consumer uses
/// `no_latch_visit_and_drain` instead of `latched_visit_and_drain`: since
/// class A's per-class bit was never set (A's push never got a sidecar), a
/// class-A-triggered per-class-only scan finds `was_dirty == false` and
/// returns immediately without draining the ring at all -- so NEITHER entry
/// is recovered by this one visit (not even B's, which is a coarse
/// per-segment-bit fact this per-class-only scan does not consult).
#[test]
#[should_panic(expected = "pre-latch")]
fn counterfactual_no_latch_loses_visibility_of_oom_window_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = LatchedDirtyModel::new();

        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_and_mark_with_latch(10, 0 /* class A */, false);
        });
        ta.join().unwrap();

        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_and_mark_with_latch(20, 1 /* class B */, true);
        });
        tb.join().unwrap();

        // Pre-R13-1 behaviour: no latch consulted at all.
        let reclaimed = model.no_latch_visit_and_drain(0);

        let found_a = reclaimed.iter().any(|&(c, o)| c == 0 && o == 10);
        let found_b = reclaimed.iter().any(|&(c, o)| c == 1 && o == 20);
        assert!(
            found_a && found_b,
            "pre-latch behaviour lost visibility: reclaimed only {reclaimed:?} from \
             a class-A-triggered visit (want class A offset 10 AND class B offset \
             20) -- without the coarse-only latch, class A's OOM-window entry is \
             invisible to a per-class-only scan (its per-class bit was never set), \
             which is EXACTLY the bug R13-1 fixes"
        );
    });
}

// =========================================================================
// R14-2 (task #287): the specific three-way OOM-trip / successful-publish /
// consumer-drain interleaving R13's reviews flagged as not explicitly
// covered. `latch_concurrent_producer_consumer_eventual_visibility` above
// already races all three threads, but only asserts the WEAKER
// "concurrent-visit-plus-one-guaranteed-final-visit" total. This test
// isolates the interleaving R13-1 (the latch) exists specifically to close
// -- producer A's sidecar-OOM trip racing producer B's successful
// materialisation racing a consumer's SINGLE visit -- and additionally
// asserts the STRONGER "immediate visibility" property the doc comments
// claim: once a consumer visit observes the latch as tripped (Acquire, per
// R14-2's fix), that ONE visit must recover BOTH entries, not just
// eventually across several visits. Because loom explores the full
// interleaving space under `preemption_bound`, this includes schedules
// where the consumer's Acquire load of the latch races A's Release store
// and B's per-class-bit Release store landing in every relative order.
#[test]
fn latch_trip_and_successful_publish_race_single_consumer_visit() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = LatchedDirtyModel::new();

        // Producer A: sidecar OOM for this push -- trips the latch. NOT
        // joined before B or the consumer start: the latch trip itself races
        // both.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || while !m_a.push_and_mark_with_latch(10, 0, false) {});

        // Producer B: successful sidecar materialisation, racing A's trip.
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || while !m_b.push_and_mark_with_latch(20, 1, true) {});

        ta.join().unwrap();
        tb.join().unwrap();

        // ONE consumer visit, AFTER both producers have joined -- so the
        // latch trip and both pushes are already stably visible in program
        // order; what loom explores here is every legal interleaving of the
        // memory operations WITHIN that visit racing the two producers'
        // stores under the model's bounded preemption search, together with
        // `latch_concurrent_producer_consumer_eventual_visibility` above
        // (which additionally races the visit itself against the producers,
        // unjoined). A single class-A-triggered visit, observing the
        // Release-published latch via its Acquire read, must recover BOTH
        // entries in this ONE pass -- the strong "immediate visibility"
        // contract `HeapSlotRemote::sidecar_oom_latch`'s doc comment claims,
        // not merely "eventually across several visits".
        let reclaimed = model.latched_visit_and_drain(0);

        let found_a = reclaimed.iter().any(|&(c, o)| c == 0 && o == 10);
        let found_b = reclaimed.iter().any(|&(c, o)| c == 1 && o == 20);
        assert!(
            found_a && found_b,
            "single post-join visit did not recover both entries: {reclaimed:?} \
             (want class A offset 10 -- published during the sidecar-OOM window \
             -- AND class B offset 20 -- published via successful materialisation) \
             -- this is the exact OOM-trip/successful-publish/consumer race R13-1's \
             latch exists to close, and R14-2's Acquire/Release pairing is what \
             makes the ONE-PASS (not just eventual) guarantee provable"
        );
    });
}

// =========================================================================
// R15-4 (task #306, review finding P2-4): `latch_trip_and_successful_publish_
// race_single_consumer_visit` above joins BOTH producers before the single
// consumer visit -- `join()` itself gives a full happens-before between
// everything the joined thread did and code that runs AFTER the join, so
// that test's assertion passes under ANY memory ordering on
// `coarse_only_latch` (even fully `Relaxed`), regardless of whether the
// Acquire/Release pairing this section's doc comment credits is doing any
// work at all. It genuinely covers the LATCH's *logic* (coarse-only routing
// recovers both entries in one pass), but not the *ordering* property R14-2
// claims to make provable.
//
// The two tests below close that gap using the classic message-passing
// litmus-test shape: a consumer thread SPINS (no `join()`) on
// `coarse_only_latch`'s own Acquire load until it observes the trip, then
// asserts that producer A's PRIOR (program-order) writes -- the ring-slot
// publish and the per-segment dirty bit, both stored Release before the
// latch's own Release store -- are unconditionally visible at that point.
// If the latch's ordering were weakened to `Relaxed`, loom's bounded
// interleaving search must find a schedule where the spin observes the trip
// but a prior write is not yet visible -- which is exactly what
// `counterfactual_relaxed_latch_spin_may_observe_trip_before_prior_writes`
// below proves, using a second model built with `Relaxed` throughout,
// mirroring this file's established permanent-counterfactual-method pattern
// (`no_latch_visit_and_drain` / `class_scoped_visit_and_partial_drain`)
// rather than a temporary hand-edit-and-revert.
// =========================================================================

/// Bound on both this test's producer push-retry loop and the consumer's
/// latch-spin loop. Per `loom_remote_ring.rs`'s `MODEL_RETRY_BOUND` doc
/// comment (same rationale, copied here rather than shared since these are
/// independent single-file loom models by this file's own established
/// convention): an UNBOUNDED retry-until-success or spin-until-observed loop
/// makes loom's checker abort with "Model exceeded maximum number of
/// branches" -- loom explores every iteration as a new branch regardless of
/// `yield_now()` calls, so a small bound keeps the model tractable while
/// still covering the property under test (does the FIRST successful push
/// get its prior writes made visible to a spinning consumer under every
/// interleaving loom explores). `RING_CAP=4` means the producer's own push
/// never actually needs to retry in this model (only one push happens), so
/// this bound is pure headroom against loom's own scheduling exploration,
/// not a realistic contention bound.
const SPIN_BOUND: u32 = 8;

/// Consumer SPINS (no `join()`) on `coarse_only_latch`'s Acquire load until
/// it observes producer A's trip, then asserts A's prior program-order
/// writes (ring-slot publish, per-segment dirty bit) are visible without
/// ever calling `latched_visit_and_drain`'s full drain (which would mutate
/// state this test does not need to touch) -- isolating exactly the
/// Acquire/Release pairing under test, nothing else.
#[test]
fn latch_acquire_pairing_makes_prior_writes_visible_without_join() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = LatchedDirtyModel::new();

        // Producer A: sidecar OOM -- publishes its ring entry and the coarse
        // per-segment bit (both Release), THEN trips the latch (Release).
        // NOT joined before the consumer spins below. Bounded retry (see
        // `SPIN_BOUND`'s doc comment) -- a single push into an empty
        // `RING_CAP=4` ring never actually needs more than one attempt.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            for _ in 0..SPIN_BOUND {
                if m_a.push_and_mark_with_latch(10, 0, false) {
                    return;
                }
            }
            panic!("producer A did not land its push within SPIN_BOUND attempts");
        });

        // Consumer: spin on the latch's OWN Acquire load -- no join(). The
        // instant this observes a nonzero latch, Acquire/Release must make
        // every write A performed in program order BEFORE its Release store
        // to `coarse_only_latch` visible here. Bounded (see `SPIN_BOUND`):
        // if the latch is never observed tripped within the bound under some
        // interleaving loom explores, that is a distinct liveness question
        // out of scope for this ordering-focused test -- treated as a skip,
        // not a failure, so this test stays focused purely on "IF the trip
        // is observed, are the prior writes visible".
        let m_c = Arc::clone(&model);
        let tc = thread::spawn(move || {
            for _ in 0..SPIN_BOUND {
                if m_c.coarse_only_latch.load(Ordering::Acquire) != 0 {
                    // Per-segment dirty bit: A's `fetch_or(1, Release)` ran
                    // strictly before A's `coarse_only_latch.store(1,
                    // Release)` in program order -- must be visible now.
                    let dirty = m_c.dirty_segment.load(Ordering::Acquire) & 1 != 0;
                    assert!(
                        dirty,
                        "latch observed tripped but the per-segment dirty bit A set \
                         BEFORE tripping the latch is not yet visible -- this is the \
                         Acquire/Release pairing R14-2 claims to establish"
                    );
                    // Ring publish: A's `ring_slots[idx].store(10, Release)`
                    // also ran strictly before the latch trip -- the tail
                    // must have advanced past the head, and the published
                    // slot must not read back as empty.
                    let t = m_c.ring_tail.load(Ordering::Acquire);
                    let h = m_c.ring_head.load(Ordering::Relaxed);
                    assert!(
                        t != h,
                        "latch observed tripped but A's ring-tail advance is not yet \
                         visible -- ring publish should happen-before the latch trip"
                    );
                    let idx = (h as usize) % RING_CAP as usize;
                    let off = m_c.ring_slots[idx].load(Ordering::Acquire);
                    assert_ne!(
                        off, RING_SLOT_EMPTY,
                        "latch observed tripped but A's published ring offset is not \
                         yet visible at the head slot"
                    );
                    return;
                }
            }
        });

        ta.join().unwrap();
        tc.join().unwrap();
    });
}

/// COUNTERFACTUAL (`#[should_panic]`): proves
/// `latch_acquire_pairing_makes_prior_writes_visible_without_join` above is
/// non-vacuous. `RelaxedLatchModel` is a byte-for-byte copy of
/// `LatchedDirtyModel::push_and_mark_with_latch`'s publish path EXCEPT the
/// latch's store/load pair is `Relaxed` instead of `Release`/`Acquire` --
/// the exact weakening this task's review finding warned the joined-producer
/// test could not detect. Under `Relaxed`, nothing forbids the consumer's
/// spin from observing the latch trip before observing A's prior per-segment
/// dirty-bit write (both are independent `Relaxed` operations with no
/// ordering relationship), so loom's bounded interleaving search must find a
/// schedule where the assertion fires.
#[test]
#[should_panic(expected = "not yet visible")]
fn counterfactual_relaxed_latch_spin_may_observe_trip_before_prior_writes() {
    struct RelaxedLatchModel {
        ring_head: AtomicU32,
        ring_tail: AtomicU32,
        ring_slots: [AtomicU32; RING_CAP as usize],
        dirty_segment: AtomicU64,
        coarse_only_latch: AtomicU32,
    }

    impl RelaxedLatchModel {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                ring_head: AtomicU32::new(0),
                ring_tail: AtomicU32::new(0),
                ring_slots: core::array::from_fn(|_| AtomicU32::new(RING_SLOT_EMPTY)),
                dirty_segment: AtomicU64::new(0),
                coarse_only_latch: AtomicU32::new(0),
            })
        }

        /// Same shape as `push_and_mark_with_latch(.., sidecar_ok: false)`,
        /// but with the latch's store weakened to `Relaxed` -- the
        /// counterfactual ordering this test proves is unsound.
        fn push_and_trip_relaxed(&self, offset: u32) -> bool {
            loop {
                let t = self.ring_tail.load(Ordering::Relaxed);
                let h = self.ring_head.load(Ordering::Acquire);
                if t.wrapping_sub(h) >= RING_CAP {
                    return false;
                }
                match self.ring_tail.compare_exchange_weak(
                    t,
                    t.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        let idx = (t as usize) % RING_CAP as usize;
                        self.ring_slots[idx].store(offset, Ordering::Release);
                        self.dirty_segment.fetch_or(1, Ordering::Release);
                        // BUG (counterfactual only): `Relaxed` instead of
                        // `Release` -- no longer forbids this store from
                        // being observed ahead of the two writes above.
                        self.coarse_only_latch.store(1, Ordering::Relaxed);
                        return true;
                    }
                    Err(_) => continue,
                }
            }
        }
    }

    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = RelaxedLatchModel::new();

        // Bounded (see `SPIN_BOUND`'s doc comment above, same rationale).
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            for _ in 0..SPIN_BOUND {
                if m_a.push_and_trip_relaxed(10) {
                    return;
                }
            }
            panic!("producer A did not land its push within SPIN_BOUND attempts");
        });

        let m_c = Arc::clone(&model);
        let tc = thread::spawn(move || {
            for _ in 0..SPIN_BOUND {
                // BUG (counterfactual only): `Relaxed` instead of `Acquire`
                // -- matches the weakened store above.
                if m_c.coarse_only_latch.load(Ordering::Relaxed) != 0 {
                    let dirty = m_c.dirty_segment.load(Ordering::Acquire) & 1 != 0;
                    assert!(dirty, "counterfactual: dirty bit not yet visible");
                    let t = m_c.ring_tail.load(Ordering::Acquire);
                    let h = m_c.ring_head.load(Ordering::Relaxed);
                    assert!(t != h, "counterfactual: ring tail not yet visible");
                    let idx = (h as usize) % RING_CAP as usize;
                    let off = m_c.ring_slots[idx].load(Ordering::Acquire);
                    assert_ne!(
                        off, RING_SLOT_EMPTY,
                        "counterfactual: offset not yet visible"
                    );
                    return;
                }
            }
        });

        ta.join().unwrap();
        tc.join().unwrap();
    });
}
