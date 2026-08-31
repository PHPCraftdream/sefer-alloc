//! P1-1 negative compile-fail regression (Sol-codex run-3): the review's
//! minimal safe-Rust double-issue repro — TWO
//! `ArrayLinks` instances used against ONE `StackHead` — must NOT compile
//! against the post-redesign API.
//!
//! The fixture at `tests/compile_fail/two_backings/src/main.rs` is exactly
//! that repro, adapted to the post-redesign type names: two independent
//! `ArrayLinks` backings (`a`, `b`) plus one `StackHead<16>`, with the
//! stack's `push`/`pop` called externally against each. Before the redesign
//! (`StackHead` + per-call `&L: Links` parameters) this compiled and silently
//! double-issued an index from `pop`. The redesign makes the repro
//! UNEXPRESSIBLE: `StackHead` is the head word ONLY — it has no `push`/`pop`
//! methods (those live on `StackOps`, blanket-implemented by the crate for
//! every `StackStorage` implementor, and an implementor owns head AND links
//! in ONE place) — so the fixture must fail to compile with **E0599**
//! ("no method named `push` / `pop` found for struct `StackHead`"). That
//! compile error IS the structural fix itself, not a runtime panic.
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
//! # Why hand-rolled and not `trybuild`
//!
//! This workspace's standing convention is to decline a `trybuild`
//! dev-dependency in favor of hand-rolled compile-fail tests, each decision
//! documented in-source where it was made — do not trust a count quoted in
//! any one comment (the notes accumulate over time): the accurate,
//! mechanically re-derivable way to find them is
//! `grep -rn trybuild --include=*.rs .` from the workspace root. This test
//! follows the established alternative (see root
//! `tests/r34_3_internals_boundary_api.rs` and root
//! `examples/sol_f1_dbg_carve_batch_negative_probe.rs` plus root
//! `scripts/verify-internals-negative-boundary.mjs` — repository files,
//! not part of the published package): an out-of-process `cargo build` of a
//! deliberately-broken fixture, asserted to FAIL. This crate's own
//! `tests/stack_unit.rs` documents the compile-fail coverage that now
//! EXISTS (the `compile_fail_*.rs` drivers, this file included) rather than
//! a decline.
//!
//! # Why not a `compile_fail` doctest
//!
//! This repo bans doctests outright (see CLAUDE.md, "No doctests").
//!
//! # Why an out-of-process cargo build
//!
//! A compile-FAIL property cannot be asserted by a normal `#[test]`: a test
//! binary that fails to compile never runs, so the assertion "this code must
//! not compile" has to be made from a process that compiles fine and then
//! invokes rustc/cargo on the fixture as a CHILD process — the same analysis
//! root `tests/r34_3_internals_boundary_api.rs` itself documents (a
//! repository file, not part of the published package).
//!
//! # RUSTFLAGS stripping
//!
//! Under `RUSTFLAGS="--cfg loom" cargo test`, an inherited `--cfg loom` would
//! make the fixture fail for the WRONG reason: the crate's own
//! `--cfg loom`-without-feature `compile_error!` (see `src/lib.rs`) fires
//! before any method resolution, so the assertions below would pass even if
//! `StackHead` still had `push`/`pop`. The child env therefore REMOVES
//! `RUSTFLAGS` and `CARGO_ENCODED_RUSTFLAGS` so the fixture fails for the
//! RIGHT reason: E0599.
#![cfg(not(loom))]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn two_arraylinks_backings_against_one_stackhead_must_not_compile() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join("two_backings")
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
    let child_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("compile_fail_two_backings");

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
        "the P1-1 double-issue repro COMPILED — the redesign's structural fix \
         regressed (StackHead must have no push/pop):\n{context}"
    );
    assert!(
        stderr.contains("E0599"),
        "expected E0599 (no method named `push`/`pop`) in the fixture's \
         compile errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("no method named `push` "),
        "expected `no method named `push`` on StackHead in the fixture's \
         compile errors:\n{context}"
    );
    assert!(
        stderr.contains("no method named `pop` "),
        "expected `no method named `pop`` on StackHead in the fixture's \
         compile errors:\n{context}"
    );
}
