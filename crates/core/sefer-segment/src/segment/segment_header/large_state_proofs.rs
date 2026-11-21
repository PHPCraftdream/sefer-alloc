use super::terminal_words::{
    large_generation, large_phase, next_large_generation_bounded, pack_large_state, LargePhase,
    MAX_LARGE_GENERATION,
};
use crate::support::reservation_state_impl::LargeReservationState;
use core::sync::atomic::{AtomicU64, Ordering};

#[kani::proof]
fn large_codec_roundtrip_and_canonical() {
    let generation: u64 = kani::any();
    kani::assume(generation <= MAX_LARGE_GENERATION);
    let code: u64 = kani::any();
    kani::assume(code <= 6);
    let phase = large_phase(code).expect("valid phase code");
    let word = pack_large_state(phase, generation);
    assert_eq!(large_phase(word), Some(phase));
    assert_eq!(large_generation(word), generation);
    let arbitrary: u64 = kani::any();
    if let Some(decoded) = large_phase(arbitrary) {
        assert_eq!(
            pack_large_state(decoded, large_generation(arbitrary)),
            arbitrary
        );
    }
    kani::cover!(
        large_phase(arbitrary).is_none(),
        "invalid phase is reachable"
    );
}

#[kani::proof]
fn next_generation_never_exceeds_limit() {
    let generation: u64 = kani::any();
    let limit: u64 = kani::any();
    let next = next_large_generation_bounded(generation, limit);
    match next {
        Some(value) => {
            assert!(limit <= MAX_LARGE_GENERATION);
            assert!(generation < limit);
            assert_eq!(value, generation + 1);
            assert!(value <= limit);
        }
        None => assert!(generation >= limit || limit > MAX_LARGE_GENERATION),
    }
    kani::cover!(next.is_some(), "generation can advance");
}

#[kani::proof]
fn large_reservation_state_transition_relation() {
    use LargePhase::{Cached, Consuming, Initializing, Live, Pending, Released};
    let before: u64 = kani::any();
    let word = AtomicU64::new(before);
    let state = LargeReservationState::new(&word);
    let generation: u64 = kani::any();
    kani::assume(generation <= MAX_LARGE_GENERATION);
    let operation: u8 = kani::any();
    kani::assume(operation < 9);
    let old_generation = large_generation(before);
    // Independent lifecycle table from VER-04 §1.4 / §5.4: each
    // method must implement its own edge, not just any allowed edge.
    let (from, to) = match operation {
        0 => (Live, Pending),
        1 => (Pending, Consuming),
        2 => (Live, Consuming),
        3 => (Consuming, Cached),
        4 => (Consuming, Released),
        5 => (Cached, Released),
        6 => (Initializing, Released),
        7 => (Cached, Initializing),
        _ => (Initializing, Live), // operation == 8 by the guard
    };
    let expected_success = large_phase(before) == Some(from)
        && match operation {
            1 | 2 => true,
            7 => old_generation < MAX_LARGE_GENERATION,
            _ => old_generation == generation,
        };
    let mut returned_generation = None;
    let success = match operation {
        0 => state.publish_pending(generation),
        1 => {
            returned_generation = state.claim_pending();
            returned_generation.is_some()
        }
        2 => {
            returned_generation = state.claim_live();
            returned_generation.is_some()
        }
        3 => state.cache_consumed(generation),
        4 => state.release_consumed(generation),
        5 => state.release_cached(generation),
        6 => state.release_initializing(generation),
        7 => {
            returned_generation = state.begin_reuse();
            returned_generation.is_some()
        }
        _ => state.finish_reuse(generation),
    };
    let after = word.load(Ordering::Relaxed);
    let changed = after != before;
    // Kani 0.67 models strong CAS success flag loosely; `ok => changed`
    // is pinned natively by consumed_cache_hit_advances_generation_only_after_reset_phase.
    assert_eq!(changed, expected_success);
    assert!(!changed || success);
    if changed {
        let next_generation = if operation == 7 {
            old_generation + 1 // expected_success excludes exhaustion
        } else {
            old_generation
        };
        assert_eq!(large_phase(after), Some(to));
        assert_eq!(large_generation(after), next_generation);
        assert_eq!(after, pack_large_state(to, next_generation));
        if operation == 1 || operation == 2 || operation == 7 {
            assert_eq!(returned_generation, Some(next_generation));
        }
    } else {
        assert_eq!(after, before);
        if !success {
            assert_eq!(returned_generation, None);
        }
    }
    kani::cover!(changed, "a lifecycle transition changes the word");
}
