//! P3-6 negative compile-fail regression (Sol-codex run-3, task
//! tis-sc3-Group4 #1774): `TaggedIndex`'s `INDEX_BITS` const-generic
//! bounds must stay enforced — a `TaggedIndex<0>` and a `TaggedIndex<17>`
//! must NOT compile.
//!
//! The fixtures at `tests/compile_fail/index_bits_zero/src/main.rs` and
//! `tests/compile_fail/index_bits_seventeen/src/main.rs` each read the
//! associated constant of an out-of-range instantiation (`INDEX_BITS` of 0
//! and 17 respectively). Both must fail with **E0080** carrying the crate's
//! own assert message: `INDEX_BITS must be in 1..=16: the tag half must keep
//! at least 48 bits ...` (the `_CHECK_BITS` assert's minimum-48-bit-tag /
//! sentinel argument).
//!
//! # Published-package behavior (round-10 @oh review, P2-1)
//!
//! The fixture crates under `tests/compile_fail/` are git-checkout-only
//! test infrastructure: each has its own `Cargo.toml`, so cargo's
//! packaging rule auto-excludes them from the published `.crate` (now
//! stated explicitly via the `[package]` `exclude` in `Cargo.toml`). This
//! driver file itself IS packaged (a plain `.rs` file directly under
//! `tests/`), so `cargo test` inside a downloaded package reaches it —
//! but not usefully as a compile-fail test: the fixtures are simply not
//! there. The runner therefore reports a missing fixture manifest and
//! SKIPS (returns successfully) instead of failing the packaged suite.
//! From a full git checkout (this workspace, CI) the fixtures exist and
//! every assertion below runs exactly as before.
//!
//! # Why the assertion names the range requirement
//!
//! Asserting only "the build failed" would be vacuous: the fixture could
//! fail for an UNRELATED reason (a syntax error, a dependency issue, an
//! inherited `--cfg loom` firing the crate's loom-cfg `compile_error!`)
//! and the test would still pass. Naming `E0080` and the exact range
//! message pins that the failure is the bounds check itself.
//!
//! # Why hand-rolled and not `trybuild`
//!
//! This workspace has declined a `trybuild` dev-dependency FIVE separate
//! times; see the full rationale in
//! `tests/compile_fail_two_backings.rs` ("Why hand-rolled and not
//! `trybuild`"). This runner follows the same established alternative: an
//! out-of-process `cargo build` of a deliberately-broken fixture, asserted
//! to FAIL with a SPECIFIC error. (Sol-codex run-3 P3-6, task
//! tis-sc3-Group4 #1774.)
//!
//! # RUSTFLAGS stripping
//!
//! The child env REMOVES `RUSTFLAGS` and `CARGO_ENCODED_RUSTFLAGS`: an
//! inherited `--cfg loom` (e.g. under `RUSTFLAGS="--cfg loom" cargo test`)
//! would make the fixture fail for the WRONG reason — the crate's
//! `--cfg loom`-without-feature `compile_error!` fires instead of the
//! `_CHECK_BITS` E0080 — so the assertions below would pass even if the
//! bounds check had regressed. The same hazard the two_backings runner
//! documents in its "RUSTFLAGS stripping" section.
#![cfg(not(loom))]

use std::path::PathBuf;
use std::process::Command;

/// Builds the named index-bits fixture and asserts it fails with the
/// `_CHECK_BITS` E0080 range requirement.
fn assert_index_bits_fixture_must_not_compile(fixture_dir: &str) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join(fixture_dir)
        .join("Cargo.toml");
    // P2-1 (round-10 @oh review): the fixture crates are excluded from the
    // published .crate (cargo's nested-manifest rule; see the `[package]`
    // `exclude` in Cargo.toml), so in a downloaded package the manifest is
    // legitimately absent — report and skip rather than fail the packaged
    // suite. From a git checkout the fixture exists and everything below
    // runs unchanged.
    if !manifest.exists() {
        eprintln!(
            "skipping: compile-fail fixture not present ({}) — fixture \
             crates are git-checkout-only test infrastructure, excluded \
             from the published .crate",
            manifest.display()
        );
        return;
    }
    assert!(
        manifest.exists(),
        "compile-fail fixture manifest missing: {}",
        manifest.display()
    );

    // Cached across runs; target/tmp is gitignored.
    let child_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(fixture_dir);

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(&cargo)
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .expect("failed to spawn cargo for the compile-fail fixture");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let context = format!(
        "fixture: {}\nstatus: {:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        manifest.display(),
        output.status.code()
    );

    assert!(
        !output.status.success(),
        "the out-of-range INDEX_BITS fixture COMPILED — the \
         `INDEX_BITS must be in 1..=16` bounds check regressed:\n{context}"
    );
    assert!(
        stderr.contains("E0080"),
        "expected E0080 (evaluation panicked) in the fixture's compile \
         errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("INDEX_BITS must be in 1..=16"),
        "expected the `_CHECK_BITS` range requirement \
         (`INDEX_BITS must be in 1..=16`) in the fixture's compile \
         errors:\n{context}"
    );
}

#[test]
fn index_bits_zero_must_not_compile() {
    assert_index_bits_fixture_must_not_compile("index_bits_zero");
}

#[test]
fn index_bits_seventeen_must_not_compile() {
    assert_index_bits_fixture_must_not_compile("index_bits_seventeen");
}
