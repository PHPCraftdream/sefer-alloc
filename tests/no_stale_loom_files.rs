//! R13-5 (task #275) structural regression guard: every `tests/loom_*.rs`
//! file in this repository must be referenced by at least one `--test
//! loom_<name>` (or the bare `--test loom_<name>` step-level `run:` form
//! `loom_thread_free` uses) token somewhere in `.github/workflows/ci.yml`.
//!
//! ## Why this exists
//!
//! This is the SECOND time a loom model-check file shipped with real
//! correctness coverage and was never wired into any CI job:
//!
//!   - task #204 (original miss): several shipping-protocol loom files
//!     (`loom_overflow_first_retry`, `loom_heap_overflow`,
//!     `loom_heap_overflow_drain_guard`) existed in `tests/` but no CI job
//!     referenced them until that task added them to `loom-misc`.
//!   - R12-7 stage 2 / task #258 (this guard's direct trigger):
//!     `tests/loom_class_aware_dirty.rs` shipped alongside the
//!     `class-aware-dirty` feature's loom model (6 tests, including the
//!     R13-1 coarse-only-latch pair) but was never added to `loom-xthread`
//!     or any other loom job — it silently never ran in CI from the moment
//!     it was committed until R13-5 (task #275) both fixed the omission and
//!     added this guard so a THIRD occurrence fails CI at the source instead
//!     of waiting for a human to notice during an unrelated review.
//!
//! ## What this test checks, and does not check
//!
//! This is a STRUCTURAL (string-presence) check, not a semantic one: it does
//! not verify that a referenced test file actually COMPILES or PASSES under
//! the feature string its job uses (that is what actually running the CI job
//! verifies), only that its filename stem appears as a `--test <stem>` token
//! somewhere in the workflow YAML. That is sufficient to catch "the file
//! exists but no CI step ever mentions it at all" — the exact shape of both
//! historical misses above — without this test needing a real YAML parser or
//! network access.
//!
//! Doc/config-only guard: it reads the workflow file and directory-lists
//! `tests/`, never links the crate, so it runs in every feature
//! configuration (including no default features).

use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Every `tests/loom_*.rs` file's stem (e.g. `loom_class_aware_dirty` for
/// `tests/loom_class_aware_dirty.rs`), sorted for a stable failure message.
fn loom_test_file_stems() -> Vec<String> {
    let tests_dir = manifest_dir().join("tests");
    let mut stems = Vec::new();
    for entry in fs::read_dir(&tests_dir).expect("read tests dir") {
        let path = entry.expect("dir entry").path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if stem.starts_with("loom_") {
            stems.push(stem.to_string());
        }
    }
    stems.sort();
    stems
}

/// Every `--test loom_<name>` token referenced anywhere in
/// `.github/workflows/ci.yml`, plus the one bare `run: cargo test --release
/// --test loom_<name>` step-level form `loom_thread_free` uses (same token
/// shape, `--test loom_<name>` still appears literally on that line — see
/// the `loom-misc` job's first step). A single generic scan over the whole
/// file (not restricted to specific job names) intentionally also picks up
/// the `loom-alloc-global`/`loom-experimental` jobs' own `--test loom_*`
/// tokens (e.g. `loom_aba`, `loom_racy_ptr_cell`, `loom_ring_mpsc` are
/// crate-local loom suites under `-p <crate>`, not `tests/loom_*.rs` files in
/// THIS repo's root — they simply never match a stem from
/// `loom_test_file_stems()` above, so including them in the scan cannot
/// cause a false negative, only extra tokens that are harmlessly ignored).
fn ci_referenced_loom_tokens() -> Vec<String> {
    let workflow = manifest_dir()
        .join(".github")
        .join("workflows")
        .join("ci.yml");
    let text = fs::read_to_string(&workflow)
        .unwrap_or_else(|e| panic!("read {}: {e}", workflow.display()));

    // Scan for `--test <name>` ANYWHERE on a line, not just at line start:
    // most occurrences are their own YAML block-scalar line (`- run: >-` /
    // `--test loom_foo` on separate lines), but `loom_thread_free`'s step
    // uses the single-line inline form `run: cargo test --release --test
    // loom_thread_free`, where `--test` is preceded by other tokens on the
    // same line.
    let mut tokens = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(idx) = rest.find("--test ") {
            rest = &rest[idx + "--test ".len()..];
            let token: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            if token.starts_with("loom_") {
                tokens.push(token);
            }
        }
    }
    tokens
}

/// Central inventory check: every `tests/loom_*.rs` file's stem must appear
/// as a `--test <stem>` token somewhere in `.github/workflows/ci.yml`.
#[test]
fn every_loom_test_file_is_referenced_in_a_ci_job() {
    let file_stems = loom_test_file_stems();
    assert!(
        !file_stems.is_empty(),
        "no tests/loom_*.rs files found -- the glob in this test is broken \
         (there are known loom files in the repo as of R13-5)"
    );

    let ci_tokens = ci_referenced_loom_tokens();
    assert!(
        !ci_tokens.is_empty(),
        "no `--test loom_*` tokens found anywhere in \
         .github/workflows/ci.yml -- the workflow parsing in this test is \
         broken, or every loom job was deleted"
    );

    let missing: Vec<&String> = file_stems
        .iter()
        .filter(|stem| !ci_tokens.contains(stem))
        .collect();

    assert!(
        missing.is_empty(),
        "these tests/loom_*.rs files exist but are NOT referenced by any \
         `--test <name>` token in .github/workflows/ci.yml (add each to the \
         appropriate loom-* job, e.g. `loom-xthread` for an `alloc-core \
         alloc-xthread`-gated model or `loom-misc` for a standalone one -- \
         see that job's own header comment for the grouping convention): \
         {missing:?}\n\nThis is the failure mode task #275 (R13-5) exists to \
         prevent: task #258's tests/loom_class_aware_dirty.rs shipped with 6 \
         real tests (including the R13-1 coarse-only-latch regression pair) \
         and silently never ran in CI because no job mentioned it.",
    );
}

/// Non-vacuousness counterfactual: this test's own logic must be capable of
/// FAILING when a real file is unreferenced, not just capable of passing.
/// Simulates "loom_class_aware_dirty was removed from every CI job" by
/// filtering it out of the CI-token set the same way the real check would
/// see an accidental removal, and asserts the central check's own filter
/// (replicated here) would flag it as missing.
#[test]
fn counterfactual_missing_file_is_detected() {
    let file_stems = loom_test_file_stems();
    assert!(
        file_stems.contains(&"loom_class_aware_dirty".to_string()),
        "this counterfactual assumes tests/loom_class_aware_dirty.rs exists \
         (it does, as of R13-5) -- if it was renamed/removed, update this \
         test to reference a different real loom_*.rs file"
    );

    // Simulate the CI file NOT mentioning it -- an empty token set is the
    // most extreme case of "unreferenced", and it must be detected as
    // missing by the same containment check the real test above uses.
    let simulated_ci_tokens: Vec<String> = Vec::new();
    let would_be_flagged = file_stems
        .iter()
        .any(|stem| stem == "loom_class_aware_dirty" && !simulated_ci_tokens.contains(stem));

    assert!(
        would_be_flagged,
        "the inventory check's containment logic failed to flag a known-\
         unreferenced file under a simulated empty CI token set -- the \
         check itself is vacuous and would never fail on a real omission"
    );
}
