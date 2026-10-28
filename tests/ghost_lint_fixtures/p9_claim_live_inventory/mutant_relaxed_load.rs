// expect: not in the catalog table
impl<'a> LargeReservationState<'a> {
    pub fn publish_pending(&self, generation: u64) -> bool {
        let published = self
            .word
            .compare_exchange(
                pack_large_state(LargePhase::Live, generation),
                pack_large_state(LargePhase::Pending, generation),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok();
        crate::ghost::ghost_event!(
            "large/pending",
            core::ptr::from_ref(self.word).addr(),
            i64::from(published)
        );
        published
    }

    pub fn claim_pending(&self) -> Option<u64> {
        let observed = self.word.load(Ordering::Acquire);
        if large_phase(observed) != Some(LargePhase::Pending) {
            return None;
        }
        let generation = large_generation(observed);
        let claimed = self
            .word
            .compare_exchange(
                observed,
                pack_large_state(LargePhase::Consuming, generation),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()
            .map(|_| generation);
        crate::ghost::ghost_event!(
            "large/pending",
            core::ptr::from_ref(self.word).addr(),
            -i64::from(claimed.is_some())
        );
        claimed
    }

    pub fn claim_live(&self) -> Option<u64> {
        let observed = self.word.load(Ordering::Relaxed);
        if large_phase(observed) != Some(LargePhase::Live) {
            return None;
        }
        let generation = large_generation(observed);
        self.word
            .compare_exchange(
                observed,
                pack_large_state(LargePhase::Consuming, generation),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()
            .map(|_| generation)
    }

    pub fn begin_reuse(&self) -> Option<u64> {
        let observed = self.word.load(Ordering::Acquire);
        if large_phase(observed) != Some(LargePhase::Cached) {
            return None;
        }
        let next = next_large_generation(large_generation(observed))?;
        self.word
            .compare_exchange(
                observed,
                pack_large_state(LargePhase::Initializing, next),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()
            .map(|_| next)
    }

    fn transition(&self, generation: u64, from: LargePhase, to: LargePhase) -> bool {
        self.word
            .compare_exchange(
                pack_large_state(from, generation),
                pack_large_state(to, generation),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}
