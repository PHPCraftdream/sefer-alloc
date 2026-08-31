//! P1-1 negative compile-fail regression (Sol-codex run-3, task tis-sc3-P1-1
//! #1765): the review's minimal safe-Rust double-issue repro — TWO
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
//! there. The runner therefore reports a missing fixture manifest and
//! SKIPS (returns successfully) instead of failing the packaged suite.
//! From a full git checkout (this workspace, CI) the fixtures exist and
//! every assertion below runs exactly as before.
//!
//! # Why hand-rolled and not `trybuild`
//!
//! This workspace has declined a `trybuild` dev-dependency FIVE separate
//! times, each documented in-source:
//! `crates/tagged-index-stack/tests/stack_unit.rs`'s compile-fail note,
//! `crates/sefer-region/tests/handle_static_asserts.rs`,
//! `crates/aligned-vmem/tests/smoke.rs`, root
//! `tests/r31_4_reserved_small_segment_handle.rs`, and root
//! `tests/r34_3_internals_boundary_api.rs`. This test follows the established
//! alternative (see `tests/r34_3_internals_boundary_api.rs` and
//! `examples/sol_f1_dbg_carve_batch_negative_probe.rs` plus
//! `scripts/verify-internals-negative-boundary.mjs`): an out-of-process
//! `cargo build` of a deliberately-broken fixture, asserted to FAIL.
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
//! `tests/r34_3_internals_boundary_api.rs` itself documents.
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
