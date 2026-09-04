//! Deterministic test-only oracle for the local backoff state machine.

#![cfg(any(feature = "test-internals", loom))]

#[test]
fn backoff_progression_and_saturation_are_exact() {
    assert_eq!(
        tagged_index_stack::backoff_spin_depths_for_test(),
        [1, 2, 4, 8, 16, 32, 64, 64, 64]
    );
}
