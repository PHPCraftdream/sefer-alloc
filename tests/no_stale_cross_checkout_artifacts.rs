//! Guard against STALE CROSS-CHECKOUT CARGO ARTIFACTS: a test binary compiled
//! in one git worktree of this repo being silently replayed by cargo in
//! another (task #1073).
//!
//! The trap: this machine sets a user-global `CARGO_TARGET_DIR`
//! (`D:\dev\rust\.cargo-target`, HKCU user env var), so EVERY git worktree of
//! this repo shares ONE cargo artifact cache. Tests like
//! `tests/ci_clippy_matrix_consistency.rs` read repo files through
//! `Path::new(env!("CARGO_MANIFEST_DIR"))` — rustc bakes that path INTO the
//! test binary at compile time (line 43 there is the canonical example, and
//! the idiom is CORRECT Rust; the defect is artifact reuse, not the idiom).
//! A binary compiled in worktree A and replayed by cargo in worktree B then
//! fails in one of two directions: (a) FALSE RED — worktree A since deleted,
//! every file-reading sibling panics with a misleading `Os { code: 3, kind:
//! NotFound }` error (this literally happened: `npm run check` failed at
//! `test (--features pinning)` with `read scripts/check-matrix.mjs: Os {
//! code: 3, kind: NotFound, ... }` from
//! `tests/ci_clippy_matrix_consistency.rs:239:10`); or (b) FALSE GREEN —
//! worktree A still exists, and the tests silently validate ITS files instead
//! of the current tree's (the same class as task #1071's cache-replay hole,
//! with the sign flipped).
//!
//! The guard's mechanism (verified empirically): cargo runs test binaries
//! with the process cwd set to the package root — the manifest dir of the
//! INVOKING checkout — regardless of the directory cargo was invoked from,
//! so a runtime comparison of `env!("CARGO_MANIFEST_DIR")` with
//! `current_dir()` detects ANY foreign-built test binary, whichever direction
//! and whatever cargo freshness quirk let it replay. Remediation when this
//! test fires: rebuild from THIS checkout — `touch` the affected test source
//! (e.g. this file) or `cargo clean -p sefer-alloc` — then re-run.
//!
//! Companion output sniffer: `scripts/stale-artifact-diagnosis.mjs` (wired
//! into `scripts/check-all.mjs`'s failure path plus a `--self-test` gate
//! step). This is the root-crate complement of task #1071's aligned-vmem
//! cache-replay guards. Task #1073.

use std::fs;
use std::path::Path;

#[test]
fn baked_manifest_dir_matches_running_tree() {
    let baked = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cwd = std::env::current_dir().expect("current_dir() of the test process");

    // Deleted-worktree direction first: a baked dir that no longer exists
    // cannot be canonicalized, so this branch must run BEFORE the
    // canonicalize calls below (which would otherwise fail with their own —
    // far less explanatory — NotFound error).
    if !baked.exists() {
        panic!(
            "STALE CROSS-CHECKOUT CARGO ARTIFACT (deleted source checkout): this test \
             binary was compiled with CARGO_MANIFEST_DIR baked in as {baked:?}, but that \
             directory no longer exists — the binary was built in a git worktree that has \
             since been removed, and the shared CARGO_TARGET_DIR ({}) let cargo replay it \
             in this checkout ({cwd:?}). Every sibling test that reads repo files through \
             env!(\"CARGO_MANIFEST_DIR\") will fail with a misleading `Os {{ code: 3, kind: \
             NotFound }}` panic (e.g. the task #1073 `read scripts/check-matrix.mjs` \
             failure). Fix: rebuild from THIS checkout — `touch` a test source (e.g. this \
             file) or `cargo clean -p sefer-alloc` — then re-run. See task #1073 and this \
             file's module doc.",
            std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "<unset>".to_string()),
        );
    }

    // canonicalize on both sides: it resolves symlinks/junctions and, on
    // Windows, normalizes both paths to the same `\\?\` verbatim prefix, so
    // the equality below is a true identity check rather than a string-shape
    // lottery between the baked and runtime path spellings.
    let baked_real = fs::canonicalize(baked).expect("canonicalize baked CARGO_MANIFEST_DIR");
    let cwd_real = fs::canonicalize(&cwd).expect("canonicalize test-process cwd");
    assert_eq!(
        baked_real,
        cwd_real,
        "STALE CROSS-CHECKOUT CARGO ARTIFACT (foreign source checkout): this test binary \
         was compiled in {baked:?} but is running in {cwd:?}. The shared CARGO_TARGET_DIR \
         ({}) let cargo replay a binary built by a DIFFERENT checkout of this repo — every \
         file-reading test in this binary is silently validating the WRONG tree (a false \
         green), not this one. Fix: rebuild from THIS checkout — `touch` a test source \
         (e.g. this file) or `cargo clean -p sefer-alloc` — then re-run. See task #1073 \
         and this file's module doc.",
        std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "<unset>".to_string()),
    );
}
