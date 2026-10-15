//! R2-01 (independent src review round 2, task #2003) — automated oracle for
//! the compile-fail fixture at
//! `tests/compile_fail/r2_01_segment_bases_outlives_owner/`.
//!
//! A compile-FAIL property cannot be asserted by a normal `#[test]`: a test
//! binary that fails to compile never runs, so the assertion "this code must
//! not compile" has to be made from a process that compiles fine and then
//! invokes `cargo build` on the fixture as a CHILD process — the same
//! out-of-process technique `tests/tagged_index_stack_compile_fail.rs` uses
//! (see that file's own module doc for the full rationale).
//!
//! ## Why a standalone fixture crate, not a root `[[example]]`
//!
//! Unlike `examples/sol_f1_dbg_carve_batch_negative_probe.rs` (which only
//! fails to compile under a NARROW feature combination that happens to never
//! coincide with any real per-PR check-matrix row's `--all-targets` sweep),
//! this probe fails UNCONDITIONALLY the moment `alloc-global` is enabled —
//! there is no feature combination under which it compiles. A root
//! `[[example]]` with `required-features = ["alloc-global"]` was tried first
//! and DID break multiple real rows
//! (`clippy --all-features`, `clippy --features "hardened medium-classes
//! internals"`, `clippy --features "production internals"`, and several
//! `cargo test` invocations that also build examples by default) because
//! `--all-targets`/`cargo test` unconditionally attempt to build any example
//! whose `required-features` are satisfied — there is no feature string that
//! is BOTH "satisfied only by my dedicated test's own invocation" and
//! "never satisfied by any real row" using the crate's existing features
//! alone. A standalone fixture crate under `tests/compile_fail/` — carrying
//! its own empty `[workspace]` table so Cargo's workspace/target discovery
//! never reaches it, mirroring
//! `crates/tagged-index-stack/tests/compile_fail/*/Cargo.toml` — sidesteps
//! this entirely: it is never part of `sefer-alloc`'s own target set, so no
//! `--all-targets`/`cargo test` invocation on the root crate ever touches it.
//!
//! This oracle proves the FIX, not the bug: before task #2003 added `+ '_'`
//! to `AllocCore::segment_bases`/`SegmentTable::bases`/
//! `HeapCore::segment_bases`, the fixture's exact body compiled cleanly
//! (confirmed manually during that task) — a real use-after-free reachable
//! from safe code. This test pins the POST-fix state: the fixture must fail
//! with specifically **E0597** ("`core` does not live long enough"), not
//! succeed and not fail for some unrelated reason.

use std::path::PathBuf;
use std::process::Command;

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join("r2_01_segment_bases_outlives_owner")
        .join("Cargo.toml")
}

#[test]
fn segment_bases_iterator_outliving_owner_must_not_compile() {
    let manifest = fixture_manifest();
    assert!(
        manifest.is_file(),
        "compile-fail fixture missing from checkout: {}",
        manifest.display()
    );

    // Isolated child target dir (CARGO_TARGET_TMPDIR is a Cargo-provided,
    // per-test-binary temp dir — cached across runs, gitignored), mirroring
    // `tests/support/tagged_index_stack_compile_fail.rs`'s established
    // pattern. RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS stripped so an inherited
    // `--cfg loom` (or similar) cannot make this fixture fail for the WRONG
    // reason.
    let child_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("r2_01_fixture");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(&cargo)
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to spawn `cargo build` for the R2-01 fixture");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "the R2-01 fixture COMPILED — AllocCore::segment_bases' returned \
         iterator no longer captures &self's lifetime (the R2-01/task #2003 \
         UAF fix regressed). stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("E0597"),
        "the R2-01 fixture failed to compile, but NOT with the expected \
         E0597 (\"does not live long enough\") — it may be failing for an \
         unrelated reason. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("does not live long enough"),
        "expected the specific \"does not live long enough\" borrow-check \
         message tied to `core`'s lifetime. stderr:\n{stderr}"
    );
}
