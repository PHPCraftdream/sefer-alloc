//! P2-4 negative compile-fail regression (Sol-codex run-3, task
//! tis-sc3-Group4 #1770): the crate promises that a `--cfg loom` build
//! WITHOUT its `loom` feature fails fast with ONLY the crate's own named
//! `compile_error!` — `building with --cfg loom requires --features loom
//! (loom is now an optional dependency)`.
//!
//! Since P2-4's fix, the implementation module is cfg'd out entirely under
//! that invalid configuration, so NO secondary name-resolution error may
//! appear alongside the named one. This fixture pins BOTH halves:
//!
//! 1. the named error IS present, and
//! 2. no secondary error (specifically `error[E0433]: cannot find module or
//!    crate \`loom\`` from the loom-aliasing `use`) IS present.
//!
//! That second half is exactly the regression P2-4 reports: before the fix,
//! the build also produced the E0433 unresolved-crate error on top of the
//! `compile_error!`.
//!
//! The fixture at `tests/compile_fail/loom_cfg_without_feature/src/main.rs`
//! deliberately references NO crate items: it only declares the path
//! dependency (so the crate compiles) and its `fn main() {}` is empty — the
//! dependency's `compile_error!` is the only diagnostic expected.
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
//! there. The skip fires ONLY in a packaged context — detected via the
//! `Cargo.toml.orig` `cargo package` writes into every extracted package
//! (absent from any git checkout) — so a fixture that goes missing from a
//! real checkout FAILS LOUD instead (round-11 @oh review, P2-2: a bare
//! `manifest.exists()` guard silently skipped the test in a checkout whose
//! fixture directory had been renamed away, reporting a false `ok`).
//!
//! # Mechanics: the cfg is the point (inverse of the two_backings runner)
//!
//! Unlike `tests/compile_fail_two_backings.rs` — which STRIPS both flag
//! variables so an inherited `--cfg loom` cannot mask the real failure —
//! this fixture is the INVERSE: the `--cfg loom` configuration is the whole
//! point, so the child env SETS `RUSTFLAGS` to the literal `--cfg loom`.
//! It must also `env_remove("CARGO_ENCODED_RUSTFLAGS")`: cargo prefers
//! `CARGO_ENCODED_RUSTFLAGS` over `RUSTFLAGS`, so a merely-empty-or-
//! inherited encoded value silently cancels the override (an empty
//! `CARGO_ENCODED_RUSTFLAGS=""` was observed to suppress the RUSTFLAGS
//! override entirely while developing this test).
//!
//! # Why hand-rolled and not `trybuild`
//!
//! This workspace's standing convention is to decline a `trybuild`
//! dev-dependency in favor of hand-rolled compile-fail tests; see the full
//! rationale in `tests/compile_fail_two_backings.rs` ("Why hand-rolled and
//! not `trybuild`"). Same established alternative here: an out-of-process
//! `cargo build` of a deliberately-broken fixture, asserted to FAIL with a
//! SPECIFIC error.
#![cfg(not(loom))]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn loom_cfg_without_feature_fails_with_only_the_named_error() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join("loom_cfg_without_feature")
        .join("Cargo.toml");
    // P2-1 (round-10 @oh review), skip-guard hardened by round-11 @oh review
    // P2-2: the fixture crates are excluded from the published .crate
    // (cargo's nested-manifest rule; see the `[package]` `exclude` in
    // Cargo.toml), so in a downloaded package the manifest is legitimately
    // absent — report and skip rather than fail the packaged suite. The skip
    // fires ONLY in a packaged context: `cargo package` writes
    // `Cargo.toml.orig` next to the manifest in every extracted package, and
    // that file never exists in a git checkout — so a fixture missing from a
    // REAL checkout (a bad rename, an accidental deletion) fails loud here
    // instead of silently reporting `ok` with the regression untested.
    let packaged = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("Cargo.toml.orig")
        .exists();
    if !manifest.exists() {
        assert!(
            packaged,
            "compile-fail fixture missing from a git checkout: {}",
            manifest.display()
        );
        eprintln!(
            "skipping: compile-fail fixture not present ({}) — fixture \
             crates are git-checkout-only test infrastructure, excluded \
             from the published .crate",
            manifest.display()
        );
        return;
    }

    // Cached across runs; target/tmp is gitignored.
    let child_target =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("compile_fail_loom_cfg_without_feature");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(&cargo)
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target)
        // The cfg is the point here (inverse of the two_backings runner).
        // cargo prefers CARGO_ENCODED_RUSTFLAGS over RUSTFLAGS, so the
        // encoded variable must go or it silently cancels the override.
        .env("RUSTFLAGS", "--cfg loom")
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
        "the `--cfg loom` WITHOUT `--features loom` fixture COMPILED — the \
         crate's fast-fail compile_error! regressed:\n{context}"
    );
    assert!(
        stderr.contains(
            "building with --cfg loom requires --features loom \
             (loom is now an optional dependency)"
        ),
        "expected the crate's exact named compile_error! text in the \
         fixture's compile errors — it failed for some OTHER reason:\n\
         {context}"
    );
    assert!(
        !stderr.contains("E0433"),
        "expected NO secondary name-resolution error (E0433) alongside the \
         named compile_error! — the P2-4 regression (implementation module \
         not fully cfg'd out under the invalid configuration):\n{context}"
    );
    assert!(
        !stderr.contains("cannot find module or crate `loom`"),
        "expected NO `cannot find module or crate `loom`` error alongside \
         the named compile_error! — the P2-4 regression:\n{context}"
    );
}
