//! loom model-check of the **publication protocol** for the epoch tier
//! (Phase 3b-II).
//!
//! # Scope — what loom covers and what it does NOT
//!
//! loom and `crossbeam-epoch` do NOT compose: loom replaces the global
//! allocator and the atomics with its own mock, so `crossbeam_epoch::Atomic`
//! and `epoch::pin` cannot run inside a `loom::model`. This harness therefore
//! models the **publication protocol** in isolation — the seqlock-style
//! ordering between a `generation` counter and a `value` — using
//! `loom::sync::atomic` (NOT crossbeam). It asserts the core safety property:
//!
//! > A reader using the seqlock protocol (load gen → load value → re-load gen;
//! > accept only if both gens match AND equal the expected generation) NEVER
//! > resolves a value belonging to a different generation.
//!
//! This mirrors `AtomicSlot::read_with` exactly. The writer mirrors
//! `AtomicSlot::install` (write value, generation unchanged) and
//! `AtomicSlot::evict` (swap value to a tombstone, then bump generation).
//!
//! # What rests on miri, NOT loom
//!
//! The **reclamation correctness** (the `guard.defer_destroy` / epoch-advance
//! lifetime proof in `src/concurrent/hand.rs`) is NOT modelled here. That
//! rests on the `crossbeam-epoch` crate's correctness plus `miri`, which
//! verifies our `unsafe` dereferences against real epoch guards. loom models
//! ordering; miri models lifetime/aliasing. (See the final report: miri cannot
//! run the epoch tests because crossbeam-epoch 0.9.18's global collector is
//! itself not miri-clean — an upstream limitation, not our code.)
//!
//! # How to run
//!
//! loom is a `cfg(loom)` dev-dependency, so this file is only compiled under
//! `--cfg loom`:
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features experimental --test loom_epoch
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel for "vacant" (the null pointer in the real `AtomicSlot`). A reader
/// that loads this resolves to `None` (I2 — tombstone).
const VACANT: usize = 0;

/// The shared publication state, mirroring a single `AtomicSlot<T>`:
/// a generation counter and a value word. We use `AtomicUsize` for the value
/// (not a raw pointer) because loom models *ordering*, not freeing — the value
/// is a stand-in for the "pointee contents" a real reader would observe.
/// Reclamation is LEAKED under loom (a value is never freed), which is fine:
/// we are checking the generation/value ordering protocol, not the reclamation
/// lifetime.
struct PubState {
    generation: AtomicU64,
    value: AtomicUsize,
}

impl PubState {
    fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            value: AtomicUsize::new(VACANT),
        }
    }

    /// Mirror of `AtomicSlot::install`: store the value (Release). Generation
    /// is unchanged — a handle minted now carries the current generation.
    fn install(&self, value: usize) {
        self.value.store(value, Ordering::Release);
    }

    /// Mirror of `AtomicSlot::evict`: swap value to VACANT (AcqRel), then bump
    /// generation (Release). Returns the generation the slot will have AFTER
    /// eviction (for the writer to record).
    fn evict(&self) -> u64 {
        self.value.swap(VACANT, Ordering::AcqRel);
        let g = self.generation.load(Ordering::Acquire);
        // Saturation omitted in the model (loom explores few steps).
        self.generation.store(g + 1, Ordering::Release);
        g
    }

    /// Mirror of `AtomicSlot::read_with` with the **seqlock validation**:
    /// load gen (g1) → load value → re-load gen (g2); accept only if
    /// `g1 == expected_gen && g1 == g2`. Returns the resolved value or `None`.
    fn read_with(&self, expected_gen: u64) -> Option<usize> {
        let g1 = self.generation.load(Ordering::Acquire);
        if g1 != expected_gen {
            return None;
        }
        let v = self.value.load(Ordering::Acquire);
        let g2 = self.generation.load(Ordering::Acquire);
        if g2 != g1 {
            return None;
        }
        if v == VACANT {
            return None;
        }
        Some(v)
    }

    /// COUNTERFACTUAL: `read_with` WITHOUT the seqlock re-check (g2). Accepts
    /// the value based on `g1 == expected_gen` alone — between the g1 load and
    /// the value load the writer can evict (bumping generation) AND install a
    /// new value at the next generation, so the reader's value load returns a
    /// value belonging to a different generation than `expected_gen`. Used only
    /// by the `counterfactual_no_recheck_yields_torn_read` test below.
    fn read_with_no_recheck(&self, expected_gen: u64) -> Option<usize> {
        let g1 = self.generation.load(Ordering::Acquire);
        if g1 != expected_gen {
            return None;
        }
        let v = self.value.load(Ordering::Acquire);
        // BUG: no re-check of g2. A writer racing evict → install at gen+1
        // between the g1 load and this value load makes us return a value
        // belonging to gen+1 while the caller still believes it is at
        // `expected_gen` — a torn read the seqlock re-check exists to prevent.
        if v == VACANT {
            return None;
        }
        Some(v)
    }
}

/// loom model-check: 1 writer + 1 reader over a single `(generation, value)`
/// publication. The writer churns install → evict cycles (publishing a tagged
/// value, then tombstoning it and bumping the generation), modelling the real
/// `insert`/`remove` churn across a generation boundary. The reader probes with
/// the seqlock protocol. The assertion: a reader NEVER resolves a value to a
/// generation it does not belong to — every resolved value equals the tag the
/// writer published at the reader's observed generation.
///
/// **Bounded exploration** (`preemption_bound = 3`) keeps the check to a few
/// seconds while still covering every interleaving with up to 3 preemptions —
/// enough to expose the torn read this protocol must prevent (verified:
/// removing the seqlock re-check in `read_with` makes loom fail here).
/// Unbounded exploration of this model is combinatorially explosive and
/// impractical for the dev loop. A single reader suffices: the hazard is
/// reader-vs-writer, not reader-vs-reader.
#[test]
fn publication_protocol_never_yields_a_mismatched_value() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let state = Arc::new(PubState::new());
        let r_state = Arc::clone(&state);

        // The writer publishes tagged values. The tag encodes the generation
        // the value was published at, so a coherent reader can verify the pair
        // matches: `value = gen * STEP + WRITER_TAG`, so a torn read (resolving
        // a value from a different generation) is detectable.
        const STEP: usize = 1000;
        const WRITER_TAG: usize = 7;
        let make_value = |gen: u64| usize::try_from(gen).unwrap_or(0) * STEP + WRITER_TAG;

        let reader = thread::spawn(move || {
            for _ in 0..2 {
                // Peek the generation the reader will pass as `expected_gen`
                // (mirrors a real handle minted at some generation), then read.
                let expected = r_state.generation.load(Ordering::Acquire);
                if let Some(v) = r_state.read_with(expected) {
                    // A resolved value must equal the tag for `expected`; a torn
                    // read would surface a value from a different generation.
                    assert_eq!(
                        v,
                        make_value(expected),
                        "torn read: resolved a value belonging to a different generation"
                    );
                }
            }
        });

        // Writer (main loom thread): churn install → evict across a generation
        // boundary (gen 0 → 1 → 2), the minimum to exhibit install/evict/
        // reinstall while the reader is mid-read.
        for target_gen in 0..2_u64 {
            state.install(make_value(target_gen));
            state.evict();
        }

        reader.join().expect("reader panicked");
    });
}

// =========================================================================
// Counterfactual — `read_with` WITHOUT the seqlock g2 re-check.
// =========================================================================

/// COUNTERFACTUAL for `publication_protocol_never_yields_a_mismatched_value`:
/// proves the harness is non-vacuous by running the SAME 1-writer/1-reader
/// thread structure against a DELIBERATELY BROKEN reader (`read_with_no_recheck`)
/// that accepts a value based on `g1 == expected_gen` alone, WITHOUT re-loading
/// `generation` to confirm no evict-and-reinstall happened between the g1 load
/// and the value load.
///
/// This directly backs the file's existing doc-comment claim (line 118 of the
/// positive test: "removing the seqlock re-check in `read_with` makes loom fail
/// here") — the claim was previously asserted only in prose; this test makes it
/// an executable regression.
///
/// The race loom finds: (a) reader loads `expected = generation = 0`; (b) reader
/// loads `g1 = 0`, passes the check; (c) writer evicts gen 0 (→ gen 1, value =
/// VACANT) then installs `make_value(1) = 1007` (value = 1007, gen still 1);
/// (d) reader loads `value = 1007`; (e) WITHOUT the g2 re-check the reader
/// returns `Some(1007)`, which mismatches `make_value(expected) = make_value(0)
/// = 7` — the assertion fires.
///
/// `#[should_panic]` because loom explores all interleavings with
/// `preemption_bound = 3` and FINDS the one where the broken reader surfaces a
/// torn read. If this passes (does not panic), the counterfactual is vacuous
/// and the harness is broken.
#[test]
#[should_panic(expected = "torn read")]
fn counterfactual_no_recheck_yields_torn_read() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let state = Arc::new(PubState::new());
        let r_state = Arc::clone(&state);

        const STEP: usize = 1000;
        const WRITER_TAG: usize = 7;
        let make_value = |gen: u64| usize::try_from(gen).unwrap_or(0) * STEP + WRITER_TAG;

        let reader = thread::spawn(move || {
            for _ in 0..2 {
                let expected = r_state.generation.load(Ordering::Acquire);
                // BROKEN reader: no g2 re-check.
                if let Some(v) = r_state.read_with_no_recheck(expected) {
                    assert_eq!(
                        v,
                        make_value(expected),
                        "torn read: resolved a value belonging to a different generation"
                    );
                }
            }
        });

        for target_gen in 0..2_u64 {
            state.install(make_value(target_gen));
            state.evict();
        }

        reader.join().expect("reader panicked");
    });
}
