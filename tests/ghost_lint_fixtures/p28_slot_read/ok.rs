//! One immutable-after-publication scalar slot; no mapped memory is accessed.
#![forbid(unsafe_code)]
use core::sync::atomic::{AtomicU64, Ordering};

#[repr(C)]
pub(crate) struct AdviceSlot {
    payload: [AtomicU64; 7],
    complete: AtomicU64,
}

impl AdviceSlot {
    pub(crate) const fn new() -> Self {
        Self {
            payload: [const { AtomicU64::new(0) }; 7],
            complete: AtomicU64::new(0),
        }
    }

    /// Exactly one writer owns this index; no slot is recycled in an active log.
    pub(crate) fn publish(&self, sequence: u64, words: [u64; 7]) -> Result<(), &'static str> {
        if sequence == 0 || sequence == u64::MAX {
            return Err("invalid advice completion sequence");
        }
        // A single bounded claim also prevents any cursor-wrap reuse from writing
        // over a prior publication. MAX is never a supported completion sequence.
        if self
            .complete
            .compare_exchange(0, u64::MAX, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return Err("advice event slot cannot be reused");
        }
        for (field, value) in self.payload.iter().zip(words) {
            field.store(value, Ordering::Relaxed);
        }
        self.complete.store(sequence, Ordering::Release);
        Ok(())
    }

    pub(crate) fn read(&self, sequence: u64) -> Result<[u64; 7], &'static str> {
        if self.complete.load(Ordering::Acquire) != sequence {
            return Err("advice event slot is not completely published");
        }
        let mut words = [0; 7];
        for (word, field) in words.iter_mut().zip(&self.payload) {
            *word = field.load(Ordering::Relaxed);
        }
        Ok(words)
    }
}

const _: () = assert!(core::mem::size_of::<AdviceSlot>() == 64);
