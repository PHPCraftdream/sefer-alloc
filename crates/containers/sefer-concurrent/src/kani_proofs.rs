//! Sequential Kani proof over the epoch hand's vacant-slot facts, compiled
//! only under `cfg(kani)`. Moved here from sefer-alloc's `src/kani_proofs.rs`
//! (workspace-restructure plan step P1.2): this crate owns the
//! `pub(crate)` `epoch::hand` internals the harness needs.
//!
//! - `hand_proofs` checks two vacant-slot facts, not epoch concurrency.
//!   The Loom tests are handwritten shadow publication models, not proofs of
//!   the real `AtomicSlot` implementation or reclamation.
//!
//! These proofs do not establish concurrent memory ordering, reservation
//! lifetime, ownership authority, or reclamation safety.

#[cfg(kani)]
mod hand_proofs {
    use crate::epoch::hand::AtomicSlot;

    #[kani::proof]
    fn vacant_starts_at_generation_zero() {
        let slot = AtomicSlot::<u32>::vacant();
        assert_eq!(slot.generation(), 0);
    }

    #[kani::proof]
    fn drop_value_vacant_is_noop() {
        let mut slot = AtomicSlot::<u32>::vacant();
        slot.drop_value();
    }
}
