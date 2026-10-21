//! B4-30 platform SKIP marker. On macOS the budget test file
//! (`b4_30_first_alloc_min_stack.rs`) compiles to nothing via its file-level
//! `cfg`, so there is no measurement and no failure — this tiny file emits a
//! MARKED green (a deliberate skip, not an unmarked vacuous pass). The
//! marker text goes to test stdout/stderr, which libtest CAPTURES: without
//! `--nocapture` the CI log shows only `test b4_30_platform_skip_marker ...
//! ok`, so the macOS CI step greps that result line (see the B4-30 CI
//! proposal), not the marker text. On every other target this file compiles
//! to nothing.
#![cfg(all(target_os = "macos", feature = "alloc-global", feature = "internals"))]
#[test]
fn b4_30_platform_skip_marker() {
    // eprintln so the marker is at least visible in `--nocapture` runs and
    // in captured-output dumps of failures; CI greps the result-line
    // sentinel above because plain stdout is captured.
    eprintln!("[b4-30] SKIP macos (budgets are x86_64 windows/linux evidence)");
}
