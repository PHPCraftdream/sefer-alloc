//! B4-30 platform SKIP marker. On macOS the budget test file
//! (`b4_30_first_alloc_min_stack.rs`) compiles to nothing via its file-level
//! `cfg`, so there is no measurement and no failure — this tiny file emits a
//! MARKED green (a deliberate skip, not an unmarked vacuous pass). The
//! marker text goes to test stdout/stderr, which libtest CAPTURES: without
//! `--nocapture` the marker is invisible and the CI log shows only
//! `test b4_30_platform_skip_marker ... ok`. NOTE (fix8): no CI step greps
//! that result line today — the macOS sentinel proposed for the B4-30 CI
//! steps was NOT landed, so the skip's visibility on macOS rests on
//! `--nocapture`/failure dumps only (see card 253 in
//! `docs/correctness-open-items/TRACKED_ci_gate_coverage.md`). On every
//! other target this file compiles to nothing.
#![cfg(all(target_os = "macos", feature = "alloc-global", feature = "internals"))]
#[test]
fn b4_30_platform_skip_marker() {
    // eprintln so the marker is at least visible in `--nocapture` runs and
    // in captured-output dumps of failures; no CI step greps it (see above).
    eprintln!("[b4-30] SKIP macos (budgets are x86_64 windows/linux evidence)");
}
