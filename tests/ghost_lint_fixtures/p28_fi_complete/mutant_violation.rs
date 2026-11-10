// expect: not in the catalog table
//! Bounded append-only diagnostic capture; never an allocator policy input.
#![forbid(unsafe_code)]
use super::event::AdviceEvent;
use super::slot::AdviceSlot;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

/// Caller establishes quiescence before reading. A concurrent reader cannot prove
/// that no future writer exists; publication checks do not authenticate this premise.
/// CAP is compile-time storage, not a runtime allocator configuration setting.
pub struct AdviceEventLog<const CAP: usize> {
    started: AtomicBool,
    cursor: AtomicU64,
    failure: AtomicU8,
    slots: [AdviceSlot; CAP],
}

impl<const CAP: usize> Default for AdviceEventLog<CAP> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAP: usize> AdviceEventLog<CAP> {
    pub const fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            cursor: AtomicU64::new(0),
            failure: AtomicU8::new(0),
            slots: [const { AdviceSlot::new() }; CAP],
        }
    }

    fn message(reason: u8) -> &'static str {
        match reason {
            1 => "advice event capacity exhausted",
            2 => "advice event integer width unsupported",
            3 => "advice event publication incomplete",
            4 => "advice event encoding invalid",
            5 => "advice event log cannot be restarted",
            6 => "advice event log capacity invalid",
            7 => "advice event boundary is not quiescent",
            _ => "advice event capture unsupported",
        }
    }

    fn reject<T>(&self, reason: u8) -> Result<T, &'static str> {
        // Diagnostic first-failure code only; it publishes no payload or lifetime.
        let first =
            match self
                .failure
                .compare_exchange(0, reason, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => reason,
                Err(first) => first,
            };
        Err(Self::message(first))
    }

    fn supported(&self) -> Result<(), &'static str> {
        let reason = self.failure.load(Ordering::Relaxed);
        if reason == 0 {
            Ok(())
        } else {
            Err(Self::message(reason))
        }
    }

    /// Start once before any producer exists. No reset/recycling API is provided.
    pub fn begin(&self) -> Result<(), &'static str> {
        if CAP == 0 || CAP > 65536 {
            return self.reject(6);
        }
        self.supported()?;
        if self
            .started
            .compare_exchange(false, true, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            return self.reject(5);
        }
        Ok(())
    }

    /// Inert local-log FI only; the native global exposes no mutable log accessor.
    #[cfg(feature = "fault-injection")]
    #[doc(hidden)]
    pub fn seed_cursor_for_test(&self, value: u64) {
        self.cursor.store(value, Ordering::Relaxed);
    }

    /// Reserve a deliberately incomplete event in a synthetic local capture.
    #[cfg(feature = "fault-injection")]
    #[doc(hidden)]
    pub fn reserve_unpublished_for_test(&self) -> Result<u64, &'static str> {
        if !self.is_started() { return Err("advice event capture has not started"); }
        self.supported()?;
        let sequence = self.cursor.fetch_add(1, Ordering::Relaxed);
        if usize::try_from(sequence).ok().is_none_or(|index| index >= CAP) { return self.reject(1); }
        Ok(sequence)
    }

    /// Finish only a synthetic reservation; a failed epoch can never be repaired.
    #[cfg(feature = "fault-injection")]
    #[doc(hidden)]
    pub fn complete_reserved_for_test(&self, sequence: u64, event: AdviceEvent) -> Result<(), &'static str> {
        self.supported()?;
        if !self.is_started() || sequence >= self.cursor.swap(0, Ordering::Relaxed) {
            return Err("synthetic slot was not reserved");
        }
        let index = usize::try_from(sequence).map_err(|_| "synthetic slot index overflow")?;
        let slot = self.slots.get(index).ok_or("synthetic slot outside capacity")?;
        let complete = sequence.checked_add(1).ok_or("synthetic completion overflow")?;
        slot.publish(complete, event.words())
    }

    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }

    /// Allocation-free single reservation plus seven scalar stores and publication.
    /// Exhaustion/wrap cannot produce readable truncated evidence: failure is sticky,
    /// and supported readers require all producers joined/quiescent.
    pub fn append(&self, event: AdviceEvent) -> Result<u64, &'static str> {
        if !self.is_started() {
            return Err("advice event capture has not started");
        }
        self.supported()?;
        let sequence = self.cursor.fetch_add(1, Ordering::Relaxed);
        let index = match usize::try_from(sequence) {
            Ok(index) => index,
            Err(_) => return self.reject(2),
        };
        let Some(slot) = self.slots.get(index) else {
            return self.reject(1);
        };
        let Some(complete) = sequence.checked_add(1) else {
            return self.reject(1);
        };
        if slot.publish(complete, event.words()).is_err() {
            return self.reject(3);
        }
        Ok(sequence)
    }

    /// Record a producer-side conversion/geometry failure without returning it to
    /// allocator policy. Readable evidence is disabled; allocator behavior is not.
    pub fn record(&self, event: Result<AdviceEvent, &'static str>) {
        if !self.is_started() {
            return;
        }
        match event {
            Ok(event) => {
                let _ = self.append(event);
            }
            Err(_) => {
                let _: Result<(), _> = self.reject(4);
            }
        }
    }

    /// A supported quiescent boundary covers every reserved slot, not just cursor
    /// arithmetic. Acquire completion reads precede relaxed immutable payload loads.
    pub fn boundary(&self) -> Result<u64, &'static str> {
        if !self.is_started() {
            return Err("advice event capture has not started");
        }
        self.supported()?;
        let end = self.cursor.load(Ordering::Relaxed);
        let count = match usize::try_from(end) {
            Ok(count) => count,
            Err(_) => return self.reject(2),
        };
        if count > CAP {
            return self.reject(1);
        }
        for index in 0..count {
            let sequence = u64::try_from(index)
                .map_err(|_| "advice event index conversion overflow")?
                .checked_add(1)
                .ok_or("advice event completion overflow")?;
            let words = match self.slots[index].read(sequence) {
                Ok(words) => words,
                Err(_) => return self.reject(3),
            };
            if AdviceEvent::from_words(words).is_err() {
                return self.reject(4);
            }
        }
        if self.cursor.load(Ordering::Relaxed) != end {
            return self.reject(7);
        }
        self.supported()?;
        Ok(end)
    }

    /// Read an immutable prefix slot only after a supported quiescent boundary.
    /// The owning controller must keep producers quiescent throughout replay.
    pub fn read(&self, index: u64, boundary: u64) -> Result<AdviceEvent, &'static str> {
        self.supported()?;
        if !self.is_started()
            || index >= boundary
            || self.cursor.load(Ordering::Relaxed) != boundary
        {
            return self.reject(7);
        }
        let offset = match usize::try_from(index) {
            Ok(index) => index,
            Err(_) => return self.reject(2),
        };
        let Some(slot) = self.slots.get(offset) else {
            return self.reject(1);
        };
        let Some(sequence) = index.checked_add(1) else {
            return self.reject(1);
        };
        let words = match slot.read(sequence) {
            Ok(words) => words,
            Err(_) => return self.reject(3),
        };
        match AdviceEvent::from_words(words) {
            Ok(event) => Ok(event),
            Err(_) => self.reject(4),
        }
    }

    pub(crate) fn changed_boundary<T>(&self) -> Result<T, &'static str> {
        self.reject(7)
    }

    pub const fn slot_storage_bytes() -> usize {
        core::mem::size_of::<[AdviceSlot; CAP]>()
    }
    pub const fn total_storage_bytes() -> usize {
        core::mem::size_of::<Self>()
    }
}
