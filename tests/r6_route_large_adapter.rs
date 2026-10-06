#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::alloc_core::segment_header::{pack_large_state, LargePhase};
use sefer_alloc::registry::segment_route::LargeState;

#[test]
fn adapter_uses_existing_phase_encoding_and_reset_gate() {
    let state = LargeState::new();
    assert_eq!(state.word_for_test(), pack_large_state(LargePhase::Live, 1));
    assert!(state.publish_pending());
    assert_eq!(
        state.word_for_test(),
        pack_large_state(LargePhase::Pending, 1)
    );
    assert_eq!(state.claim_pending(), Some(1));
    assert_eq!(
        state.word_for_test(),
        pack_large_state(LargePhase::Consuming, 1)
    );
    assert!(state.cache_consumed(1));
    assert_eq!(
        state.word_for_test(),
        pack_large_state(LargePhase::Cached, 1)
    );
    assert_eq!(state.begin_reuse(), Some(2));
    assert_eq!(
        state.word_for_test(),
        pack_large_state(LargePhase::Initializing, 2)
    );
    assert!(!state.publish_pending());
    assert!(!state.finish_reuse(1));
    // Phase-only oracle; allocation-path owner reset is outside this seam.
    assert!(state.finish_reuse(2));
    assert_eq!(state.word_for_test(), pack_large_state(LargePhase::Live, 2));
}
