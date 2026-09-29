use core::sync::atomic::{AtomicU64, Ordering};

use super::LargeReservationState;
use crate::alloc_core::segment_header::{
    large_generation, large_phase, next_large_generation, next_large_generation_bounded,
    pack_large_state, LargePhase, MAX_LARGE_GENERATION,
};

#[test]
fn pending_is_single_admission_and_single_owner_claim() {
    let word = AtomicU64::new(pack_large_state(LargePhase::Live, 7));
    let state = LargeReservationState::new(&word);
    assert!(!state.publish_pending(6));
    assert!(state.publish_pending(7));
    assert!(!state.publish_pending(7));
    assert_eq!(state.claim_pending(), Some(7));
    assert_eq!(state.claim_pending(), None);
    assert_eq!(
        large_phase(word.load(Ordering::Acquire)),
        Some(LargePhase::Consuming)
    );
}

#[test]
fn consumed_cache_hit_advances_generation_only_after_reset_phase() {
    let word = AtomicU64::new(pack_large_state(LargePhase::Live, 9));
    let state = LargeReservationState::new(&word);
    assert!(state.publish_pending(9));
    assert_eq!(state.claim_pending(), Some(9));
    assert!(state.cache_consumed(9));
    assert!(!state.cache_consumed(9));
    assert_eq!(state.begin_reuse(), Some(10));
    assert_eq!(
        large_phase(word.load(Ordering::Acquire)),
        Some(LargePhase::Initializing)
    );
    assert!(!state.publish_pending(10));
    assert!(!state.finish_reuse(9));
    assert!(state.finish_reuse(10));
    assert_eq!(large_generation(word.load(Ordering::Acquire)), 10);
    assert!(!state.publish_pending(9));
    assert!(state.publish_pending(10));
}

#[test]
fn release_is_terminal_and_cannot_reenter_cache() {
    let word = AtomicU64::new(pack_large_state(LargePhase::Live, 1));
    let state = LargeReservationState::new(&word);
    assert!(state.publish_pending(1));
    assert_eq!(state.claim_pending(), Some(1));
    assert!(state.release_consumed(1));
    assert_eq!(
        large_phase(word.load(Ordering::Acquire)),
        Some(LargePhase::Released)
    );
    assert!(!state.cache_consumed(1));
    assert_eq!(state.begin_reuse(), None);
    assert!(!state.publish_pending(1));
}

#[test]
fn reduced_width_exhaustion_retires_without_wrap() {
    let limit = 0b111;
    for generation in 0..limit {
        assert_eq!(
            next_large_generation_bounded(generation, limit),
            Some(generation + 1)
        );
    }
    assert_eq!(next_large_generation_bounded(limit, limit), None);
    assert_eq!(next_large_generation(MAX_LARGE_GENERATION), None);

    let word = AtomicU64::new(pack_large_state(LargePhase::Cached, MAX_LARGE_GENERATION));
    let state = LargeReservationState::new(&word);
    assert_eq!(state.begin_reuse(), None);
    assert_eq!(
        large_phase(word.load(Ordering::Acquire)),
        Some(LargePhase::Cached)
    );
    assert!(state.release_cached(MAX_LARGE_GENERATION));
    assert_eq!(
        large_phase(word.load(Ordering::Acquire)),
        Some(LargePhase::Released)
    );
}

#[test]
#[should_panic(expected = "old predecessor pointer crosses reservation incarnation")]
fn negative_control_pointer_stack_aba() {
    // A paused push retains predecessor VA 0x1000, not its reservation identity.
    let predecessor = (0x1000usize, 1u64);
    let resumed_predecessor = (0x1000usize, 2u64);
    assert_eq!(predecessor.0, resumed_predecessor.0);
    assert_eq!(
        predecessor, resumed_predecessor,
        "old predecessor pointer crosses reservation incarnation"
    );
}
