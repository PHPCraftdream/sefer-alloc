//! R2-19 (docs/reviews/2026-09-22-120730-src-review-xa-round-2.md, task
//! #2021): plain `production` must NOT transitively compile aligned-vmem's
//! test-only fault-injection into the release commit path.
//!
//! Mechanism guarded here: before R2-19 the feature chain was
//! `production -> primordial-lazy-commit -> aligned-vmem/fault-injection`
//! (and `small-segment-lazy-commit` carried the same edge), so EVERY real
//! commit through `os::commit_pages` -> `aligned_vmem::try_commit_range`
//! paid an uncontended acquire of vmem's process-global
//! `FAULT_STATE: Mutex<_>` test hook — even with nothing armed (the
//! `target` check happens under the lock). The fix moved that dependency
//! edge onto the dedicated `lazy-commit-fault-injection` fixture feature,
//! which is now the ONLY route to `aligned-vmem/fault-injection` in the
//! manifest.
//!
//! This file is the review's acceptance criteria in executable form. The
//! graph legs deliberately spawn fresh `cargo tree` resolutions instead of
//! scanning Cargo.toml text: a feature-unification answer must come from
//! the resolver, and both legs are INDEPENDENT of whichever feature set the
//! surrounding `cargo test` row was built with (the child process resolves
//! from scratch), so the release-graph assertion holds in every
//! configuration. GENUINE COUNTERFACTUAL: on the pre-R2-19 wiring the
//! first assertion fails — `cargo tree -e features,no-dev -i aligned-vmem
//! --features production` prints an
//! `aligned-vmem feature "fault-injection"` node reached from
//! `sefer-alloc feature "primordial-lazy-commit"` (verified on the
//! pre-fix tree) — while the second passes both before and after, which is
//! the point: the hooks stay reachable for tests instead of being deleted.
//!
//! Source-scan style per `tests/ci_clippy_matrix_consistency.rs` /
//! `tests/no_stale_doc_references.rs` (targeted checks over small,
//! human-authored surfaces, not a general parser). Cost: exactly two
//! `cargo tree` invocations per test-binary run.

use std::fs;
use std::path::Path;
use std::process::Command;

/// `cargo tree -e features,no-dev -i aligned-vmem` with `extra` appended
/// (the feature selection), run in the manifest dir. Panics with cargo's
/// stderr on a non-zero exit.
fn inverted_vmem_feature_tree(extra: &[&str]) -> String {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .current_dir(manifest_dir)
        .args(["tree", "-e", "features,no-dev", "-i", "aligned-vmem"])
        .args(extra)
        .output()
        .expect("failed to spawn `cargo tree`");
    assert!(
        output.status.success(),
        "`cargo tree {extra:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Acceptance criteria 1+2 (R2-19): the ordinary `production` graph has no
/// fault-injection, while the dedicated fixture feature still reaches it.
#[test]
fn release_graph_has_no_fault_injection_and_test_graph_keeps_it() {
    let release = inverted_vmem_feature_tree(&["--features", "production"]);
    assert!(
        !release.contains("feature \"fault-injection\""),
        "plain `production`'s resolved graph contains aligned-vmem's \
         test-only `fault-injection` feature (R2-19 regression: the \
         test-only FAULT_STATE mutex would be compiled into every real \
         commit path); inverted feature tree:\n{release}"
    );

    let fixture = inverted_vmem_feature_tree(&["--features", "lazy-commit-fault-injection"]);
    assert!(
        fixture.contains("feature \"fault-injection\""),
        "the dedicated `lazy-commit-fault-injection` fixture feature no \
         longer reaches aligned-vmem's hooks (R2-19 contrast leg: keep the \
         test seam reachable without adding it to `production`); \
         inverted feature tree:\n{fixture}"
    );
}

/// Acceptance criterion 3 (R2-19 instrumentation leg): on vmem's real
/// commit path, the ONLY consult of the fault hook — the only commit-path
/// taker of the `FAULT_STATE` lock (`should_fail_commit` in
/// `crates/aligned-vmem/src/fault_injection.rs`) — sits directly under the
/// `#[cfg(feature = "fault-injection")]` gate, and the hook module itself
/// is gated the same way. Combined with the release-graph assertion above
/// (feature absent => module and call site are compiled out entirely, per
/// vmem's own "zero cost when disabled" contract), this confirms no
/// `FAULT_STATE` lock is taken on the release commit path.
#[test]
fn commit_path_fault_hook_call_site_is_cfg_gated() {
    let vmem_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/aligned-vmem/src");

    let commit_range =
        fs::read_to_string(vmem_src.join("api/commit_range.rs")).expect("read commit_range.rs");
    const CALL: &str = "if fault_injection::should_fail_commit() {";
    assert_eq!(
        commit_range.matches(CALL).count(),
        1,
        "commit_range.rs must consult the commit fault hook exactly once"
    );
    let lines: Vec<&str> = commit_range.lines().collect();
    let call_line = lines
        .iter()
        .position(|line| line.contains(CALL))
        .expect("hook call site present (checked above)");
    assert!(
        call_line > 0 && lines[call_line - 1].trim() == "#[cfg(feature = \"fault-injection\")]",
        "the commit-path fault hook call site must be gated directly by \
         #[cfg(feature = \"fault-injection\")] so it is compiled out — not \
         merely disarmed — when the feature is absent (R2-19)"
    );

    let lib = fs::read_to_string(vmem_src.join("lib.rs")).expect("read lib.rs");
    const MODULE: &str = "\
#[cfg(feature = \"fault-injection\")]
#[cfg_attr(docsrs, doc(cfg(feature = \"fault-injection\")))]
pub mod fault_injection;";
    assert!(
        lib.contains(MODULE),
        "vmem's fault_injection module must stay wholly behind \
         #[cfg(feature = \"fault-injection\")] (R2-19: zero-cost-when-off \
         is what makes the release-graph assertion above sufficient)"
    );
}
