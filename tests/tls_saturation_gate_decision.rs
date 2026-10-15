//! R6-T07: the TLS saturated-verdict retry policy — a cached negative verdict
//! blocks re-claiming only until the `SaturationHint` version moves or the
//! skip interval is exhausted.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::global::tls_heap::dbg_saturation_retry_decision;

const INTERVAL: u32 = 64;

#[test]
fn saturated_verdict_blocks_until_version_changes_or_interval_elapses() {
    // No cached verdict: always retry.
    assert!(dbg_saturation_retry_decision(false, 8, 0, 8));
    // Cached verdict, unchanged version: blocked (skips below the interval).
    assert!(!dbg_saturation_retry_decision(true, 8, 0, 8));
    assert!(!dbg_saturation_retry_decision(true, 8, INTERVAL - 1, 8));
    // ...but only for INTERVAL attempts.
    assert!(dbg_saturation_retry_decision(true, 8, INTERVAL, 8));
    // Any version change (a slot was published claimable) invalidates at once.
    assert!(dbg_saturation_retry_decision(true, 8, 0, 10));
}
