#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::sync::atomic::{AtomicU64, Ordering};

use sefer_alloc::alloc_core::segment_header::{large_phase, pack_large_state, LargePhase};
use sefer_alloc::alloc_core::LargeReservationState;

// One instance owns one credit in LIVE/PENDING/CONSUMING. Transfer to cache,
// rollback, or release owns none; only a completed reissue creates a new one.
fn credit(word: &AtomicU64) -> u8 {
    match large_phase(word.load(Ordering::Acquire)) {
        Some(LargePhase::Live | LargePhase::Pending | LargePhase::Consuming) => 1,
        Some(
            LargePhase::Unused
            | LargePhase::Initializing
            | LargePhase::Cached
            | LargePhase::Released,
        ) => 0,
        None => panic!("invalid Large phase"),
    }
}

#[test]
fn owner_release_and_cache_reissue_transfer_credit_once() {
    let word = AtomicU64::new(pack_large_state(LargePhase::Live, 1));
    let state = LargeReservationState::new(&word);
    assert_eq!(credit(&word), 1); // fresh issue
    assert_eq!(state.claim_live(), Some(1));
    assert_eq!(credit(&word), 1); // still outstanding until disposition
    assert_eq!(state.claim_live(), None);
    assert!(state.cache_consumed(1));
    assert_eq!(credit(&word), 0);
    assert!(!state.cache_consumed(1));
    assert_eq!(state.begin_reuse(), Some(2));
    assert_eq!(credit(&word), 0); // reset/table registration not yet published
    assert!(state.finish_reuse(2));
    assert_eq!(credit(&word), 1); // new allocation instance
    assert_eq!(state.claim_live(), Some(2));
    assert!(state.release_consumed(2));
    assert_eq!(credit(&word), 0);
    assert!(!state.release_consumed(2));
}

#[test]
fn remote_pending_claim_and_rollback_cannot_double_consume() {
    let word = AtomicU64::new(pack_large_state(LargePhase::Live, 4));
    let state = LargeReservationState::new(&word);
    assert!(state.publish_pending(4));
    assert_eq!(credit(&word), 1); // producer never removes credit
    assert!(!state.publish_pending(4));
    assert_eq!(state.claim_live(), None);
    assert_eq!(state.claim_pending(), Some(4));
    assert_eq!(credit(&word), 1); // detached/claimed but not reclaimed
    assert_eq!(state.claim_pending(), None);
    assert!(state.cache_consumed(4));
    assert_eq!(credit(&word), 0);
    assert_eq!(state.begin_reuse(), Some(5));
    assert_eq!(credit(&word), 0);
    assert!(state.release_initializing(5)); // registration/OOM rollback
    assert_eq!(credit(&word), 0);
    assert!(!state.finish_reuse(5));
    assert!(!state.release_initializing(5));
}

#[test]
fn cached_eviction_has_no_outstanding_credit() {
    let word = AtomicU64::new(pack_large_state(LargePhase::Cached, 8));
    let state = LargeReservationState::new(&word);
    assert_eq!(credit(&word), 0);
    assert!(state.release_cached(8));
    assert_eq!(credit(&word), 0);
    assert!(!state.release_cached(8));
    assert_eq!(state.begin_reuse(), None);
}
