// expect: rule requires ["AcqRel", "Acquire"]
impl HeapRegistry {
    fn claim_impl<M, R>(mut materialise: M, on_already_initialised: R) -> Option<HeapLease> {
        loop {
            let first_claim = match observed {
                Ok(SlotState::Empty) => {
                    if slot
                        .cas_state(
                            SlotState::Empty,
                            SlotState::Initializing,
                            Ordering::Relaxed,
                            Ordering::Acquire,
                        )
                        .is_err()
                    {
                        continue;
                    }
                    true
                }
                Ok(SlotState::Free) => {
                    if slot
                        .cas_state(
                            SlotState::Free,
                            SlotState::Live,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_err()
                    {
                        continue;
                    }
                    if slot.initialised.load(Ordering::Acquire) {
                        false
                    } else {
                        if slot
                            .cas_state(
                                SlotState::Live,
                                SlotState::Initializing,
                                Ordering::Release,
                                Ordering::Relaxed,
                            )
                            .is_err()
                        {
                            fatal(b"claim: initializing cas lost in recovery scan\n");
                        }
                        true
                    }
                }
                _ => continue,
            };
            if first_claim {
                slot.initialised.store(true, Ordering::Release);
                if slot
                    .cas_state(
                        SlotState::Initializing,
                        SlotState::Live,
                        Ordering::Release,
                        Ordering::Relaxed,
                    )
                    .is_err()
                {
                    fatal(b"claim: live publish cas lost\n");
                }
            }
        }
    }
}
