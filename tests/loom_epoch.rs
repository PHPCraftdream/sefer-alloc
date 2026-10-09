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
//! > A reader using a valid minted handle and the seqlock protocol
//! > (load gen → load value → re-load gen; accept only if both gens match AND
//! > equal the handle's expected generation) must not resolve a value belonging
//! > to a different generation.
//!
//! This is a handwritten shadow model using Loom atomics, not actual
//! `EpochRegion` or `AtomicSlot` coverage or an implementation refinement proof.
//! It abstracts `install` as a Release value store with generation unchanged,
//! and `try_evict_at` as a generation CAS before tombstoning the value; a failed
//! CAS leaves the value untouched.
//!
//! Readers use a valid generation-0 handle established by installation before
//! spawning. Peeking an arbitrary current generation is not handle minting:
//! between the CAS and tombstone swap, the new generation can coexist with the
//! old value, but no handle for that new generation has yet been minted.
//! Reinstallation here occurs only after eviction completes. Free-list/queue
//! reuse, saturation/retirement, and reclamation are outside this model.
//!
//! # Reclamation is outside this model
//!
//! No crossbeam epoch guards, pointer dereferences, `guard.defer_destroy`, or
//! epoch-advance lifetime behavior are exercised. Ordering checks here do not
//! establish reclamation correctness, actual-type safety, or liveness.
//!
//! # How to run
//!
//! loom is a `cfg(loom)` dev-dependency, so this file is only compiled under
//! `--cfg loom`:
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features "experimental tagged-index-stack/loom" --test loom_epoch -- --test-threads=1
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel for "vacant" (the null pointer in the real `AtomicSlot`). A reader
/// that loads this resolves to `None` (I2 — tombstone).
const VACANT: usize = 0;

/// Abstract publication state inspired by a single `AtomicSlot<T>`:
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

    /// Abstract `AtomicSlot::install`: store the value (Release). Generation
    /// is unchanged — a handle minted now carries the current generation.
    fn install(&self, value: usize) {
        self.value.store(value, Ordering::Release);
    }

    /// Abstract `AtomicSlot::try_evict_at`: strong generation CAS (AcqRel on
    /// success, Acquire on failure), then swap value to VACANT (AcqRel) only
    /// on success. Failure leaves the value untouched. No outcome/reclamation
    /// accounting is modelled. u64 saturation is explicitly omitted: these
    /// bounded scenarios only transition 0 → 1 → 2.
    fn evict(&self, expected_gen: u64) {
        if self
            .generation
            .compare_exchange(
                expected_gen,
                expected_gen + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            self.value.swap(VACANT, Ordering::AcqRel);
        }
    }

    /// Abstract reader with **seqlock validation**:
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

/// Bounded shadow check: 1 writer + 1 reader over `(generation, value)`.
/// Install generation 0 before spawning to establish a valid handle. The
/// reader retains that handle while the writer evicts, reinstalls at generation
/// 1, and evicts again. Every resolved value must match the handle's tag.
/// No synchronization is added after spawning; the reader races the writer.
///
/// `preemption_bound = 3` limits exploration. The missing-g2 negative control
/// below is intended to expose a torn read within that bound; its outcome is
/// not established by static reasoning or compilation.
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

        // Establish a valid generation-0 handle before the reader can run.
        state.install(make_value(0));
        let reader = thread::spawn(move || {
            for _ in 0..2 {
                // Retain the minted handle; a generation peek is not minting.
                let expected = 0;
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

        // Reuse only after eviction completes, never between CAS and swap.
        state.evict(0);
        state.install(make_value(1));
        state.evict(1);

        reader.join().expect("reader panicked");
    });
}

// =========================================================================
// Counterfactual — `read_with` WITHOUT the seqlock g2 re-check.
// =========================================================================

/// Negative control for `publication_protocol_never_yields_a_mismatched_value`:
/// the same valid generation-0 handle and 1-writer/1-reader scenario, but with
/// `read_with_no_recheck` deliberately omitting g2 validation.
///
/// Intended witness: the reader loads g1 = 0; the writer completes eviction
/// (CAS 0 → 1, swap to VACANT) and installs `make_value(1) = 1007`; the reader
/// loads 1007 and returns it for expected generation 0, whose tag is 7.
/// The assertion should panic with "torn read". No post-spawn handoff orders
/// the reader after eviction or reinstall.
///
/// `#[should_panic]` requires the intended assertion signature. Whether Loom
/// finds this witness with `preemption_bound = 3` requires execution; if it
/// does not panic, the negative-control test fails.
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

        // Establish a valid generation-0 handle before the reader can run.
        state.install(make_value(0));
        let reader = thread::spawn(move || {
            for _ in 0..2 {
                let expected = 0;
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

        // Reuse only after eviction completes, never between CAS and swap.
        state.evict(0);
        state.install(make_value(1));
        state.evict(1);

        reader.join().expect("reader panicked");
    });
}
