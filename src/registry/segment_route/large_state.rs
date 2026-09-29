use core::sync::atomic::{AtomicU64, Ordering};

const PHASE_BITS: u64 = 3;
const MAX_GENERATION: u64 = u64::MAX >> PHASE_BITS;
const INITIALIZING: u64 = 0;
const LIVE: u64 = 1;
const PENDING: u64 = 2;
const CONSUMING: u64 = 3;
const CACHED: u64 = 4;
const RELEASED: u64 = 5;

/// Per-reservation Large phase/generation, separate from reservation bytes.
/// Owner transitions: INITIALIZING -> LIVE(1), PENDING -> CONSUMING ->
/// CACHED -> LIVE(g+1) or RELEASED. Producer: LIVE(g) -> PENDING(g).
/// Only LIVE/PENDING/CONSUMING carry an external allocation credit; the
/// owner ledger and cache/release ordering are not wired by this substrate.
pub struct LargeState {
    word: AtomicU64,
}

impl LargeState {
    pub const fn new() -> Self {
        Self {
            word: AtomicU64::new(INITIALIZING),
        }
    }

    pub(super) fn activate(&self) -> bool {
        self.word
            .compare_exchange(
                INITIALIZING,
                Self::pack(1, LIVE),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Producer terminal CAS. No descriptor or reservation access follows.
    pub(super) fn publish(&self) -> bool {
        let current = self.word.load(Ordering::Acquire);
        if Self::phase(current) != LIVE {
            return false;
        }
        self.word
            .compare_exchange(
                current,
                (current & !7) | PENDING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub(super) fn claim(&self) -> bool {
        let current = self.word.load(Ordering::Acquire);
        Self::phase(current) == PENDING
            && self
                .word
                .compare_exchange(
                    current,
                    (current & !7) | CONSUMING,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
    }

    pub(super) fn cache(&self) -> bool {
        let current = self.word.load(Ordering::Acquire);
        Self::phase(current) == CONSUMING
            && self
                .word
                .compare_exchange(
                    current,
                    (current & !7) | CACHED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
    }

    /// Owner-only reissue. Exhaustion leaves the entry cached, never aliased.
    pub(super) fn reissue(&self) -> bool {
        let current = self.word.load(Ordering::Acquire);
        let generation = current >> PHASE_BITS;
        if Self::phase(current) != CACHED || generation == MAX_GENERATION {
            return false;
        }
        self.word
            .compare_exchange(
                current,
                Self::pack(generation + 1, LIVE),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub(super) fn retire_cached(&self) -> bool {
        let current = self.word.load(Ordering::Acquire);
        Self::phase(current) == CACHED
            && self
                .word
                .compare_exchange(
                    current,
                    (current & !7) | RELEASED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
    }

    pub fn generation(&self) -> u64 {
        self.word.load(Ordering::Acquire) >> PHASE_BITS
    }

    const fn pack(generation: u64, phase: u64) -> u64 {
        (generation << PHASE_BITS) | phase
    }

    const fn phase(word: u64) -> u64 {
        word & 7
    }
}

impl Default for LargeState {
    fn default() -> Self {
        Self::new()
    }
}
