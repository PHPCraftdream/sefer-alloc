//! Regression oracle for task #1309 (finding F4 of the fifteenth
//! independent review,
//! `docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`):
//! `NodeId` is valid-by-construction with respect to the `NO_NODE`
//! sentinel. Before the fix, `NodeId::new` was an unchecked constructor
//! that happily wrapped `NO_NODE` (`u32::MAX`) even though its own doc
//! comment said the sentinel "must NOT be wrapped".

use numa_shim::{NodeId, NO_NODE};

/// The one value invalid on EVERY platform — the sentinel itself — is the
/// one value construction rejects.
#[test]
fn new_rejects_exactly_the_no_node_sentinel() {
    assert!(
        NodeId::new(NO_NODE).is_none(),
        "NodeId::new(NO_NODE) must not construct — the sentinel is the one value the type exists to exclude"
    );
    // Same value spelled explicitly.
    assert!(NodeId::new(u32::MAX).is_none());
}

/// Every NON-sentinel `u32` constructs — including values that are invalid
/// on specific platforms (64 is beyond Linux's single-`u64` nodemask but is
/// NOT the sentinel): platform-dependent node existence is validated at
/// `reserve_preferred_on_node` time, not at construction. Proves the
/// constructor rejects a single exact value, not a range.
#[test]
fn new_accepts_every_non_sentinel_value() {
    for id in [0u32, 63, 64, u32::MAX - 1] {
        match NodeId::new(id) {
            Some(node) => assert_eq!(node.get(), id, "get() must round-trip the wrapped id"),
            None => panic!("NodeId::new({id}) must construct — only NO_NODE is rejected"),
        }
    }
}
