#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

#[path = "r11_p3_small_sidecar_issue_oom/support.rs"]
mod support;

#[test]
fn persistent_spill_oom_rolls_back_and_transient_failure_recovers() {
    support::verify(false);
}
