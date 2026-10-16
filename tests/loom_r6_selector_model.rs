//! R6-T53 extension of the R11 selector shadow model (`loom_r11_registry_claim.rs`
//! stays unchanged as an evidence-pinned model): recycle with the production
//! swap-and-spill, `count`, `bump_count` index reservation, versioned
//! saturation certification and the `candidate < count` guard. All
//! interleaved atoms and threads are Loom's.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;

const FREE: u8 = 0;
const LIVE: u8 = 1;
const NO_HINT: u64 = 2;

struct RegistryModel {
    states: [AtomicU8; 2],
    hint: AtomicU64,
    spill: AtomicU64,
    saturation: AtomicU64,
}

impl RegistryModel {
    fn claim(&self) -> bool {
        let hinted = self.hint.swap(NO_HINT, Ordering::AcqRel);
        if hinted < NO_HINT
            && self.states[hinted as usize]
                .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            return true;
        }
        if self.saturation.fetch_or(0, Ordering::Acquire) & 1 != 0 {
            return false;
        }
        let bits = self.spill.load(Ordering::Acquire) & 3;
        if bits != 0 {
            let bit = 1u64 << bits.trailing_zeros();
            if self.spill.fetch_and(!bit, Ordering::AcqRel) & bit != 0 {
                let slot = bits.trailing_zeros() as usize;
                if self.states[slot]
                    .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return true;
                }
            }
        }
        let snapshot = self.saturation.load(Ordering::Acquire);
        for slot in &self.states {
            if slot
                .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
        if snapshot & 1 == 0 {
            let _ = self.saturation.compare_exchange(
                snapshot,
                snapshot | 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        false
    }

    fn recycle_one(&self) {
        assert!(self.states[1]
            .compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed)
            .is_ok());
        let displaced = self.hint.swap(1, Ordering::Relaxed);
        if displaced < NO_HINT {
            self.spill.fetch_or(1 << displaced, Ordering::Release);
        }
        let mut old = self.saturation.load(Ordering::Acquire);
        loop {
            let next = (old + 2) & !1;
            match self.saturation.compare_exchange_weak(
                old,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(actual) => old = actual,
            }
        }
    }
}

#[test]
fn completed_free_publication_cannot_leave_false_full_or_two_owners() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let reg = Arc::new(RegistryModel {
            states: [AtomicU8::new(LIVE), AtomicU8::new(LIVE)],
            hint: AtomicU64::new(NO_HINT),
            spill: AtomicU64::new(0),
            saturation: AtomicU64::new(0),
        });
        let scanner = Arc::clone(&reg);
        let scan = thread::spawn(move || scanner.claim());
        let recycler = Arc::clone(&reg);
        let publish = thread::spawn(move || recycler.recycle_one());
        let first = scan.join().expect("scanner");
        publish.join().expect("publisher");
        // Model a displaced/consumed hint: completed availability publication
        // must independently defeat any stale negative-scan certification.
        reg.hint.store(NO_HINT, Ordering::Relaxed);
        assert_eq!(reg.saturation.fetch_or(0, Ordering::Acquire) & 1, 0);
        assert_eq!(reg.states[0].load(Ordering::Acquire), LIVE);
        if first {
            assert_eq!(reg.states[1].load(Ordering::Acquire), LIVE);
        } else {
            assert_eq!(reg.states[1].load(Ordering::Acquire), FREE);
            assert!(reg.claim(), "completed publication cannot stay saturated");
        }
        assert_eq!(reg.states[1].load(Ordering::Acquire), LIVE);
    });
}

// ---------------------------------------------------------------------------
// R6-T53: the production selector (stack.rs `pick_with_hint_spill`) with
// `count`, production `bump_count` (index reservation only, a losing CAS
// touches no slot), versioned saturation certification
// (`SaturationHint::snapshot` / `try_mark_saturated` CAS), and the production
// `recycle` swap-with-spill hint publication. The pre-R6 shape — a hint
// consumed without the `candidate < count` bound — is the negative control.
// The `before == max` certification guard is exercised positive only: in this
// faithful model its removal is not end-state observable (see the R6-T53
// fix-round report) — certification then fires only on a factually full
// range, and every later claimability publication carries a version bump
// that invalidates a stale `try_mark_saturated`.
// ---------------------------------------------------------------------------

/// A minted, unclaimed index: claimable through the scan once `count` covers
/// it (production `scan_claimable_slot` returns an EMPTY, uninitialised
/// slot).
const UNCLAIMED: u8 = 2;
/// Registry cap in this model (two slots).
const SEL_MAX: usize = 2;

#[derive(Clone, Copy)]
struct SelectorMutants {
    /// stack.rs: `candidate < before` — the mutant accepts any in-range hint.
    hint_without_count_guard: bool,
}

impl SelectorMutants {
    fn none() -> Self {
        Self {
            hint_without_count_guard: false,
        }
    }
}

struct SelectorModel {
    hint: AtomicU64,
    count: AtomicU64,
    /// SaturationHint shadow: bit 0 = certified negative scan, upper bits =
    /// publication version.
    saturation: AtomicU64,
    /// FREE_SLOTS shadow: bit i set = slot i sits in the spill set.
    spill: AtomicU64,
    states: [AtomicU8; SEL_MAX],
    owners: [AtomicU8; SEL_MAX],
}

impl SelectorModel {
    /// Slot 0 exists (`count = 1`) and is LIVE; slot 1 is minted only by
    /// [`Self::bump`].
    fn fresh() -> Self {
        Self {
            hint: AtomicU64::new(NO_HINT),
            count: AtomicU64::new(1),
            saturation: AtomicU64::new(0),
            spill: AtomicU64::new(0),
            states: [AtomicU8::new(LIVE), AtomicU8::new(UNCLAIMED)],
            owners: [AtomicU8::new(0), AtomicU8::new(0)],
        }
    }

    /// `bump_count` (stack.rs:89): reserve the next index below the cap. The
    /// mint publishes the index only; a losing CAS touches no slot.
    fn bump(&self) -> Option<usize> {
        let mut count = self.count.load(Ordering::Acquire);
        loop {
            if count as usize >= SEL_MAX {
                return None;
            }
            match self.count.compare_exchange_weak(
                count,
                count + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(count as usize),
                Err(actual) => count = actual,
            }
        }
    }

    fn take(&self, slot: usize) -> bool {
        let from = self.states[slot].load(Ordering::Acquire);
        if from != FREE && from != UNCLAIMED {
            return false;
        }
        if self.states[slot]
            .compare_exchange(from, LIVE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            assert_eq!(
                self.owners[slot].fetch_add(1, Ordering::Relaxed),
                0,
                "exclusive state-CAS ownership"
            );
            true
        } else {
            false
        }
    }

    /// One discovery pass in production order (stack.rs `pick_with_hint_spill`):
    /// hint swap → `candidate < before` → spill take → `before == max &&
    /// is_saturated()` → bump → `snapshot()` → scan → `try_mark_saturated`.
    fn claim(&self, mutants: SelectorMutants) -> bool {
        let candidate = self.hint.swap(NO_HINT, Ordering::AcqRel) as usize;
        let before = (self.count.load(Ordering::Acquire) as usize).min(SEL_MAX);
        if candidate < SEL_MAX && (mutants.hint_without_count_guard || candidate < before) {
            let _ = self.take(candidate);
            return true;
        }
        let bits = self.spill.load(Ordering::Acquire) & ((1 << SEL_MAX) - 1);
        if bits != 0 {
            let bit = 1u64 << bits.trailing_zeros();
            if self.spill.fetch_and(!bit, Ordering::AcqRel) & bit != 0 {
                let slot = bits.trailing_zeros() as usize;
                if slot < (self.count.load(Ordering::Acquire) as usize).min(SEL_MAX)
                    && self.take(slot)
                {
                    return true;
                }
            }
        }
        if before == SEL_MAX && self.is_saturated() {
            return false;
        }
        if let Some(idx) = self.bump() {
            return self.take(idx);
        }
        let version = self.snapshot();
        if let Some(idx) = self.scan() {
            if self.take(idx) {
                return true;
            }
        }
        let now = (self.count.load(Ordering::Acquire) as usize).min(SEL_MAX);
        if before == SEL_MAX && now == SEL_MAX {
            self.try_mark_saturated(version);
        }
        false
    }

    /// One fair pass over the cap range (production `scan_claimable_slot`): a
    /// FREE or minted-unclaimed slot is claimable.
    fn scan(&self) -> Option<usize> {
        let count = (self.count.load(Ordering::Acquire) as usize).min(SEL_MAX);
        (0..count).find(|&i| matches!(self.states[i].load(Ordering::Acquire), FREE | UNCLAIMED))
    }

    // -- SaturationHint shadow (src/registry/bootstrap/saturation.rs); the
    // DISABLED terminal-word boundary is unreachable at this model's version
    // counts and is dropped. --

    fn snapshot(&self) -> u64 {
        self.saturation.load(Ordering::Acquire)
    }

    fn is_saturated(&self) -> bool {
        // RMW read like production `is_saturated`: it cannot return a value
        // older than a completed publisher's RMW in the modification order.
        self.saturation.fetch_or(0, Ordering::Acquire) & 1 != 0
    }

    /// Commit a negative scan only if no availability publication raced it.
    fn try_mark_saturated(&self, snapshot: u64) {
        if snapshot & 1 == 0 {
            let _ = self.saturation.compare_exchange(
                snapshot,
                snapshot | 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }
}

fn assert_exclusive(model: &SelectorModel) {
    for slot in 0..SEL_MAX {
        assert!(
            model.owners[slot].load(Ordering::Relaxed) <= 1,
            "exclusive state-CAS ownership"
        );
    }
}

/// A minted, unowned slot left behind: claimable, but discoverable only if
/// certification has not declared the registry full.
fn claimable_slot_left_behind(model: &SelectorModel) -> bool {
    model.count.load(Ordering::Acquire) as usize == SEL_MAX
        && model.owners[1].load(Ordering::Relaxed) == 0
        && matches!(model.states[1].load(Ordering::Acquire), FREE | UNCLAIMED)
}

fn bounded_selector(f: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 2000;
    builder.check(f);
}

#[test]
fn selector_bump_before_scan_never_certifies_a_claimable_slot() {
    bounded_selector(|| {
        let model = Arc::new(SelectorModel::fresh());
        let c = Arc::clone(&model);
        let claimer = thread::spawn(move || c.claim(SelectorMutants::none()));
        let b = Arc::clone(&model);
        // The production mint: bump reserves the index; a losing bump touches
        // no slot.
        let bumper = thread::spawn(move || {
            b.bump();
        });
        claimer.join().unwrap();
        bumper.join().unwrap();
        assert_exclusive(&model);
        // Oracle, evaluated in every schedule: a certified "full" while a
        // minted, unowned slot is left behind is a false full no later
        // publication heals in this model. The production shape never
        // certifies here: certification requires `before == max`, and a
        // claimant that observed the cap scan-claims the minted index
        // instead of certifying.
        assert!(
            !(claimable_slot_left_behind(&model) && model.is_saturated()),
            "certified full while a claimable minted slot is left behind"
        );
        // Liveness: whatever the interleaving, a final claim takes any slot
        // left minted and unowned.
        if claimable_slot_left_behind(&model) {
            assert!(model.claim(SelectorMutants::none()));
        }
    });
}

#[test]
fn stale_hint_at_or_above_count_is_never_a_heap() {
    bounded_selector(|| {
        // A chunk-OOM fuelled minted index sits in the hint while the
        // high-water is still 1: slot 1 is not covered by `count`. The
        // production bound skips it and the spill set still reaches the
        // materialised FREE slot.
        let model = Arc::new(SelectorModel::fresh());
        model.states[0].store(FREE, Ordering::Relaxed);
        model.spill.store(1, Ordering::Relaxed);
        model.hint.store(1, Ordering::Relaxed);
        assert!(model.claim(SelectorMutants::none()));
        assert_exclusive(&model);
        assert_eq!(
            model.owners[1].load(Ordering::Relaxed),
            0,
            "an unminted slot is never handed out"
        );
        assert_eq!(
            model.owners[0].load(Ordering::Relaxed),
            1,
            "spill set still reaches the materialised FREE slot"
        );
    });
}

#[test]
#[should_panic(expected = "an unminted slot is never handed out")]
fn negative_hint_without_count_guard_hands_out_an_unminted_slot() {
    bounded_selector(|| {
        // count = 1: the hint holds index 1, which the high-water does not
        // cover (the chunk-OOM fuelled minted-index shape). The mutant
        // consumes it as a heap candidate and takes the slot.
        let model = Arc::new(SelectorModel::fresh());
        model.hint.store(1, Ordering::Relaxed);
        assert!(model.claim(SelectorMutants {
            hint_without_count_guard: true,
        }));
        // Oracle: the high-water still excludes index 1, so no ownership may
        // have been granted through the stale hint. The mutant granted it,
        // so this fires in the model's only schedule.
        assert_eq!(
            model.owners[1].load(Ordering::Relaxed),
            0,
            "an unminted slot is never handed out"
        );
    });
}
