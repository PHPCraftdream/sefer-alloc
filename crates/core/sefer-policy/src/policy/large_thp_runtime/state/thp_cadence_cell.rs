//! One request group's adaptive control-cadence state: a saturating
//! agreement run held in a single atomic word.

use super::large_thp_cadence::LargeThpCadence;
use super::{AtomicU64, Ordering};

/// Bits `0..=4` hold the agreement run (`0..=20`); the rest stay zero in
/// production. The level is derived from one load of this word. Relaxed
/// accesses suffice: the word publishes no external payload and a stale
/// level only delays a rate change.
pub struct ThpCadenceCell {
    pub state: AtomicU64,
}

// Under the mechanism gate the runtime uses only part of this surface; the
// actual-source tests exercise all of it.
#[allow(dead_code)]
impl ThpCadenceCell {
    /// Run zero: level 0, today's fixed 1-in-8 candidates.
    pub const INITIAL: u64 = 0;
    const RUN_MASK: u64 = 0x1F;
    const MAX_ATTEMPTS: usize = 8;
    /// Diagnostic pin: bit 5 enables it, bits `6..=8` carry the level.
    #[cfg(feature = "bench-internals")]
    const PIN_VALID: u64 = 1 << 5;
    #[cfg(feature = "bench-internals")]
    const PIN_MASK: u64 = 0xF << 5;

    // Process statics initialize directly; actual-source tests use new().
    pub fn new() -> Self {
        Self {
            state: AtomicU64::new(Self::INITIAL),
        }
    }

    fn run_of(word: u64) -> u32 {
        let run = (word & Self::RUN_MASK) as u32;
        if run > LargeThpCadence::RUN_MAX {
            LargeThpCadence::RUN_MAX
        } else {
            run
        }
    }

    /// Current level from one load; bench-internals honours the pin.
    pub fn level(&self) -> u32 {
        let word = self.state.load(Ordering::Relaxed);
        #[cfg(feature = "bench-internals")]
        if word & Self::PIN_VALID != 0 {
            let pinned = ((word >> 6) & 0x7) as u32;
            return pinned.min(LargeThpCadence::LEVEL_MAX);
        }
        LargeThpCadence::level_of(Self::run_of(word))
    }

    /// Current agreement run from one load (telemetry).
    pub fn run(&self) -> u32 {
        Self::run_of(self.state.load(Ordering::Relaxed))
    }

    /// Record one agreeing observation. At most eight CAS attempts; false
    /// means the agreement was dropped, which only keeps the rate higher. A
    /// saturated run returns without any read-modify-write.
    pub fn note_agree(&self) -> bool {
        let mut current = self.state.load(Ordering::Relaxed);
        let mut attempts = 0;
        while attempts < Self::MAX_ATTEMPTS {
            attempts += 1;
            let run = Self::run_of(current);
            if run >= LargeThpCadence::RUN_MAX {
                return true;
            }
            let next =
                (current & !Self::RUN_MASK) | u64::from(LargeThpCadence::next_run(run, true));
            match self
                .state
                .compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
        false
    }

    /// Record one disagreeing observation: one wait-free `fetch_and` clears
    /// the run, so a disagreement is never lost to contention. Returns the
    /// previous run.
    pub fn note_disagree(&self) -> u32 {
        let previous = self.state.fetch_and(!Self::RUN_MASK, Ordering::Relaxed);
        Self::run_of(previous)
    }

    /// Bench-internals: force the level, or release the pin with `None`.
    /// The two steps are not one atomic action; call it while no control
    /// decisions are in flight.
    #[cfg(feature = "bench-internals")]
    pub fn pin(&self, level: Option<u32>) {
        self.state.fetch_and(!Self::PIN_MASK, Ordering::Relaxed);
        if let Some(level) = level {
            let level = u64::from(level.min(LargeThpCadence::LEVEL_MAX));
            self.state
                .fetch_or(Self::PIN_VALID | (level << 6), Ordering::Relaxed);
        }
    }
}

impl Default for ThpCadenceCell {
    fn default() -> Self {
        Self::new()
    }
}
